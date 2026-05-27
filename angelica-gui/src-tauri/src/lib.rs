use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tokio::sync::mpsc;
use tracing_subscriber::prelude::*;

use angelica::agent::events::{AppEvent, UserAction};
use angelica::config::Config;

#[tauri::command]
async fn send_message(state: tauri::State<'_, AppState>, content: String) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::SendMessage { content })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn approve_pending(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::ApprovePending)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn approve_always(
    state: tauri::State<'_, AppState>,
    tool: String,
    target: String,
    persist: bool,
) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::ApproveAlways {
        tool,
        target,
        persist,
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn reject_tool(
    state: tauri::State<'_, AppState>,
    feedback: Option<String>,
) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::RejectTool { feedback })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn force_sleep(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::ForceSleep)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rebuild_embeddings(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::RebuildEmbeddings)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn request_usage_stats(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::UsageStats)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn request_init(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::RequestInit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn quit(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::Quit).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_data_dir(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::GetDataDir)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_config(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::LoadConfig)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(state: tauri::State<'_, AppState>, toml_str: String) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::SaveConfig { toml_str })
        .await
        .map_err(|e| e.to_string())
}

pub struct AppState {
    pub user_tx: Mutex<mpsc::Sender<UserAction>>,
}

pub fn run() {
    init_logging();
    tracing::info!("angelica-gui starting");

    let config = match Config::load_or_create(None) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to load config: {}", e);
            show_fatal_dialog(&format!("配置加载失败:\n{e}"));
            std::process::exit(1);
        }
    };

    let (app_event_tx, mut app_event_rx) = mpsc::channel::<AppEvent>(256);
    let (user_action_tx, user_action_rx) = mpsc::channel::<UserAction>(256);
    let quit_tx = user_action_tx.clone();

    {
        let data_dir = config.state.data_dir();
        if let Err(e) = angelica::data_git::ensure_repo(&data_dir) {
            tracing::warn!("Failed to initialize data git repo: {}", e);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            user_tx: Mutex::new(user_action_tx),
        })
        .setup(move |app| {
            let app_handle = app.handle().clone();

            if let Some(window) = app.get_webview_window("main") {
                let qtx = quit_tx.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        tracing::info!("Window closing, sending quit to agent");
                        let _ = qtx.try_send(UserAction::Quit);
                    }
                });
            }

            tauri::async_runtime::spawn(async move {
                tracing::info!("Event bridge started");
                while let Some(event) = app_event_rx.recv().await {
                    let (event_name, payload) = serialize_event(&event);
                    tracing::debug!(
                        event = %event_name,
                        payload_size = serde_json::to_string(&payload).map_or(0, |s| s.len()),
                        "emitting event to frontend"
                    );
                    let _ = app_handle.emit(event_name, payload);
                }
                tracing::info!("Event bridge ended");
            });

            tauri::async_runtime::spawn(async move {
                tracing::info!("Agent starting");
                match angelica::agent::run(config, user_action_rx, app_event_tx, None).await {
                    Ok(()) => tracing::info!("Agent exited normally"),
                    Err(e) => tracing::error!("Agent error: {}", e),
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send_message,
            approve_pending,
            approve_always,
            reject_tool,
            force_sleep,
            rebuild_embeddings,
            request_usage_stats,
            request_init,
            quit,
            load_config,
            get_data_dir,
            save_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_logging() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("angelica");

    let file_appender = tracing_appender::rolling::daily(&log_dir, "angelica-gui.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_level(true)
        .with_target(false)
        .with_line_number(false)
        .with_filter(env_filter.clone());

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_level(true)
        .with_target(false)
        .with_line_number(false)
        .with_filter(env_filter);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();

    std::mem::forget(_guard);
}

fn show_fatal_dialog(message: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display dialog \"{}\" buttons {{\"OK\"}} default button \"OK\" with title \"祈芷\" with icon stop",
            message
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output();
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("{}", message);
    }
}

fn serialize_event(event: &AppEvent) -> (&'static str, serde_json::Value) {
    use angelica::agent::events::{
        ApprovalPendingPayload, ConfigLoadedPayload, ConfigSavedPayload, DataDirPayload,
        ErrorPayload, FatigueUpdatePayload, InitPayload, TextDeltaPayload, TextDonePayload,
        ThinkingDeltaPayload, ToolCallingPayload, ToolRejectedPayload, ToolResultPayload,
        UsageStatsLoadedPayload, UsageUpdatePayload, WakingUpPayload,
    };

    match event {
        AppEvent::Init {
            entries,
            current_usage,
            model_name,
        } => {
            let payload = serde_json::to_value(InitPayload {
                entries: entries.clone(),
                current_usage: *current_usage,
                model_name: model_name.clone(),
            })
            .expect("serialize init payload");
            ("init", payload)
        }
        AppEvent::ThinkingDelta { delta } => {
            let payload = serde_json::to_value(ThinkingDeltaPayload {
                delta: delta.clone(),
            })
            .expect("serialize thinking-delta payload");
            ("thinking-delta", payload)
        }
        AppEvent::TextDelta { delta } => {
            let payload = serde_json::to_value(TextDeltaPayload {
                delta: delta.clone(),
            })
            .expect("serialize text-delta payload");
            ("text-delta", payload)
        }
        AppEvent::TextDone { full_text } => {
            let payload = serde_json::to_value(TextDonePayload {
                full_text: full_text.clone(),
            })
            .expect("serialize text-done payload");
            ("text-done", payload)
        }
        AppEvent::TurnComplete => ("turn-complete", serde_json::json!({})),
        AppEvent::ToolCalling {
            call_id,
            name,
            display,
        } => {
            let payload = serde_json::to_value(ToolCallingPayload {
                call_id: call_id.clone(),
                name: name.clone(),
                display: display.clone(),
            })
            .expect("serialize tool-calling payload");
            ("tool-calling", payload)
        }
        AppEvent::ToolResult {
            call_id,
            name,
            result,
            diff_preview,
        } => {
            let payload = serde_json::to_value(ToolResultPayload {
                call_id: call_id.clone(),
                name: name.clone(),
                result: result.clone(),
                diff_preview: diff_preview.clone(),
            })
            .expect("serialize tool-result payload");
            ("tool-result", payload)
        }
        AppEvent::ApprovalPending {
            call_id,
            tool_name,
            tool_target,
            preview,
            tool_label,
            is_diff,
        } => {
            let payload = serde_json::to_value(ApprovalPendingPayload {
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                tool_target: tool_target.clone(),
                preview: preview.clone(),
                tool_label: tool_label.clone(),
                is_diff: *is_diff,
            })
            .expect("serialize approval-pending payload");
            ("approval-pending", payload)
        }
        AppEvent::ToolRejected { call_id, feedback } => {
            let payload = serde_json::to_value(ToolRejectedPayload {
                call_id: call_id.clone(),
                feedback: feedback.clone(),
            })
            .expect("serialize tool-rejected payload");
            ("tool-rejected", payload)
        }
        AppEvent::ConfigLoaded { toml } => {
            let payload = serde_json::to_value(ConfigLoadedPayload { toml: toml.clone() })
                .expect("serialize config-loaded payload");
            ("config-loaded", payload)
        }
        AppEvent::ConfigSaved { message } => {
            let payload = serde_json::to_value(ConfigSavedPayload {
                message: message.clone(),
            })
            .expect("serialize config-saved payload");
            ("config-saved", payload)
        }
        AppEvent::DataDir { path } => {
            let payload = serde_json::to_value(DataDirPayload { path: path.clone() })
                .expect("serialize data-dir payload");
            ("data-dir", payload)
        }
        AppEvent::Error { message } => {
            let payload = serde_json::to_value(ErrorPayload {
                message: message.clone(),
            })
            .expect("serialize error payload");
            ("error", payload)
        }
        AppEvent::FatigueUpdate {
            fatigue,
            turns,
            tool_calls,
            desc,
        } => {
            let payload = serde_json::to_value(FatigueUpdatePayload {
                fatigue: *fatigue,
                turns: *turns,
                tool_calls: *tool_calls,
                desc: desc.clone(),
            })
            .expect("serialize fatigue-update payload");
            ("fatigue-update", payload)
        }
        AppEvent::UsageUpdate { metrics } => {
            let payload = serde_json::to_value(UsageUpdatePayload { metrics: *metrics })
                .expect("serialize usage-update payload");
            ("usage-update", payload)
        }
        AppEvent::UsageStatsLoaded { sessions } => {
            let payload = serde_json::to_value(UsageStatsLoadedPayload {
                sessions: sessions.clone(),
            })
            .expect("serialize usage-stats-loaded payload");
            ("usage-stats-loaded", payload)
        }
        AppEvent::FallingAsleep => ("falling-asleep", serde_json::json!({})),
        AppEvent::Sleeping => ("sleeping", serde_json::json!({})),
        AppEvent::WakingUp { dream } => {
            let payload = serde_json::to_value(WakingUpPayload {
                dream: dream.clone(),
            })
            .expect("serialize waking-up payload");
            ("waking-up", payload)
        }
    }
}
