pub mod tools;

use crate::llm::types::ChatMessage;

pub fn build_conversation_text(messages: &[ChatMessage]) -> String {
    if messages.is_empty() {
        return "（无对话记录）".to_string();
    }

    // Find the last context message index
    let last_context_idx = messages
        .iter()
        .rposition(|m| m.role == "system" && m.name.as_deref() == Some("context"));

    let mut text = String::new();
    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                let name = msg.name.as_deref().unwrap_or("");
                if name == "context" {
                    // Skip all context messages except the last one
                    if last_context_idx.is_some_and(|idx| std::ptr::eq(msg, &messages[idx]))
                        && let Some(content) = &msg.content
                    {
                        text.push_str(&format!("[context] {}\n", content));
                    }
                }
                // Skip system prompt messages
            }
            "user" => {
                let content = msg.content.as_deref().unwrap_or("");
                text.push_str(&format!("用户: {}\n", content));
            }
            "assistant" => {
                if let Some(content) = &msg.content {
                    text.push_str(&format!("祈芷: {}\n", content));
                }
            }
            "tool" => {
                if let Some(content) = &msg.content {
                    let preview: String = content.chars().take(200).collect();
                    text.push_str(&format!("[tool result] {}\n", preview));
                }
            }
            _ => {}
        }
    }
    text
}
