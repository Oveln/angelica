#[derive(Debug, Clone)]
pub enum AppEvent {
    Init {
        messages: Vec<HistoryEntry>,
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
        arguments: String,
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

    // Life state events
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
    Quit,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryEntry {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<crate::llm::types::Context>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<crate::llm::types::ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl From<&crate::llm::types::ChatMessage> for HistoryEntry {
    fn from(m: &crate::llm::types::ChatMessage) -> Self {
        Self {
            role: m.role.as_str().to_string(),
            content: m.content.clone(),
            context: m.context.clone(),
            reasoning_content: m.reasoning_content.clone(),
            tool_calls: m.tool_calls.clone(),
            tool_call_id: m.tool_call_id.clone(),
            name: m.name.clone(),
        }
    }
}
