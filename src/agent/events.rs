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
    },

    ApprovalPending {
        call_id: String,
        tool_name: String,
        preview: String,
    },
    ToolRejected {
        call_id: String,
        feedback: String,
    },

    Error {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum UserAction {
    SendMessage { content: String },
    ApprovePending,
    RejectTool { feedback: Option<String> },
    ClearHistory,
    Quit,
}
