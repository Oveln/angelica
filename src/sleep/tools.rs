use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::memory::MemoryManager;
use crate::tools::Tool;

/// A dreaming tool that captures the agent's dream during sleep.
/// The dream content is stored in a shared Arc<Mutex<Option<String>>>.
pub struct DreamTool {
    dream: Arc<Mutex<Option<String>>>,
}

impl DreamTool {
    pub fn new(dream: Arc<Mutex<Option<String>>>) -> Self {
        Self { dream }
    }
}

#[async_trait]
impl Tool for DreamTool {
    fn name(&self) -> &str {
        "dreaming"
    }

    fn description(&self) -> &str {
        "记录你的梦境。当你整理完思绪，想随口说点什么的时候，调用这个工具。这会是你在睡眠中留下的痕迹，醒来后会残留淡淡的余韵。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "dream": {
                    "type": "string",
                    "description": "你的梦——可以是任何东西，不必和今天的事有关。一段感受、一个画面、一句自言自语……"
                }
            },
            "required": ["dream"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let dream = args["dream"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'dream'"))?;
        *self.dream.lock().expect("dream lock poisoned") = Some(dream.to_string());
        Ok("梦已记录。晚安。".to_string())
    }
}

macro_rules! define_edit_tool {
    ($name:ident, $tool_name:expr, $desc:expr, $read_fn:ident, $write_fn:ident) => {
        pub struct $name {
            memory: Arc<MemoryManager>,
        }

        impl $name {
            pub fn new(memory: Arc<MemoryManager>) -> Self {
                Self { memory }
            }
        }

        #[async_trait]
        impl Tool for $name {
            fn name(&self) -> &str {
                $tool_name
            }

            fn description(&self) -> &str {
                $desc
            }

            fn parameters(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["edit", "append", "rewrite"],
                            "description": "edit=搜索替换, append=追加, rewrite=重写全部"
                        },
                        "search": {
                            "type": "string",
                            "description": "要搜索的文本（edit 模式）"
                        },
                        "replace": {
                            "type": "string",
                            "description": "替换后的文本（edit 模式）"
                        },
                        "content": {
                            "type": "string",
                            "description": "新内容（append/rewrite 模式）"
                        }
                    },
                    "required": ["action"]
                })
            }

            async fn execute(&self, args: Value) -> anyhow::Result<String> {
                let action = args["action"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing 'action'"))?;

                match action {
                    "edit" => {
                        let search = args["search"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("missing 'search' for edit"))?;
                        let replace = args["replace"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("missing 'replace' for edit"))?;
                        let content = self.memory.$read_fn();
                        if search == replace {
                            return Err(anyhow::anyhow!("search and replace are identical"));
                        }
                        let count = content.matches(search).count();
                        if count == 0 {
                            return Err(anyhow::anyhow!(
                                "未找到匹配文本。当前文件内容：\n{}",
                                &content[..content.floor_char_boundary(500)]
                            ));
                        }
                        if count > 1 {
                            return Err(anyhow::anyhow!(
                                "找到 {} 处匹配，需要更具体的搜索文本",
                                count
                            ));
                        }
                        let updated = content.replacen(search, replace, 1);
                        self.memory.$write_fn(&updated);
                        Ok(format!("{}已更新。", $tool_name))
                    }
                    "append" => {
                        let content = args["content"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("missing 'content' for append"))?;
                        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
                        let entry = format!("## {}\n{}\n", date, content);
                        let existing = self.memory.$read_fn();
                        let updated = format!("{}\n{}", existing.trim_end(), entry);
                        self.memory.$write_fn(&updated);
                        Ok(format!("已追加到{}。", $tool_name))
                    }
                    "rewrite" => {
                        let content = args["content"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("missing 'content' for rewrite"))?;
                        self.memory.$write_fn(content);
                        Ok(format!("{}已重写。", $tool_name))
                    }
                    _ => Err(anyhow::anyhow!("Unknown action: {}", action)),
                }
            }
        }
    };
}

define_edit_tool!(
    EditSoulTool,
    "edit_soul",
    "审视你的性格、行为方式、处世态度、世界观。",
    read_soul,
    write_soul
);
define_edit_tool!(
    EditMemoryTool,
    "edit_memory",
    "整理你的记忆。",
    read_memory,
    write_memory
);
define_edit_tool!(
    EditProfileTool,
    "edit_profile",
    "更新你对用户的认知。",
    read_user_profile,
    write_user_profile
);
