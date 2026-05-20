use std::io::BufRead;
use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::agent::history::TimedMessage;
use crate::tools::Tool;

pub struct RecallTool {
    archive_dir: PathBuf,
    conversation_path: PathBuf,
}

impl RecallTool {
    pub fn new(archive_dir: PathBuf, conversation_path: PathBuf) -> Self {
        Self {
            archive_dir,
            conversation_path,
        }
    }
}

#[async_trait]
impl Tool for RecallTool {
    fn name(&self) -> &str {
        "recall"
    }

    fn description(&self) -> &str {
        "回忆过往的对话。搜索你的历史。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "keyword": {
                    "type": "string",
                    "description": "搜索关键词"
                },
                "limit": {
                    "type": "integer",
                    "description": "最大结果数（默认 5）",
                    "default": 5
                }
            },
            "required": ["keyword"]
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
        let keyword = args["keyword"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'keyword'"))?;
        let limit = args["limit"].as_u64().unwrap_or(5) as usize;

        let mut results: Vec<String> = Vec::new();
        let keyword_lower = keyword.to_lowercase();

        // Search current conversation
        if self.conversation_path.exists() {
            if let Ok(matches) = self.search_jsonl(&self.conversation_path, &keyword_lower, limit) {
                results.extend(matches);
            }
        }

        // Search archive files
        if self.archive_dir.exists() {
            let mut entries: Vec<_> = std::fs::read_dir(&self.archive_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
                .collect();
            entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

            for entry in entries {
                if results.len() >= limit {
                    break;
                }
                if let Ok(matches) =
                    self.search_jsonl(&entry.path(), &keyword_lower, limit - results.len())
                {
                    results.extend(matches);
                }
            }
        }

        if results.is_empty() {
            Ok(format!("没有找到和「{}」相关的记忆。", keyword))
        } else {
            Ok(format!(
                "找到 {} 条相关记忆：\n\n{}",
                results.len(),
                results.join("\n---\n")
            ))
        }
    }
}

impl RecallTool {
    fn search_jsonl(
        &self,
        path: &PathBuf,
        keyword_lower: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut results = Vec::new();

        for line in reader.lines() {
            if results.len() >= limit {
                break;
            }
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(tm) = serde_json::from_str::<TimedMessage>(&line) {
                let content = tm.message.content.unwrap_or_default();
                if content.to_lowercase().contains(keyword_lower) {
                    let preview: String = content.chars().take(200).collect();
                    results.push(format!("[{}]\n{}", tm.ts, preview));
                }
            }
        }
        Ok(results)
    }
}
