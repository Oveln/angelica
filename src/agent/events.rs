use serde::Serialize;
use ts_rs::TS;

use crate::usage::UsageMetrics;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Init {
        entries: Vec<DisplayEntry>,
        current_usage: Option<UsageMetrics>,
        model_name: String,
    },
    ThinkingDelta {
        delta: String,
    },
    TextDelta {
        delta: String,
    },
    TextDone {
        full_text: String,
    },
    TurnComplete,

    ToolCalling {
        call_id: String,
        name: String,
        display: String,
    },
    ToolResult {
        call_id: String,
        name: String,
        result: String,
        diff_preview: Option<String>,
    },

    ApprovalPending {
        call_id: String,
        tool_name: String,
        tool_target: Option<String>,
        preview: String,
        tool_label: String,
        is_diff: bool,
    },
    ToolRejected {
        call_id: String,
        feedback: String,
    },

    ConfigLoaded {
        toml: String,
    },
    ConfigSaved {
        message: String,
    },
    DataDir {
        path: String,
    },
    Error {
        message: String,
    },

    FatigueUpdate {
        fatigue: f64,
        turns: u32,
        tool_calls: u32,
        desc: String,
    },
    UsageUpdate {
        metrics: crate::usage::UsageMetrics,
    },

    UsageStatsLoaded {
        sessions: Vec<crate::usage::SessionUsage>,
    },

    FallingAsleep,
    Sleeping,
    WakingUp {
        dream: String,
    },
    UndoDone {
        entries: Vec<DisplayEntry>,
    },
}

#[derive(Debug, Clone)]
pub enum UserAction {
    SendMessage {
        content: String,
    },
    ApprovePending,
    ApproveAlways {
        tool: String,
        target: String,
        persist: bool,
    },
    RejectTool {
        feedback: Option<String>,
    },
    ForceSleep,
    RebuildEmbeddings,
    LoadConfig,
    SaveConfig {
        toml_str: String,
    },
    GetDataDir,
    UsageStats,
    RequestInit,
    Quit,
    Undo,
}

// ── TS-exported payload types for the GUI event bridge ──
// All types use #[ts(export_to = "api-generated.ts")] to land in a single file.

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct InitPayload {
    pub entries: Vec<DisplayEntry>,
    pub current_usage: Option<UsageMetrics>,
    pub model_name: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ThinkingDeltaPayload {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct TextDeltaPayload {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct TextDonePayload {
    pub full_text: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ToolCallingPayload {
    pub call_id: String,
    pub name: String,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ToolResultPayload {
    pub call_id: String,
    pub name: String,
    pub result: String,
    pub diff_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ApprovalPendingPayload {
    pub call_id: String,
    pub tool_name: String,
    pub tool_target: Option<String>,
    pub preview: String,
    pub tool_label: String,
    pub is_diff: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ToolRejectedPayload {
    pub call_id: String,
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ErrorPayload {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct FatigueUpdatePayload {
    pub fatigue: f64,
    pub turns: u32,
    pub tool_calls: u32,
    pub desc: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct UsageUpdatePayload {
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct UsageStatsLoadedPayload {
    pub sessions: Vec<crate::usage::SessionUsage>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct WakingUpPayload {
    pub dream: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ConfigLoadedPayload {
    pub toml: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct ConfigSavedPayload {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "api-generated.ts")]
pub struct DataDirPayload {
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "api-generated.ts")]
pub enum DisplayRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "api-generated.ts")]
pub enum DisplayEntry {
    Chat {
        role: DisplayRole,
        content: String,
        thinking: Option<String>,
    },
    Tool {
        call_id: String,
        name: String,
        args_display: String,
        result: Option<String>,
        diff_preview: Option<String>,
    },
}

pub fn format_tool_call_display(name: &str, arguments: &str) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    match name {
        "run_command" => {
            let cmd = args["command"].as_str().unwrap_or("?");
            format!("$ {}", cmd)
        }
        "read_file" => {
            let path = args["path"].as_str().unwrap_or("?");
            format!("read {}", path)
        }
        "write_file" => {
            let path = args["path"].as_str().unwrap_or("?");
            format!("write {}", path)
        }
        "edit_file" => {
            let path = args["path"].as_str().unwrap_or("?");
            if let Some(count) = args["count"].as_u64() {
                format!("edit {} ({} changes)", path, count)
            } else {
                format!("edit {}", path)
            }
        }
        "list_dir" => {
            let path = args["path"].as_str().unwrap_or(".");
            format!("ls {}", path)
        }
        _ => name.to_string(),
    }
}

pub fn format_approval_label(preview: &str) -> (String, bool) {
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

    let is_diff = preview.lines().count() > 1 || !preview.starts_with("$ ");

    (tool_label, is_diff)
}

pub fn format_args_brief(arguments: &str) -> String {
    let s: String = arguments.chars().take(60).collect();
    if arguments.len() > 60 {
        format!("{}...", s)
    } else {
        s
    }
}

/// Slash command definition shared across frontends.
#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

/// ── Module-scope anchor for TS export (don't call `export_all` on payload types) ──
#[allow(dead_code)]
#[derive(Debug, TS)]
#[ts(export_to = "api-generated.ts")]
struct _AllPayloads {
    init: InitPayload,
    thinking_delta: ThinkingDeltaPayload,
    text_delta: TextDeltaPayload,
    text_done: TextDonePayload,
    tool_calling: ToolCallingPayload,
    tool_result: ToolResultPayload,
    approval_pending: ApprovalPendingPayload,
    tool_rejected: ToolRejectedPayload,
    error: ErrorPayload,
    fatigue_update: FatigueUpdatePayload,
    usage_update: UsageUpdatePayload,
    usage_stats_loaded: UsageStatsLoadedPayload,
    waking_up: WakingUpPayload,
    config_loaded: ConfigLoadedPayload,
    config_saved: ConfigSavedPayload,
    data_dir: DataDirPayload,
}

#[cfg(test)]
mod tests {
    use ts_rs::TS;

    #[test]
    fn export_ts_types() {
        let cfg = ts_rs::Config::new()
            .with_out_dir("./angelica-gui/frontend/src/lib/")
            .with_large_int("number");
        super::_AllPayloads::export_all(&cfg).expect("failed to export TS types");
    }
}
