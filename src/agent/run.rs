use tokio::sync::mpsc;

use super::Agent;
use super::events::{AppEvent, UserAction};
use crate::agent::modes::AwakeMode;
use crate::usage::{self, restore_current_usage};

pub async fn run(
    config: crate::config::Config,
    mut user_rx: mpsc::Receiver<UserAction>,
    event_tx: mpsc::Sender<AppEvent>,
    debug_tx: Option<tokio::sync::watch::Sender<crate::debug::DebugSnapshot>>,
) -> anyhow::Result<()> {
    let mut agent = Agent::<AwakeMode>::awake(config, debug_tx)?;

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

    run_loop(agent, &mut user_rx, &event_tx, usage_stats_path, model_name).await;

    Ok(())
}

async fn run_loop(
    mut agent: Agent<AwakeMode>,
    user_rx: &mut mpsc::Receiver<UserAction>,
    event_tx: &mpsc::Sender<AppEvent>,
    usage_stats_path: std::path::PathBuf,
    model_name: String,
) {
    while let Some(action) = user_rx.recv().await {
        match action {
            UserAction::SendMessage { content } => {
                agent.push_user_message(&content);
                agent.reset_iteration();
                let _ = agent.step(event_tx).await;
                agent.save_if_dirty().await;

                if agent.should_sleep() {
                    let _ = event_tx.send(AppEvent::FallingAsleep).await;
                    let snapshot_ts = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
                    let sleeping = agent.transition_to_sleeping(snapshot_ts.clone());
                    agent = sleeping.run_sleep_cycle(event_tx, snapshot_ts).await;
                }
            }
            UserAction::ForceSleep => {
                let _ = event_tx.send(AppEvent::FallingAsleep).await;
                let snapshot_ts = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
                let sleeping = agent.transition_to_sleeping(snapshot_ts.clone());
                agent = sleeping.run_sleep_cycle(event_tx, snapshot_ts).await;
            }
            UserAction::RebuildEmbeddings => {
                let _ = event_tx
                    .send(AppEvent::TextDelta {
                        delta: "Rebuilding embeddings...\n".to_string(),
                    })
                    .await;
                match agent.rebuild_embeddings().await {
                    Ok(count) => {
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
            UserAction::Quit => {
                break;
            }
        }
    }

    agent.shutdown().await;
}
