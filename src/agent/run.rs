use tokio::sync::mpsc;

use super::Agent;
use super::events::{AppEvent, UserAction};
use crate::agent::modes::SleepingMode;
use crate::config::Config;

pub async fn run(
    config: Config,
    mut user_rx: mpsc::Receiver<UserAction>,
    event_tx: mpsc::Sender<AppEvent>,
) {
    let mut agent = Agent::awake(config);

    if let Err(e) = agent.initialize().await {
        let _ = event_tx
            .send(AppEvent::Error {
                message: format!("Initialization failed: {}", e),
            })
            .await;
        return;
    }

    run_loop(agent, &mut user_rx, &event_tx).await;
}

async fn run_loop(
    mut agent: Agent,
    user_rx: &mut mpsc::Receiver<UserAction>,
    event_tx: &mpsc::Sender<AppEvent>,
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
                    agent = execute_sleep(agent, event_tx).await;
                }
            }
            UserAction::ForceSleep => {
                let _ = event_tx.send(AppEvent::FallingAsleep).await;
                agent = execute_sleep(agent, event_tx).await;
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
            UserAction::Quit => {
                break;
            }
        }
    }

    agent.shutdown().await;
}

async fn execute_sleep(agent: Agent, event_tx: &mpsc::Sender<AppEvent>) -> Agent {
    agent.save_state();

    let config = agent.config().clone();
    let memory = agent.memory().clone();
    let skills = agent.skills().clone();
    let conversation_messages: Vec<_> = agent.history_messages().to_vec();

    let (conversation_text, fatigue_desc, turns, tool_calls, fatigue_val) = {
        let awake = agent.run_state_as_awake();
        let (t, tc, f) = awake.fatigue_info();
        (
            crate::sleep::build_conversation_text(&conversation_messages),
            awake.fatigue_desc().to_string(),
            t,
            tc,
            f,
        )
    };

    let _ = event_tx.send(AppEvent::Sleeping).await;

    let snapshot_ts = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let data_dir = std::path::PathBuf::from(&config.state.conversation_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("data"));

    // Phase 1: archive conversation to history folder
    let history_folder = data_dir.join("history").join(&snapshot_ts);
    if let Err(e) = std::fs::create_dir_all(&history_folder) {
        tracing::error!("Failed to create history folder: {}", e);
    }
    if !conversation_messages.is_empty() {
        let archive_path = history_folder.join("conversation.jsonl");
        let now_ts = chrono::Local::now().to_rfc3339();
        if let Ok(mut file) = std::fs::File::create(&archive_path) {
            use std::io::Write;
            for (i, msg) in conversation_messages.iter().enumerate() {
                let tm = crate::agent::history::TimedMessage {
                    ts: format!("{} (msg {})", now_ts, i),
                    message: msg.clone(),
                };
                let _ = writeln!(file, "{}", serde_json::to_string(&tm).unwrap_or_default());
            }
        }
    }

    // Phase 3: create sleep agent and run full ReAct loop
    let sleeping = SleepingMode::new(
        memory.clone(),
        conversation_text,
        turns,
        tool_calls,
        fatigue_desc,
    );

    let sleep_history_path = history_folder.join("sleep.jsonl");
    let mut sleep_agent = Agent::new(
        config.clone(),
        Box::new(sleeping),
        memory.clone(),
        skills.clone(),
        crate::agent::history::History::new(sleep_history_path),
    );

    loop {
        if event_tx.is_closed() {
            break;
        }
        if sleep_agent.step(event_tx).await {
            continue;
        }
        if sleep_agent.is_finished() {
            break;
        }
    }

    // Phase 4: capture dream
    let dream = sleep_agent.run_state_as_sleeping_mut().take_dream();

    // Phase 5: save sleep record
    let sleep_record = serde_json::json!({
        "timestamp": snapshot_ts,
        "turns": turns,
        "tool_calls": tool_calls,
        "fatigue": fatigue_val,
        "dream": dream,
    });
    let sleep_path = history_folder.join("sleep.json");
    if let Err(e) = std::fs::write(
        &sleep_path,
        serde_json::to_string_pretty(&sleep_record).unwrap_or_default(),
    ) {
        tracing::error!("Failed to write sleep record: {}", e);
    }

    // Phase 6: git commit sleep artifacts
    if let Err(e) = crate::data_git::commit_all(&data_dir, &format!("sleep done: {}", snapshot_ts)) {
        tracing::error!("Failed to commit sleep data: {}", e);
    }

    // Phase 7: reset state — clear fatigue and conversation after sleep completes
    {
        let state_path = std::path::PathBuf::from(&config.state.path);
        if state_path.exists() {
            if let Err(e) = std::fs::remove_file(&state_path) {
                tracing::error!("Failed to remove state file: {}", e);
            }
        }

        let conversation_path = std::path::PathBuf::from(&config.state.conversation_path);
        if conversation_path.exists() {
            if let Err(e) = std::fs::write(&conversation_path, "") {
                tracing::error!("Failed to clear conversation history: {}", e);
            }
        }
    }

    // Phase 8: build new awake agent with dream afterglow
    let mut new_agent = Agent::awake(config);

    {
        let new_awake = new_agent.run_state_as_awake_mut();
        if let Some(d) = &dream {
            new_awake.set_dream(d.clone());
        }
        new_awake.set_last_snapshot(snapshot_ts);
    }
    new_agent.save_state();

    if let Err(e) = new_agent.initialize().await {
        tracing::error!("Failed to initialize agent after sleep: {}", e);
    }

    if let Some(dream) = dream {
        let _ = event_tx.send(AppEvent::WakingUp { dream }).await;
    }
    new_agent.send_fatigue_update(event_tx);

    new_agent
}
