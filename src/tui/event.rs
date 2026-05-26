use crate::llm::types::Role;
use crate::agent::events::AppEvent;

use super::mode::AppMode;
use super::mode::approval::ApprovalState;
use super::state::AppState;
use super::types::*;

impl AppState {
    pub fn handle_event(&mut self, event: &AppEvent) {
        if matches!(self.mode, AppMode::Welcome) {
            self.load_conversation();
        }

        match event {
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
                arguments,
            } => {
                if !self.text_buffer.is_empty() {
                    let thinking = if self.thinking_buffer.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut self.thinking_buffer))
                    };
                    let text = std::mem::take(&mut self.text_buffer);
                    self.add_chat(Role::Assistant, &text, thinking);
                }
                let display = self.format_tool_args(name, arguments);
                self.add_tool_call(call_id.clone(), name.clone(), display);
            }
            AppEvent::ApprovalPending {
                call_id,
                tool_name,
                tool_target,
                preview,
            } => {
                let first_line = preview.lines().next().unwrap_or("");

                let tool_label = if first_line.starts_with("$ ") {
                    format!("\u{2192} {}", first_line)
                } else if first_line.starts_with("---") || first_line.starts_with("diff ") {
                    let second = preview.lines().nth(1).unwrap_or("");
                    if let Some(path) = second.strip_prefix("+++ ") {
                        format!(
                            "\u{2192} edit {}",
                            path.trim_start_matches('b').trim_start_matches('/')
                        )
                    } else {
                        "\u{2192} edit".to_string()
                    }
                } else {
                    format!("\u{2192} {}", first_line)
                };

                let has_diff_content = preview.lines().count() > 1 || !preview.starts_with("$ ");

                if has_diff_content {
                    if let Some(DisplayMessage::Tool { hidden, .. }) = self.messages.last_mut() {
                        *hidden = true;
                    }

                    self.add_diff(preview.clone());
                }
                self.scroll.to_bottom();
                self.mode = AppMode::Approval(ApprovalState::new(
                    call_id.clone(),
                    tool_name.clone(),
                    tool_label,
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
            AppEvent::UsageUpdate { record } => {
                self.last_response_usage = Some(record.metrics);
                self.usage.accumulate(&record.metrics);
                self.last_total_tokens = record.metrics.total_tokens;
            }
        }
    }
}
