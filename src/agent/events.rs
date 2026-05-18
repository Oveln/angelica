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
        name: String,
        arguments: String,
    },
    ToolResult {
        name: String,
        result: String,
    },

    ApprovalPending {
        preview: String,
        is_tty_command: bool,
        command: Option<String>,
    },
    CommandResult {
        output: String,
    },
    CommandRejected {
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
    ApprovePendingWithResult { output: String },
    RejectCommand { feedback: Option<String> },
    Interrupt,
    ClearHistory,
    Quit,
}
