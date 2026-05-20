use tokio::sync::mpsc;

use super::events::{AppEvent, UserAction};
use super::Agent;
use crate::config::Config;

pub async fn run(
    config: Config,
    mut user_rx: mpsc::Receiver<UserAction>,
    event_tx: mpsc::Sender<AppEvent>,
) {
    let mut agent = Agent::new(config);

    if let Err(e) = agent.initialize().await {
        let _ = event_tx
            .send(AppEvent::Error {
                message: format!("Initialization failed: {}", e),
            })
            .await;
        return;
    }

    run_loop(&mut agent, &mut user_rx, &event_tx).await;

    agent.shutdown().await;
}

async fn run_loop(
    agent: &mut Agent,
    user_rx: &mut mpsc::Receiver<UserAction>,
    event_tx: &mpsc::Sender<AppEvent>,
) {
    while let Some(action) = user_rx.recv().await {
        match action {
            UserAction::SendMessage { content } => {
                agent.push_user_message(&content);
                agent.iteration = 0;
                agent.tool_queue.clear();
                let _ = agent.step(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::ApprovePending => {
                let _ = agent.approve_and_step(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::RejectTool { feedback } => {
                let _ = agent
                    .reject_and_step(feedback.as_deref().unwrap_or(""), event_tx)
                    .await;
                agent.save_if_dirty().await;
            }
            UserAction::ClearHistory => {
                agent.clear_history();
            }
            UserAction::ResumeSession { session_id } => {
                if session_id.is_empty() {
                    match agent.sessions.query_sessions("", 20) {
                        Ok(sessions) => {
                            let _ = event_tx.send(AppEvent::SessionList { sessions }).await;
                        }
                        Err(e) => {
                            let _ = event_tx
                                .send(AppEvent::Error {
                                    message: format!("Failed to query sessions: {}", e),
                                })
                                .await;
                        }
                    }
                } else {
                    match agent.sessions.load_session_messages(&session_id) {
                        Ok(messages) => {
                            agent.history.clear();
                            for msg in &messages {
                                agent.history.push(msg.clone());
                            }
                            agent.tool_queue.clear();
                            agent.pending_approval = None;
                            agent.iteration = 0;
                            agent.dirty = true;
                            let _ = event_tx.send(AppEvent::SessionLoaded { messages }).await;
                        }
                        Err(e) => {
                            let _ = event_tx
                                .send(AppEvent::Error {
                                    message: format!(
                                        "Failed to load session {}: {}",
                                        session_id, e
                                    ),
                                })
                                .await;
                        }
                    }
                }
            }
            UserAction::Quit => {
                break;
            }
        }
    }
}
