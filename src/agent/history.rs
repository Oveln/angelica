use crate::llm::types::{ChatMessage, ToolCall};

pub struct History {
    messages: Vec<ChatMessage>,
}

impl History {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn push(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    pub fn record_assistant(
        &mut self,
        content: Option<String>,
        reasoning: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
    ) {
        // DeepSeek rule: keep reasoning_content only when tool_calls present
        let reasoning_content = if tool_calls.is_some() {
            reasoning
        } else {
            None
        };

        self.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content,
            reasoning_content,
            tool_calls,
            tool_call_id: None,
            name: None,
        });
    }

    pub fn record_tool_result(&mut self, tool_call_id: String, content: String) {
        self.messages.push(ChatMessage {
            role: "tool".to_string(),
            content: Some(content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            name: None,
        });
    }

    /// Replace the tool result for a given tool_call_id.
    /// Used to update "Pending user approval..." with the actual output.
    pub fn update_tool_result(&mut self, tc_id: &str, content: String) {
        for msg in self.messages.iter_mut().rev() {
            if msg.tool_call_id.as_deref() == Some(tc_id) {
                msg.content = Some(content);
                return;
            }
        }
        // No existing message found — shouldn't happen, but add it as safety
        self.record_tool_result(tc_id.to_string(), content);
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn pop_last(&mut self) {
        self.messages.pop();
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Find the tool_call_id for the last `run_command` in an assistant message's tool_calls.
    pub fn find_last_command_tc_id(&self) -> Option<String> {
        for msg in self.messages.iter().rev() {
            if let Some(tcs) = &msg.tool_calls {
                for tc in tcs {
                    if tc.function.name == "run_command" {
                        return Some(tc.id.clone());
                    }
                }
            }
        }
        None
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}
