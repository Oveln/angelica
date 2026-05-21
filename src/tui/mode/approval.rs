use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{AppMode, ApprovalChoice};
use crate::agent::events::UserAction;
use crate::tui::input::InputBuffer;
use crate::tui::state::AppState;
use crate::tui::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalState {
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_label: String,
    pub tool_target: Option<String>,
    pub selected: ApprovalChoice,
    pub feedback: InputBuffer,
}

impl ApprovalState {
    pub fn new(
        tool_call_id: String,
        tool_name: String,
        tool_label: String,
        tool_target: Option<String>,
    ) -> Self {
        Self {
            tool_call_id,
            tool_name,
            tool_label,
            tool_target,
            selected: ApprovalChoice::Allow,
            feedback: InputBuffer::new(),
        }
    }
}

enum ApprovalAction {
    None,
    Approve,
    ApproveAlways {
        tool: String,
        target: String,
        persist: bool,
    },
    Reject(Option<String>),
}

pub async fn handle_key(
    state: &mut AppState,
    key: KeyEvent,
    tx: &tokio::sync::mpsc::Sender<UserAction>,
) {
    let mut action = ApprovalAction::None;

    if let AppMode::Approval(ref mut a) = state.mode {
        let editing = a.selected == ApprovalChoice::EditFeedback;

        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.should_quit = true;
                let _ = tx.send(UserAction::Quit).await;
                return;
            }
            KeyCode::Esc if editing => {
                a.selected = ApprovalChoice::Reject;
            }
            KeyCode::Up => {
                state.scroll.up(3);
            }
            KeyCode::Down => {
                state.scroll.down(3);
            }
            KeyCode::PageUp => {
                state.scroll.up(10);
            }
            KeyCode::PageDown => {
                state.scroll.down(10);
            }
            KeyCode::Left => {
                if editing {
                    a.feedback.move_left();
                } else {
                    a.selected = prev_choice(a.selected);
                }
            }
            KeyCode::Right => {
                if editing {
                    a.feedback.move_right();
                } else {
                    a.selected = next_choice(a.selected);
                }
            }
            KeyCode::Tab => {
                a.selected = next_choice(a.selected);
            }
            KeyCode::Enter => match a.selected {
                ApprovalChoice::Allow => action = ApprovalAction::Approve,
                ApprovalChoice::AlwaysSession => {
                    let target = a.tool_target.clone().unwrap_or_else(|| "*".to_string());
                    action = ApprovalAction::ApproveAlways {
                        tool: a.tool_name.clone(),
                        target,
                        persist: false,
                    }
                }
                ApprovalChoice::AlwaysPersist => {
                    let target = a.tool_target.clone().unwrap_or_else(|| "*".to_string());
                    action = ApprovalAction::ApproveAlways {
                        tool: a.tool_name.clone(),
                        target,
                        persist: true,
                    }
                }
                ApprovalChoice::Reject => action = ApprovalAction::Reject(None),
                ApprovalChoice::EditFeedback if editing => {
                    action = ApprovalAction::Reject(Some(a.feedback.trim().to_string()));
                }
                ApprovalChoice::EditFeedback => {
                    a.selected = ApprovalChoice::EditFeedback;
                }
            },
            KeyCode::Char('y') if !editing => action = ApprovalAction::Approve,
            KeyCode::Char('a') if !editing => {
                let target = a.tool_target.clone().unwrap_or_else(|| "*".to_string());
                action = ApprovalAction::ApproveAlways {
                    tool: a.tool_name.clone(),
                    target,
                    persist: false,
                }
            }
            KeyCode::Char('p') if !editing => {
                let target = a.tool_target.clone().unwrap_or_else(|| "*".to_string());
                action = ApprovalAction::ApproveAlways {
                    tool: a.tool_name.clone(),
                    target,
                    persist: true,
                }
            }
            KeyCode::Char('n') if !editing => action = ApprovalAction::Reject(None),
            KeyCode::Char(c) if editing => {
                a.feedback.insert(c);
            }
            KeyCode::Backspace if editing => {
                a.feedback.backspace();
            }
            _ => {}
        }
    }

    match action {
        ApprovalAction::None => {}
        ApprovalAction::Approve => {
            state.mode = AppMode::Chat;
            let _ = tx.send(UserAction::ApprovePending).await;
        }
        ApprovalAction::ApproveAlways {
            tool,
            target,
            persist,
        } => {
            state.mode = AppMode::Chat;
            let _ = tx
                .send(UserAction::ApproveAlways {
                    tool,
                    target,
                    persist,
                })
                .await;
        }
        ApprovalAction::Reject(feedback) => {
            state.mode = AppMode::Chat;
            let _ = tx.send(UserAction::RejectTool { feedback }).await;
        }
    }
}

