use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::memory::MemoryManager;
use crate::tools::Tool;

pub struct UpdateUserProfileTool {
    memory: Arc<MemoryManager>,
}

impl UpdateUserProfileTool {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
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

        match action {
            "append" => self.memory.append_user_profile(content),
            "overwrite" => self.memory.write_user_profile(content),
            _ => return Ok(format!("Unknown action: {}", action)),
        }
        Ok("User profile updated.".to_string())
    }
}
