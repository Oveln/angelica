use tokio::sync::mpsc;

use super::Agent;
use super::events::AppEvent;
use crate::llm::types::ChatMessage;
use crate::llm::{LlmResponse, LlmStreamEvent, RequestOptions};

impl Agent {
    pub(super) async fn next_llm_response(
        &self,
        event_tx: &mpsc::Sender<AppEvent>,
        stream_to_tui: bool,
    ) -> Option<LlmResponse> {
        let messages = self.build_turn_messages();
        let tools = self.all_tool_specs();
        tracing::debug!(
            messages = messages.len(),
            tools = tools.len(),
            iteration = self.iteration,
            "requesting llm turn"
        );

        let stream_result = self
            .llm
            .stream(&messages, RequestOptions::new().with_tools(tools))
            .await;

        let (handle, rx) = match stream_result {
            Ok(stream) => stream,
            Err(e) => {
                emit_llm_error(event_tx, stream_to_tui, "LLM error", e).await;
                return None;
            }
        };

        forward_stream_events(rx, event_tx.clone(), stream_to_tui);

        match handle.await {
            Ok(Ok(response)) => {
                tracing::debug!(
                    has_content = response.content.is_some(),
                    tool_calls = response.tool_calls.as_ref().map_or(0, Vec::len),
                    "received llm turn"
                );
                Some(response)
            }
            Ok(Err(e)) => {
                emit_llm_error(event_tx, stream_to_tui, "LLM error", e).await;
                None
            }
            Err(e) => {
                emit_llm_error(event_tx, stream_to_tui, "LLM task failed", e).await;
                None
            }
        }
    }

    fn build_turn_messages(&self) -> Vec<ChatMessage> {
        if !self.run_state.include_history() {
            return vec![self.build_system_message()];
        }

        let history = self.history.messages();
        let has_system = history
            .iter()
            .any(|m| m.role == "system" && m.name.is_none());
        let mut messages = Vec::with_capacity(history.len() + usize::from(!has_system));

        if !has_system {
            messages.push(self.build_system_message());
        }
        messages.extend(history.to_vec());
        messages
    }
}

fn forward_stream_events(
    mut rx: mpsc::Receiver<LlmStreamEvent>,
    event_tx: mpsc::Sender<AppEvent>,
    stream_to_tui: bool,
) {
    tokio::spawn(async move {
        while let Some(evt) = rx.recv().await {
            if !stream_to_tui {
                if matches!(evt, LlmStreamEvent::Done(_)) {
                    break;
                }
                continue;
            }

            match evt {
                LlmStreamEvent::ThinkingDelta { delta } => {
                    let _ = event_tx.send(AppEvent::ThinkingDelta { delta }).await;
                }
                LlmStreamEvent::TextDelta { delta } => {
                    let _ = event_tx.send(AppEvent::TextDelta { delta }).await;
                }
                LlmStreamEvent::Done(_) => break,
            }
        }
    });
}

async fn emit_llm_error(
    event_tx: &mpsc::Sender<AppEvent>,
    stream_to_tui: bool,
    label: &str,
    error: impl std::fmt::Display,
) {
    let message = format!("{}: {}", label, error);
    if stream_to_tui {
        let _ = event_tx.send(AppEvent::Error { message }).await;
    } else {
        tracing::error!("{}", message);
    }
}
