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
            AppEvent::UndoDone { entries } => {
                self.is_streaming = false;
                self.mode = AppMode::Chat;
                self.messages.clear();
                self.rebuild_from_entries(entries);
                self.add_chat(Role::System, "Undone.", None);
            }
            AppEvent::ConfigLoaded { .. }
            | AppEvent::ConfigSaved { .. }
            | AppEvent::DataDir { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angelica::agent::events::{DisplayEntry, DisplayRole};

    fn entries_chat(count: usize) -> Vec<DisplayEntry> {
        (0..count)
            .map(|i| DisplayEntry::Chat {
                role: if i % 2 == 0 {
                    DisplayRole::User
                } else {
                    DisplayRole::Assistant
                },
                content: format!("msg {}", i),
                thinking: None,
            })
            .collect()
    }

    #[test]
    fn undo_done_clears_and_rebuilds_messages() {
        let mut state = AppState::new("test-model".into());
        state.add_chat(Role::User, "old", None);
        state.add_chat(Role::Assistant, "reply", None);
        assert_eq!(state.messages.len(), 2);

        let entries = entries_chat(4);
        state.handle_event(&AppEvent::UndoDone { entries });

        assert_eq!(state.messages.len(), 5);
        match &state.messages[0] {
            DisplayMessage::Chat { role, content, .. } => {
                assert_eq!(*role, Role::User);
                assert_eq!(content, "msg 0");
            }
            _ => panic!("expected chat message"),
        }
        match &state.messages[4] {
            DisplayMessage::Chat { role, content, .. } => {
                assert_eq!(*role, Role::System);
                assert_eq!(content, "Undone.");
            }
            _ => panic!("expected system message"),
        }
    }

    #[test]
    fn undo_done_empty_entries_shows_only_system() {
        let mut state = AppState::new("test-model".into());
        state.add_chat(Role::User, "gone", None);

        state.handle_event(&AppEvent::UndoDone { entries: vec![] });

        assert_eq!(state.messages.len(), 1);
        match &state.messages[0] {
            DisplayMessage::Chat { role, content, .. } => {
                assert_eq!(*role, Role::System);
                assert_eq!(content, "Undone.");
            }
            _ => panic!("expected system message"),
        }
    }

    #[test]
    fn undo_done_with_tool_entries() {
        let mut state = AppState::new("test-model".into());

        let entries = vec![
            DisplayEntry::Chat {
                role: DisplayRole::User,
                content: "read foo".into(),
                thinking: None,
            },
            DisplayEntry::Tool {
                call_id: "tc_1".into(),
                name: "read_file".into(),
                args_display: "read foo.rs".into(),
                result: Some("contents".into()),
                diff_preview: None,
            },
            DisplayEntry::Chat {
                role: DisplayRole::Assistant,
                content: "here it is".into(),
                thinking: None,
            },
        ];

        state.handle_event(&AppEvent::UndoDone { entries });

        assert_eq!(state.messages.len(), 4);
        match &state.messages[1] {
            DisplayMessage::Tool { name, result, .. } => {
                assert_eq!(name, "read_file");
                assert_eq!(result.as_deref(), Some("contents"));
            }
            _ => panic!("expected tool message"),
        }
    }

    #[test]
    fn undo_done_resets_mode_to_chat() {
        let mut state = AppState::new("test-model".into());
        state.mode = AppMode::Streaming;
        state.is_streaming = true;

        state.handle_event(&AppEvent::UndoDone {
            entries: entries_chat(2),
        });

        assert!(!state.is_streaming);
    }
}
