use tokio::sync::mpsc;

use super::Agent;
use super::events::{AppEvent, UserAction};
use crate::agent::modes::AwakeMode;
use crate::config::{self, Config};
use crate::usage::{self, restore_current_usage};

#[tracing::instrument(skip(config, user_rx, event_tx, debug_tx), fields(
    model = %config.llm.default_model_name(),
    data_dir = %config.state.data_dir().display(),
))]
pub async fn run(
    config: crate::config::Config,
    mut user_rx: mpsc::Receiver<UserAction>,
    event_tx: mpsc::Sender<AppEvent>,
    debug_tx: Option<tokio::sync::watch::Sender<crate::debug::DebugSnapshot>>,
) -> anyhow::Result<()> {
    let mut agent = Agent::<AwakeMode>::awake(config, debug_tx)?;

    tracing::info!(
        history_messages = agent.history.messages().len(),
        debug_enabled = agent.debug_tx.is_some(),
        "agent initialized"
    );

    let entries = agent.history.to_display_entries();
    let model_name = agent.config.llm.default_model_name().to_string();
    let usage_stats_path = agent.config.state.data_dir().join("usage.jsonl");
    let current_usage = restore_current_usage(&usage_stats_path);

    let _ = event_tx
        .send(AppEvent::Init {
            entries,
            current_usage,
            model_name: model_name.clone(),
        })
        .await;

    if let Err(e) = agent.initialize().await {
        let _ = event_tx
            .send(AppEvent::Error {
                message: format!("Initialization failed: {}", e),
            })
            .await;
        return Ok(());
    }

    agent.emit_debug_snapshot();

    tracing::info!("entering agent main loop");
    run_loop(agent, &mut user_rx, &event_tx, usage_stats_path, model_name).await;

    tracing::info!("agent run complete");
    Ok(())
}

