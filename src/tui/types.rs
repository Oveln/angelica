use ratatui::style::{Color, Modifier, Style};

use super::theme::Theme;

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

    pub fn style(self, selected: bool, theme: &Theme) -> Style {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Normal,
    Verbose,
    Trace,
}

impl Verbosity {
    pub fn label(self) -> &'static str {
        match self {
            Verbosity::Normal => "normal",
            Verbosity::Verbose => "verbose",
            Verbosity::Trace => "trace",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Verbosity::Normal => Verbosity::Verbose,
            Verbosity::Verbose => Verbosity::Trace,
            Verbosity::Trace => Verbosity::Normal,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

pub static BUILTIN_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "help",
        aliases: &["?"],
        description: "Show available commands",
    },
    SlashCommand {
        name: "clear",
        aliases: &[],
        description: "Clear conversation history",
    },
    SlashCommand {
        name: "quit",
        aliases: &["q"],
        description: "Quit the application",
    },
    SlashCommand {
        name: "verbose",
        aliases: &["v"],
        description: "Cycle display verbosity",
    },
    SlashCommand {
        name: "thinking",
        aliases: &["think"],
        description: "Toggle thinking display",
    },
    SlashCommand {
        name: "model",
        aliases: &[],
        description: "Show current model",
    },
    SlashCommand {
        name: "history",
        aliases: &["h"],
        description: "Show recent history",
    },
];

#[derive(Debug, Clone)]
pub enum DisplayMessage {
    Chat {
        role: String,
        content: String,
        thinking: Option<String>,
        collapsed: bool,
        hidden: bool,
    },
    Tool {
        call_id: String,
        name: String,
        args_display: String,
        result: Option<String>,
        collapsed: bool,
        hidden: bool,
    },
    Diff {
        content: String,
        hidden: bool,
    },
}

impl DisplayMessage {
    pub fn is_hidden(&self) -> bool {
        match self {
            DisplayMessage::Chat { hidden, .. }
            | DisplayMessage::Tool { hidden, .. }
            | DisplayMessage::Diff { hidden, .. } => *hidden,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Chat,
    Approval {
        tool_call_id: String,
        tool_name: String,
        tool_label: String,
    },
    Streaming,
    SlashMenu,
}

pub struct ClickRange {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub msg_index: usize,
}
