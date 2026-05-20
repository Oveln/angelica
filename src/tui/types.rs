pub use crate::tui::mode::{AppMode, ApprovalChoice};

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
    SlashCommand {
        name: "resume",
        aliases: &["r"],
        description: "Resume a previous session",
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
        diff_preview: Option<String>,
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

pub struct ClickRange {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub msg_index: usize,
}
