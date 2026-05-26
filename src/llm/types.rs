use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::usage::UsageMetrics>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            usage: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            usage: None,
        }
    }

    pub fn assistant(
        content: Option<String>,
        reasoning: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
        usage: Option<crate::usage::UsageMetrics>,
    ) -> Self {
        let reasoning_content = if tool_calls.is_some() {
            reasoning
        } else {
            None
        };
        Self {
            role: Role::Assistant,
            content,
            reasoning_content,
            tool_calls,
            tool_call_id: None,
            name: None,
            usage,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
            usage: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

impl ToolSpec {
    pub fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            function: ToolFunction {
                name: name.to_string(),
                description: Some(description.to_string()),
                parameters: Some(parameters),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_chat_message() {
        let msg = ChatMessage::user("hello");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"hello\""));
        assert!(!json.contains("reasoning_content"));
        assert!(!json.contains("tool_calls"));
    }

    #[test]
    fn serialize_assistant_with_tool_calls() {
        let msg = ChatMessage::assistant(
            Some("let me check".to_string()),
            Some("thinking...".to_string()),
            Some(vec![ToolCall {
                id: "call_123".to_string(),
                function: FunctionCall {
                    name: "run_command".to_string(),
                    arguments: r#"{"command":"ls"}"#.to_string(),
                },
            }]),
            None,
        );
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("reasoning_content"));
        assert!(json.contains("tool_calls"));
    }

    #[test]
    fn role_serde_roundtrip() {
        for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
            let json = serde_json::to_string(&role).unwrap();
            let back: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(role, back);
        }
    }
}