#[tracing::instrument(skip(agent, user_rx, event_tx))]
async fn run_loop(
    mut agent: Agent<AwakeMode>,
    user_rx: &mut mpsc::Receiver<UserAction>,
    event_tx: &mpsc::Sender<AppEvent>,
    usage_stats_path: std::path::PathBuf,
    model_name: String,
) {
    let mut action_count: u64 = 0;
    while let Some(action) = user_rx.recv().await {
        action_count += 1;
        tracing::debug!(action = ?action, seq = action_count, "received user action");
        match action {
            UserAction::SendMessage { content } => {
                tracing::info!(
                    content_len = content.len(),
                    msg_preview = %crate::agent::truncate_str(&content, 80),
                    "user message received"
                );
                agent.push_user_message(&content);
                agent.reset_iteration();
                let _ = agent.step(event_tx).await;
                agent.save_if_dirty().await;

                if agent.should_sleep() {
                    tracing::info!("fatigue threshold reached, transitioning to sleep");
                    let _ = event_tx.send(AppEvent::FallingAsleep).await;
                    let snapshot_ts = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
                    let sleeping = agent.transition_to_sleeping(snapshot_ts.clone());
                    agent = sleeping.run_sleep_cycle(event_tx, snapshot_ts).await;
                }
            }
            UserAction::ForceSleep => {
                tracing::info!("forced sleep requested");
                let _ = event_tx.send(AppEvent::FallingAsleep).await;
                let snapshot_ts = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
                let sleeping = agent.transition_to_sleeping(snapshot_ts.clone());
                agent = sleeping.run_sleep_cycle(event_tx, snapshot_ts).await;
            }
            UserAction::RebuildEmbeddings => {
                tracing::info!("rebuilding embeddings requested");
                let _ = event_tx
                    .send(AppEvent::TextDelta {
                        delta: "Rebuilding embeddings...\n".to_string(),
                    })
                    .await;
                match agent.rebuild_embeddings().await {
                    Ok(count) => {
                        tracing::info!(count, "embeddings rebuilt");
                        let _ = event_tx
                            .send(AppEvent::TextDone {
                                full_text: format!("Rebuilt {} episode embedding(s).", count),
                            })
                            .await;
                    }
                    Err(e) => {
                        let _ = event_tx
                            .send(AppEvent::Error {
                                message: format!("Rebuild failed: {}", e),
                            })
                            .await;
                    }
                }
                let _ = event_tx.send(AppEvent::TurnComplete).await;
            }
            UserAction::ApprovePending => {
                let _ = agent.approve_and_step(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::ApproveAlways {
                tool,
                target,
                persist,
            } => {
                tracing::info!(
                    "ApproveAlways: tool={}, target={}, persist={}",
                    tool,
                    target,
                    persist
                );
                agent.approve_permission(&tool, target, persist);
                let _ = agent.approve_and_step(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::RejectTool { feedback } => {
                let _ = agent
                    .reject_and_step(feedback.as_deref().unwrap_or(""), event_tx)
                    .await;
                agent.save_if_dirty().await;
            }
            UserAction::UsageStats => {
                let sessions = usage::load_session_summaries(&usage_stats_path);
                let _ = event_tx.send(AppEvent::UsageStatsLoaded { sessions }).await;
            }
            UserAction::RequestInit => {
                let entries = agent.history.to_display_entries();
                let current_usage = restore_current_usage(&usage_stats_path);
                let _ = event_tx
                    .send(AppEvent::Init {
                        entries,
                        current_usage,
                        model_name: model_name.clone(),
                    })
                    .await;
            }
            UserAction::LoadConfig => {
                tracing::info!("loading config");
                let path = config::config_path();
                let raw_res = tokio::task::spawn_blocking(move || {
                    if path.exists() {
                        std::fs::read_to_string(&path).map_err(|e| e.to_string())
                    } else {
                        let default_config = Config::default();
                        toml::to_string_pretty(&default_config).map_err(|e| e.to_string())
                    }
                })
                .await
                .unwrap_or(Err("spawn_blocking failed".to_string()));
                match raw_res {
                    Ok(raw) => {
                        let _ = event_tx.send(AppEvent::ConfigLoaded { toml: raw }).await;
                    }
                    Err(e) => {
                        let _ = event_tx
                            .send(AppEvent::Error {
                                message: format!("Failed to load config: {}", e),
                            })
                            .await;
                    }
                }
            }
            UserAction::SaveConfig { toml_str } => {
                tracing::info!("saving config");
                match Config::parse_toml(&toml_str) {
                    Ok(_) => {
                        let path = config::config_path();
                        let write_result =
                            tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                                if let Some(parent) = path.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                }
                                let tmp = path.with_extension("toml.tmp");
                                std::fs::write(&tmp, &toml_str)?;
                                if let Err(e) = std::fs::rename(&tmp, &path) {
                                    let _ = std::fs::remove_file(&tmp);
                                    return Err(e);
                                }
                                Ok(())
                            })
                            .await
                            .unwrap_or(Err(std::io::Error::other("spawn_blocking failed")));
                        match write_result {
                            Ok(()) => {
                                let _ = event_tx
                                    .send(AppEvent::ConfigSaved {
                                        message: "Saved to config.toml (restart to apply)"
                                            .to_string(),
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = event_tx
                                    .send(AppEvent::Error {
                                        message: format!("Config save failed: {}", e),
                                    })
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = event_tx
                            .send(AppEvent::Error {
                                message: format!("Config validation failed: {}", e),
                            })
                            .await;
                    }
                }
            }
            UserAction::GetDataDir => {
                let path = config::data_base_dir().to_string_lossy().to_string();
                let _ = event_tx.send(AppEvent::DataDir { path }).await;
            }
            UserAction::Quit => {
                tracing::info!(action_count, "agent shutting down");
                break;
            }
            UserAction::Undo => {
                tracing::info!("undo requested");
                if agent.history.undo_last_exchange() {
                    agent.save_if_dirty().await;
                    let entries = agent.history.to_display_entries();
                    let _ = event_tx.send(AppEvent::UndoDone { entries }).await;
                } else {
                    let _ = event_tx
                        .send(AppEvent::Error {
                            message: "Nothing to undo.".to_string(),
                        })
                        .await;
                }
            }
        }
    }

    agent.shutdown().await;
}
