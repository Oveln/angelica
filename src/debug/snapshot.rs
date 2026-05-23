use serde::Serialize;

impl Default for DebugSnapshot {
    fn default() -> Self {
        Self {
            mode: "initializing".to_string(),
            history_messages: 0,
            tool_count: 0,
            iteration: 0,
            tool_queue_len: 0,
            fatigue: 0.0,
            fatigue_desc: String::new(),
            turns: 0,
            tool_calls: 0,
            recall_top_score: 0.0,
            recall_text_preview: String::new(),
            last_prompt_tokens: None,
            last_completion_tokens: None,
            context_messages: Vec::new(),
        }
    }
}

/// A lightweight snapshot of agent state, captured before each LLM call
/// and broadcast via `tokio::watch` to the debug HTTP server.
#[derive(Debug, Clone, Serialize)]
pub struct DebugSnapshot {
    /// Current agent mode: "awake" or "sleeping"
    pub mode: String,
    /// Number of messages in the conversation history
    pub history_messages: usize,
    /// Number of registered tools
    pub tool_count: usize,
    /// Current iteration within this step
    pub iteration: usize,
    /// Items in the tool queue awaiting processing
    pub tool_queue_len: usize,

    // Fatigue
    pub fatigue: f64,
    pub fatigue_desc: String,
    pub turns: u32,
    pub tool_calls: u32,

    // Recall
    pub recall_top_score: f32,
    pub recall_text_preview: String,

    // Token usage (last response)
    pub last_prompt_tokens: Option<u64>,
    pub last_completion_tokens: Option<u64>,

    /// The full list of messages sent to the LLM in the last call
    pub context_messages: Vec<ContextMessage>,
}

/// A simplified view of a ChatMessage for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct ContextMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Character length of the content
    pub content_length: usize,
    /// Full content text (may be large)
    pub content_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl DebugSnapshot {
    /// Total character size of all context messages combined.
    pub fn total_context_chars(&self) -> usize {
        self.context_messages.iter().map(|m| m.content_length).sum()
    }
}
