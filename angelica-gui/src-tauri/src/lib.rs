use std::sync::Mutex;
use tokio::sync::mpsc;
use tauri::{Emitter, Manager};

use angelica::agent::events::{AppEvent, UserAction};
use angelica::config::Config;

#[tauri::command]
async fn send_message(
    state: tauri::State<'_, AppState>,
    content: String,
) -> Result<(), String> {
    tracing::info!("send_message called: {:?}", content);
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
async fn quit(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tx = state.user_tx.lock().map_err(|e| e.to_string())?.clone();
    tx.send(UserAction::Quit).await.map_err(|e| e.to_string())
}

pub struct AppState {
    pub user_tx: Mutex<mpsc::Sender<UserAction>>,
    pub init_messages: Mutex<Option<Vec<serde_json::Value>>>,
}

#[tauri::command]
async fn get_init_messages(
    state: tauri::State<'_, AppState>,
) -> Result<Option<Vec<serde_json::Value>>, String> {
    let mut guard = state.init_messages.lock().map_err(|e| e.to_string())?;
    Ok(guard.take())
}

pub fn run() {
    init_logging();

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

    tracing::info!("Starting agent thread...");
    let agent_config = config;
    let agent_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async {
            if let Err(e) =
                angelica::agent::run(agent_config, user_action_rx, app_event_tx, None).await
            {
                tracing::error!("Agent error: {}", e);
            }
        })
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            user_tx: Mutex::new(user_action_tx),
            init_messages: Mutex::new(None),
        })
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // On window close, tell agent to quit so it saves state
            if let Some(window) = app.get_webview_window("main") {
                let qtx = quit_tx.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        tracing::info!("Window closing, sending quit to agent");
                        let _ = qtx.try_send(UserAction::Quit);
                    }
                });
            }

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
                rt.block_on(async {
                    tracing::info!("Event bridge started");
                    while let Some(event) = app_event_rx.recv().await {
                        if let AppEvent::Init { messages } = &event {
                            if let Some(state) = app_handle.try_state::<AppState>() {
                                if let Ok(mut guard) = state.init_messages.lock() {
                                    let raw: Vec<serde_json::Value> = messages
                                        .iter()
                                        .filter_map(|m| serde_json::to_value(m).ok())
                                        .collect();
                                    *guard = Some(raw);
                                }
                            }
                        }
                        let (event_name, payload) = serialize_event(&event);
                        tracing::debug!("Emitting: {}", event_name);
                        let _ = app_handle.emit(event_name, payload);
                    }
                    tracing::info!("Event bridge ended");
                })
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send_message,
            approve_pending,
            approve_always,
            reject_tool,
            force_sleep,
            quit,
            get_init_messages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    let _ = agent_handle.join();
}

fn init_logging() {
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("angelica");

    let log_path = log_dir.join("angelica-gui.log");

    let file = std::fs::create_dir_all(&log_dir)
        .ok()
        .and_then(|_| std::fs::File::create(&log_path).ok());

    let builder = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
    );

    if let Some(f) = file {
        builder.with_writer(f).with_ansi(false).init();
    } else {
        builder.init();
    }
}

fn show_fatal_dialog(message: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display dialog \"{}\" buttons {{\"OK\"}} default button \"OK\" with title \"祈芷\" with icon stop",
            message.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
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
    match event {
        AppEvent::Init { messages } => ("init", serde_json::json!({ "messages": messages })),
        AppEvent::ThinkingDelta { delta } => ("thinking-delta", serde_json::json!({ "delta": delta })),
        AppEvent::TextDelta { delta } => ("text-delta", serde_json::json!({ "delta": delta })),
        AppEvent::TextDone { full_text } => ("text-done", serde_json::json!({ "full_text": full_text })),
        AppEvent::TurnComplete => ("turn-complete", serde_json::json!({})),
        AppEvent::ToolCalling { call_id, name, arguments } => (
            "tool-calling",
            serde_json::json!({ "call_id": call_id, "name": name, "arguments": arguments }),
        ),
        AppEvent::ToolResult { call_id, name, result, diff_preview } => (
            "tool-result",
            serde_json::json!({ "call_id": call_id, "name": name, "result": result, "diff_preview": diff_preview }),
        ),
        AppEvent::ApprovalPending { call_id, tool_name, tool_target, preview } => (
            "approval-pending",
            serde_json::json!({ "call_id": call_id, "tool_name": tool_name, "tool_target": tool_target, "preview": preview }),
        ),
        AppEvent::ToolRejected { call_id, feedback } => (
            "tool-rejected",
            serde_json::json!({ "call_id": call_id, "feedback": feedback }),
        ),
        AppEvent::Error { message } => ("error", serde_json::json!({ "message": message })),
        AppEvent::FatigueUpdate { fatigue, turns, tool_calls, desc } => (
            "fatigue-update",
            serde_json::json!({ "fatigue": fatigue, "turns": turns, "tool_calls": tool_calls, "desc": desc }),
        ),
        AppEvent::UsageUpdate { record } => (
            "usage-update",
            serde_json::json!({ "record": record }),
        ),
        AppEvent::FallingAsleep => ("falling-asleep", serde_json::json!({})),
        AppEvent::Sleeping => ("sleeping", serde_json::json!({})),
        AppEvent::WakingUp { dream } => ("waking-up", serde_json::json!({ "dream": dream })),
    }
}
