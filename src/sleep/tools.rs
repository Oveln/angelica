use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::episode::Episode;
use crate::memory::MemoryManager;
use crate::tools::Tool;

/// A dreaming tool that captures the agent's dream during sleep.
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

/// Sleep tool for writing episode entries.
pub struct WriteEpisodeTool {
    memory: Arc<MemoryManager>,
}

impl WriteEpisodeTool {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for WriteEpisodeTool {
    fn name(&self) -> &str {
        "write_episode"
    }

    fn description(&self) -> &str {
        "将一段经历或感悟写入记忆的情景区。每条 episode 只聚焦一个主题或一次重要的对话——不要试图把一整天塞进一条。清醒期有多个值得记住的片段时，分别写成多条。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "date": {
                    "type": "string",
                    "description": "日期，格式 YYYY-MM-DD"
                },
                "content": {
                    "type": "string",
                    "description": "要记录的内容"
                },
                "emotional_weight": {
                    "type": "integer",
                    "description": "情感权重 1-5，默认 3",
                    "minimum": 1,
                    "maximum": 5
                },
                "afterglow": {
                    "type": "string",
                    "description": "这段经历的余韵"
                }
            },
            "required": ["date", "content"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let date = args["date"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'date'"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;

        let weight = args.get("emotional_weight")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .unwrap_or(3);
        let afterglow = args.get("afterglow")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut ep = Episode::new(date.to_string(), content.to_string());
        ep.emotional_weight = weight.clamp(1, 5);
        if !afterglow.is_empty() {
            ep.afterglow = afterglow;
        }

        self.memory.append_episode(&ep)?;
        Ok(format!("已记录情景「{}」。", date))
    }
}
