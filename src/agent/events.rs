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
        record: crate::usage::UsageRecord,
    },

    UsageStatsLoaded {
        sessions: Vec<crate::usage::SessionUsage>,
    },

    FallingAsleep,
    Sleeping,
    WakingUp {
        dream: String,
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
    UsageStats,
    RequestInit,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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
