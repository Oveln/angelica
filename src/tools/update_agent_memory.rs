use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::memory::MemoryManager;
use crate::tools::Tool;

pub struct UpdateAgentMemoryTool {
    memory: Arc<MemoryManager>,
}

impl UpdateAgentMemoryTool {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for UpdateAgentMemoryTool {
    fn name(&self) -> &str {
        "update_agent_memory"
    }

    fn description(&self) -> &str {
        "Update your own memory. Use action='append' to add a new entry, or action='overwrite' to replace the entire memory."
    }

    fn default_rules(&self) -> Vec<crate::permission::TargetRule> {
        use crate::permission::{PermissionAction, TargetRule};
        vec![TargetRule {
            target: "*".to_string(),
            action: PermissionAction::Allow,
        }]
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["append", "overwrite"],
                    "description": "append to add a new entry, overwrite to replace all"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
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
            "append" => self.memory.append_agent_memory(content),
            "overwrite" => self.memory.write_agent_memory(content),
            _ => return Ok(format!("Unknown action: {}", action)),
        }
        Ok("Memory updated.".to_string())
    }
}
