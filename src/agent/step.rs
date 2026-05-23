use tokio::sync::mpsc;

use super::Agent;
use super::events::AppEvent;
use super::group::group_tool_calls;
use super::modes::RunMode;
use super::dispatch::ProcessOutcome;
use crate::llm::LlmResponse;

impl<S: RunMode> Agent<S> {
    pub async fn step(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        let max_iterations = self.max_iterations();
        let stream_to_tui = self.run_state.stream_to_tui();

        loop {
            if event_tx.is_closed() {
                return false;
            }
            while let Some(group) = self.tool_queue.pop_front() {
                match self.process_one_group(group, event_tx).await {
                    ProcessOutcome::Continue => continue,
                    ProcessOutcome::NeedApproval => return true,
                }
            }

            if self.iteration >= max_iterations {
                if stream_to_tui {
                    let _ = event_tx
                        .send(AppEvent::TextDone {
                            full_text: "[Reached maximum iterations]".to_string(),
                        })
                        .await;
                    let _ = event_tx.send(AppEvent::TurnComplete).await;
                }
                return false;
            }

            let Some(llm_result) = self.next_llm_response(event_tx, stream_to_tui).await else {
                return false;
            };

            self.iteration += 1;

            let LlmResponse {
                reasoning,
                content,
                tool_calls,
                usage,
            } = llm_result;

            // Update fatigue from current context size on every LLM response.
            if let Some(ref metrics) = usage {
                self.run_state.on_context_update(metrics.prompt_tokens);
            }

            if self.run_state.accumulate_history() {
                self.history.record_assistant(
                    content.clone(),
                    reasoning,
                    tool_calls.clone(),
                    usage,
                );
                self.dirty = true;
            }

            let Some(tcs) = tool_calls else {
                self.run_state.on_turn_complete(content.as_deref());
                if stream_to_tui {
                    let full_text = content.as_deref().unwrap_or("").to_string();
                    let _ = event_tx.send(AppEvent::TextDone { full_text }).await;
                    let _ = event_tx.send(AppEvent::TurnComplete).await;
                    if let Some(evt) = self.run_state.fatigue_update_event() {
                        let _ = event_tx.try_send(evt);
                    }
                }
                // Try to recall relevant past episodes via embedding search
                if self.run_state.should_recall() {
                    self.recall_past_episodes(content.as_deref()).await;
                }
                return false;
            };

            self.run_state.on_tool_calls(tcs.len());
            if stream_to_tui && let Some(evt) = self.run_state.fatigue_update_event() {
                let _ = event_tx.try_send(evt);
            }
            self.tool_queue = group_tool_calls(tcs).into();
        }
    }
}
