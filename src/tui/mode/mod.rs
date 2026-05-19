pub(crate) mod approval;
pub(crate) mod chat;
pub(crate) mod session;
pub(crate) mod slash;

pub use approval::ApprovalState;
pub use session::{SessionAction, SessionPickerState};
pub use slash::{SlashAction, SlashMenuState};

use tokio::sync::mpsc;

use crate::agent::events::UserAction;
use crate::tui::state::AppState;
use crate::tui::types::{BUILTIN_COMMANDS, DisplayMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    Allow,
    Reject,
    EditFeedback,
}

impl ApprovalChoice {
    pub const ALL: [ApprovalChoice; 3] = [
        ApprovalChoice::Allow,
        ApprovalChoice::Reject,
        ApprovalChoice::EditFeedback,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ApprovalChoice::Allow => "Allow",
            ApprovalChoice::Reject => "Reject",
            ApprovalChoice::EditFeedback => "Edit feedback",
        }
    }

    pub fn style(self, selected: bool, theme: &super::theme::Theme) -> ratatui::style::Style {
        use ratatui::style::{Color, Modifier, Style};
        if selected {
            match self {
                ApprovalChoice::Allow => Style::default()
                    .fg(Color::Black)
                    .bg(theme.success)
                    .add_modifier(Modifier::BOLD),
                ApprovalChoice::Reject => Style::default()
                    .fg(Color::Black)
                    .bg(theme.error)
                    .add_modifier(Modifier::BOLD),
                ApprovalChoice::EditFeedback => Style::default()
                    .fg(Color::Black)
                    .bg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            }
        } else {
            Style::default().fg(theme.muted)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Chat,
    Streaming,
    SlashMenu(SlashMenuState),
    SessionPicker(SessionPickerState),
    Approval(ApprovalState),
}

pub async fn execute_slash_command(state: &mut AppState, cmd: &str, tx: &mpsc::Sender<UserAction>) {
    let (cmd_name, _arg) = match cmd.split_once(' ') {
        Some((n, a)) => (n, Some(a)),
        None => (cmd, None),
    };

    let cmd_lower = cmd_name.to_lowercase();

    let matched = BUILTIN_COMMANDS
        .iter()
        .find(|c| c.name == cmd_lower || c.aliases.iter().any(|a| *a == cmd_lower));

    if let Some(matched_cmd) = matched {
        match matched_cmd.name {
            "help" => {
                let mut help = String::from("Available commands:\n");
                for c in BUILTIN_COMMANDS {
                    let aliases = if c.aliases.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", c.aliases.join(", "))
                    };
                    help.push_str(&format!(
                        "  /{}{}\n    {}\n",
                        c.name, aliases, c.description
                    ));
                }
                state.add_chat("system", &help, None);
            }
            "clear" => {
                state.messages.clear();
                let _ = tx.send(UserAction::ClearHistory).await;
            }
            "quit" | "q" => {
                state.should_quit = true;
                let _ = tx.send(UserAction::Quit).await;
            }
            "verbose" | "v" => {
                state.display.verbosity = state.display.verbosity.cycle();
                state.add_chat(
                    "system",
                    &format!("Verbosity: {}", state.display.verbosity.label()),
                    None,
                );
            }
            "thinking" | "think" => {
                state.display.thinking_visible = !state.display.thinking_visible;
                state.add_chat(
                    "system",
                    &format!(
                        "Thinking display: {}",
                        if state.display.thinking_visible {
                            "on"
                        } else {
                            "off"
                        }
                    ),
                    None,
                );
            }
            "model" => {
                let model = state.model_name.clone();
                state.add_chat("system", &model, None);
            }
            "history" | "h" => {
                let count = state.messages.len();
                let user_count = state
                    .messages
                    .iter()
                    .filter(|m| matches!(m, DisplayMessage::Chat { role, .. } if role == "user"))
                    .count();
                state.add_chat(
                    "system",
                    &format!("{} messages ({} user)", count, user_count),
                    None,
                );
            }
            "resume" | "r" => {
                if state.is_streaming {
                    state.add_chat("system", "Cannot resume while streaming", None);
                } else if let Some(arg) = _arg {
                    let _ = tx
                        .send(UserAction::ResumeSession {
                            session_id: arg.trim().to_string(),
                        })
                        .await;
                } else {
                    state.mode = AppMode::SessionPicker(SessionPickerState::new(Vec::new()));
                    let _ = tx
                        .send(UserAction::ResumeSession {
                            session_id: String::new(),
                        })
                        .await;
                }
            }
            _ => {
                state.add_chat("system", &format!("Unknown command: /{}", cmd_name), None);
            }
        }
    } else {
        state.add_chat(
            "system",
            &format!(
                "Unknown command: /{}. Type /help for available commands.",
                cmd_name
            ),
            None,
        );
    }
}