fn next_choice(current: ApprovalChoice) -> ApprovalChoice {
    match current {
        ApprovalChoice::Allow => ApprovalChoice::AlwaysSession,
        ApprovalChoice::AlwaysSession => ApprovalChoice::AlwaysPersist,
        ApprovalChoice::AlwaysPersist => ApprovalChoice::Reject,
        ApprovalChoice::Reject => ApprovalChoice::EditFeedback,
        ApprovalChoice::EditFeedback => ApprovalChoice::Allow,
    }
}

fn prev_choice(current: ApprovalChoice) -> ApprovalChoice {
    match current {
        ApprovalChoice::Allow => ApprovalChoice::EditFeedback,
        ApprovalChoice::AlwaysSession => ApprovalChoice::Allow,
        ApprovalChoice::AlwaysPersist => ApprovalChoice::AlwaysSession,
        ApprovalChoice::Reject => ApprovalChoice::AlwaysPersist,
        ApprovalChoice::EditFeedback => ApprovalChoice::Reject,
    }
}

pub fn draw_header(f: &mut Frame, area: Rect, tool_label: &str, theme: &Theme) {
    let header = Line::from(vec![
        Span::styled(" \u{25B3} ", Style::default().fg(theme.warning)),
        Span::styled(
            "Permission required",
            Style::default()
                .fg(theme.input)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let max_w = area.width as usize;
    let label_w = max_w.saturating_sub(3);
    let truncated: String = tool_label.chars().take(label_w).collect();
    let detail = Line::from(vec![
        Span::styled("   ", Style::default()),
        Span::styled(truncated, Style::default().fg(theme.muted)),
    ]);
    let para = Paragraph::new(vec![header, detail]).style(Style::default().bg(theme.status_bg));
    f.render_widget(para, area);
}

pub fn draw_choices(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let (selected, editing) = match &state.mode {
        AppMode::Approval(a) => (a.selected, a.selected == ApprovalChoice::EditFeedback),
        _ => return,
    };
    let max_w = area.width as usize;
    let choices: Vec<Span> = ApprovalChoice::ALL
        .iter()
        .flat_map(|&choice| {
            let label = choice.label();
            let sel = choice == selected;
            let hint = match choice {
                ApprovalChoice::Allow => "y",
                ApprovalChoice::AlwaysSession => "a",
                ApprovalChoice::AlwaysPersist => "p",
                ApprovalChoice::Reject => "n",
                ApprovalChoice::EditFeedback => "e",
            };
            let styled = if sel {
                format!(" \u{25B8} {} [{}] \u{25C2} ", label, hint)
            } else {
                format!("   {} [{}]   ", label, hint)
            };
            vec![Span::styled(styled, choice.style(sel, theme))]
        })
        .collect();

    let hint_text = if editing {
        "enter confirm  \u{2502}  esc back"
    } else {
        "\u{2194} select  \u{2502}  y/a/p/n confirm"
    };
    let hint_str = format!("  {}", hint_text);
    let hint_w = UnicodeWidthStr::width(hint_str.as_str());
    let hint_display = if hint_w > max_w {
        let truncated: String = hint_str.chars().take(max_w).collect();
        truncated
    } else {
        hint_str
    };
    let hints = Line::from(Span::styled(
        hint_display,
        Style::default().fg(theme.status_muted),
    ));

    let para = Paragraph::new(vec![Line::from(""), Line::from(choices), hints]);
    f.render_widget(para, area);
}

pub fn draw_feedback_input(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let feedback_str = match &state.mode {
        AppMode::Approval(a) => (a.feedback.as_str(), a.feedback.display_cursor_col()),
        _ => return,
    };
    let feedback_block = Block::default()
        .borders(Borders::ALL)
        .title(" Feedback ")
        .border_style(Style::default().fg(theme.warning));
    let feedback_para = Paragraph::new(Span::styled(
        feedback_str.0,
        Style::default().fg(theme.input),
    ))
    .block(feedback_block);
    f.render_widget(feedback_para, area);
    f.set_cursor_position((area.x + 1 + feedback_str.1, area.y + 1));
}
