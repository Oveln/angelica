use tokio::sync::mpsc;

use super::Agent;
use super::events::AppEvent;
use super::modes::RunMode;
use crate::llm::types::{ChatMessage, Role};
use crate::llm::{LlmResponse, LlmStreamEvent, RequestOptions};
use crate::usage::UsageRecord;

impl<S: RunMode> Agent<S> {
    pub(super) fn emit_debug_snapshot(&self) {
        let Some(tx) = &self.debug_tx else {
            return;
        };
        let messages = self.build_turn_messages();
        let snapshot = crate::debug::DebugSnapshot {
            mode: self.run_state.mode_name().to_string(),
            history_messages: messages.len(),
            tool_count: self.all_tool_specs().len(),
            iteration: self.iteration,
            tool_queue_len: self.tool_queue.len(),
            fatigue: self.run_state.fatigue_value(),
            fatigue_desc: self.run_state.fatigue_desc().to_string(),
            turns: self.run_state.turns(),
            tool_calls: self.run_state.tool_calls_count(),
            recall_top_score: self.recall_top_score,
            recall_text_preview: crate::agent::truncate_str(&self.recall_text, 200),
            last_prompt_tokens: self.run_state.last_prompt_tokens(),
            last_completion_tokens: self.run_state.last_completion_tokens(),
            context_messages: messages
                .into_iter()
                .map(|m| crate::debug::snapshot::ContextMessage {
                    role: m.role.to_string(),
                    name: m.name.clone(),
                    content_length: m.content.as_ref().map(|c| c.chars().count()).unwrap_or(0),
                    content_preview: m.content.unwrap_or_default(),
                    tool_calls_count: m.tool_calls.as_ref().map(|tc| tc.len()),
                    tool_call_id: m.tool_call_id.clone(),
                })
                .collect(),
        };
        let _ = tx.send(snapshot);
    }

    pub(super) async fn next_llm_response(
        &self,
        event_tx: &mpsc::Sender<AppEvent>,
        stream_to_tui: bool,
    ) -> Option<LlmResponse> {
        self.emit_debug_snapshot();
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
                self.record_usage(event_tx, response.usage, messages.len())
                    .await;
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
            .any(|m| m.role == Role::System);
        let mut messages = Vec::with_capacity(history.len() + usize::from(!has_system));

        if !has_system {
            messages.push(self.build_system_message());
        }

        messages.extend(history.to_vec());
        messages
    }

    async fn record_usage(
        &self,
        event_tx: &mpsc::Sender<AppEvent>,
        usage: Option<crate::usage::UsageMetrics>,
        context_messages: usize,
    ) {
        let Some(metrics) = usage else {
            return;
        };

        let scope = self.run_state.usage_scope();
        let record = UsageRecord::new(scope, self.iteration, context_messages, metrics);
        let data_dir = self.config.state.data_dir();
        let disk_record = record.clone();
        tokio::task::spawn_blocking(move || {
            append_usage_record(&data_dir, &disk_record);
        });
        let _ = event_tx.send(AppEvent::UsageUpdate { record }).await;
    }
}

fn append_usage_record(data_dir: &std::path::Path, record: &UsageRecord) {
    let path = data_dir.join("usage.jsonl");

    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("Failed to create usage stats directory: {}", e);
        return;
    }

    let json = match serde_json::to_string(record) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("Failed to serialize usage record: {}", e);
            return;
        }
    };

    use std::io::Write;
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "{}", json) {
                tracing::warn!("Failed to write usage record: {}", e);
            }
        }
        Err(e) => tracing::warn!("Failed to open usage stats file: {}", e),
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
