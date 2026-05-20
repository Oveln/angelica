use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::memory::MemoryManager;
use crate::tools::Tool;

pub struct NotebookTool {
    memory: Arc<MemoryManager>,
}

impl NotebookTool {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for NotebookTool {
    fn name(&self) -> &str {
        "notebook"
    }

    fn description(&self) -> &str {
        "你的私人笔记本。可以写、读、搜索、编辑。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["write", "read", "edit", "search", "append"],
                    "description": "write=覆盖写入, read=读取全部, edit=搜索替换, search=关键词搜索, append=追加新条目"
                },
                "content": {
                    "type": "string",
                    "description": "内容（write/append/edit 的 replace 时使用）"
                },
                "search": {
                    "type": "string",
                    "description": "搜索文本（edit 模式的搜索串, 或 search 模式的关键词）"
                },
                "replace": {
                    "type": "string",
                    "description": "替换文本（edit 模式使用）"
                }
            },
            "required": ["action"]
        })
    }

    fn default_rules(&self) -> Vec<crate::permission::TargetRule> {
        use crate::permission::{PermissionAction, TargetRule};
        vec![TargetRule {
            target: "*".to_string(),
            action: PermissionAction::Allow,
        }]
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'action'"))?;

        match action {
            "read" => {
                let content = self.memory.read_notebook();
                if content.is_empty() {
                    Ok("笔记本是空的。".to_string())
                } else {
                    Ok(content)
                }
            }
            "write" => {
                let content = args["content"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing 'content' for write"))?;
                self.memory.write_notebook(content);
                Ok("笔记本已写入。".to_string())
            }
            "append" => {
                let content = args["content"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing 'content' for append"))?;
                self.memory.append_notebook(content);
                Ok("已追加到笔记本。".to_string())
            }
            "edit" => {
                let search = args["search"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing 'search' for edit"))?;
                let replace = args["replace"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing 'replace' for edit"))?;
                self.memory.edit_notebook(search, replace)
            }
            "search" => {
                let keyword = args["search"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing 'search' keyword"))?;
                Ok(self.memory.search_notebook(keyword))
            }
            _ => Err(anyhow::anyhow!("Unknown action: {}", action)),
        }
    }
}
