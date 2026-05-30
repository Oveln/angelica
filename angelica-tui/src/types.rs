pub use crate::mode::{AppMode, ApprovalChoice};

use angelica::llm::types::Role;
use angelica::usage::UsageMetrics;

pub use angelica::agent::events::SlashCommand;

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

pub static BUILTIN_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "help",
        aliases: &["?"],
        description: "Show available commands",
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
        name: "sleep",
        aliases: &[],
        description: "Put angelica to sleep (dream & recharge)",
    },
    SlashCommand {
        name: "rebuild-embeddings",
        aliases: &["rebuild"],
        description: "Rebuild episode embeddings with current model",
    },
    SlashCommand {
        name: "usage",
        aliases: &["stats"],
        description: "Show token usage statistics",
    },
    SlashCommand {
        name: "settings",
        aliases: &["set", "config"],
        description: "Open settings panel",
    },
    SlashCommand {
        name: "undo",
        aliases: &["u"],
        description: "Undo last message exchange",
    },
];

#[derive(Debug, Clone)]
pub enum DisplayMessage {
    Chat {
        role: Role,
        content: String,
        thinking: Option<String>,
        collapsed: bool,
        hidden: bool,
        token_usage: Option<UsageMetrics>,
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
