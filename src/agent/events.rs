use crate::session::SessionEntry;

#[derive(Debug, Clone)]
pub enum AppEvent {
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

    SessionList {
        sessions: Vec<SessionEntry>,
    },
    SessionLoaded {
        messages: Vec<crate::llm::types::ChatMessage>,
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
    ClearHistory,
    ResumeSession {
        session_id: String,
    },
    Quit,
}
