use angelica::agent::events::AppEvent;
use angelica::llm::types::Role;

use super::mode::AppMode;
use super::mode::approval::ApprovalState;
use super::state::AppState;
use super::types::*;

impl AppState {
    pub fn handle_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Init {
                entries,
                current_usage,
                model_name,
            } => {
                self.pending_init = Some((entries.clone(), *current_usage, model_name.clone()));
                if !matches!(self.mode, AppMode::Welcome) {
                    self.apply_pending_init();
                }
            }
            AppEvent::ThinkingDelta { delta } => {
                self.thinking_buffer.push_str(delta);
                self.mode = AppMode::Streaming;
                self.is_streaming = true;
            }
            AppEvent::TextDelta { delta } => {
                self.text_buffer.push_str(delta);
                self.mode = AppMode::Streaming;
                self.is_streaming = true;
            }
            AppEvent::TextDone { full_text } => {
                let thinking = if self.thinking_buffer.is_empty() {
                    None
                } else {
                    Some(std::mem::take(&mut self.thinking_buffer))
                };
                self.add_chat(Role::Assistant, full_text, thinking);
                self.text_buffer.clear();
            }
            AppEvent::TurnComplete => {
                if !self.text_buffer.is_empty() || !self.thinking_buffer.is_empty() {
                    let thinking = if self.thinking_buffer.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut self.thinking_buffer))
                    };
                    let text = std::mem::take(&mut self.text_buffer);
                    self.add_chat(Role::Assistant, &text, thinking);
                }
                self.is_streaming = false;
                self.mode = AppMode::Chat;
            }
            AppEvent::ToolResult {
                call_id,
                name,
                result,
                diff_preview,
            } => {
                let show = self.should_show_tool_result(name);
                self.complete_tool(call_id, result.clone(), diff_preview.clone(), !show);
            }
            AppEvent::ToolCalling {
                call_id,
                name,
                display,
            } => {
                if !self.text_buffer.is_empty() || !self.thinking_buffer.is_empty() {
                    let thinking = if self.thinking_buffer.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut self.thinking_buffer))
                    };
                    let text = std::mem::take(&mut self.text_buffer);
                    self.add_chat(Role::Assistant, &text, thinking);
                }
                self.add_tool_call(call_id.clone(), name.clone(), display.clone());
            }
            AppEvent::ApprovalPending {
                call_id,
                tool_name,
                tool_target,
                preview,
                tool_label,
                is_diff,
            } => {
                if *is_diff {
                    if let Some(DisplayMessage::Tool { hidden, .. }) = self.messages.last_mut() {
                        *hidden = true;
                    }

                    self.add_diff(preview.clone());
                }
                self.scroll.to_bottom();
                self.mode = AppMode::Approval(ApprovalState::new(
                    call_id.clone(),
                    tool_name.clone(),
                    tool_label.clone(),
                    tool_target.clone(),
                ));
            }
            AppEvent::ToolRejected { call_id, feedback } => {
                self.complete_tool(call_id, feedback.clone(), None, true);
            }
            AppEvent::Error { message } => {
                self.add_chat(Role::System, &format!("Error: {}", message), None);
            }
            AppEvent::FatigueUpdate {
                fatigue,
                turns,
                tool_calls,
                desc,
            } => {
                self.fatigue.fatigue = *fatigue;
                self.fatigue.turns = *turns;
                self.fatigue.tool_calls = *tool_calls;
                self.fatigue.desc = desc.clone();
            }
            AppEvent::FallingAsleep => {
                self.is_streaming = false;
                self.mode = AppMode::Chat;
                self.usage = Default::default();
                self.last_total_tokens = 0;
                self.last_response_usage = None;
                self.add_chat(Role::System, "祈芷正在沉睡...", None);
            }
            AppEvent::Sleeping => {
                self.is_streaming = false;
                self.mode = AppMode::Chat;
            }
            AppEvent::WakingUp { dream: _ } => {
                self.add_chat(Role::System, "祈芷醒来了，梦的余韵还留在心头。", None);
            }
            AppEvent::UsageUpdate { metrics } => {
                self.last_response_usage = Some(*metrics);
                self.usage.accumulate(metrics);
                self.last_total_tokens = metrics.total_tokens;
            }
            AppEvent::UsageStatsLoaded { sessions } => {
                self.cached_usage_sessions = Some(sessions.clone());
                self.mode = AppMode::UsageStats;
            }
        }
    }
}
