use async_trait::async_trait;
use serde_json::{Value, json};

use crate::memory::MemoryManager;
use crate::tools::Tool;

pub struct UpdateSoulTool {
    memory: std::sync::Arc<tokio::sync::RwLock<MemoryManager>>,
}

impl UpdateSoulTool {
    pub fn new(memory: std::sync::Arc<tokio::sync::RwLock<MemoryManager>>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for UpdateSoulTool {
    fn name(&self) -> &str {
        "update_soul"
    }

    fn description(&self) -> &str {
        "Update your personality and behavioral guidelines (SOUL.md). This changes how you behave in future interactions."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "New SOUL.md content in markdown format"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
        let mem = self.memory.read().await;
        mem.write_soul(content);
        Ok("SOUL.md updated. Changes will take effect in the next interaction.".to_string())
    }
}
