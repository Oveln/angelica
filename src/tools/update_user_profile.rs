use async_trait::async_trait;
use serde_json::{Value, json};

use crate::memory::MemoryManager;
use crate::tools::Tool;

pub struct UpdateUserProfileTool {
    memory: std::sync::Arc<tokio::sync::RwLock<MemoryManager>>,
}

impl UpdateUserProfileTool {
    pub fn new(memory: std::sync::Arc<tokio::sync::RwLock<MemoryManager>>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for UpdateUserProfileTool {
    fn name(&self) -> &str {
        "update_user_profile"
    }

    fn description(&self) -> &str {
        "Update the user profile/preferences. Use action='overwrite' to replace the profile."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["append", "overwrite"],
                    "description": "append to add, overwrite to replace"
                },
                "content": {
                    "type": "string",
                    "description": "Profile content to write"
                }
            },
            "required": ["action", "content"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'action'"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;

        let mem = self.memory.write().await;
        match action {
            "append" => mem.append_user_profile(content),
            "overwrite" => mem.write_user_profile(content),
            _ => return Ok(format!("Unknown action: {}", action)),
        }
        Ok("User profile updated.".to_string())
    }
}
