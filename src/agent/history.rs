use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::llm::types::{ChatMessage, ToolCall};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimedMessage {
    pub ts: String,
    #[serde(flatten)]
    pub message: ChatMessage,
}

pub struct History {
    messages: Vec<ChatMessage>,
    path: PathBuf,
    buf_writer: Option<std::io::BufWriter<std::fs::File>>,
}

impl History {
    pub fn new(path: PathBuf) -> Self {
        Self {
            messages: Vec::new(),
            path,
            buf_writer: None,
        }
    }

    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let messages = if path.exists() {
            let file = std::fs::File::open(&path)?;
            let reader = std::io::BufReader::new(file);
            let mut loaded = Vec::new();
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<TimedMessage>(&line) {
                    Ok(tm) => loaded.push(tm.message),
                    Err(e) => {
                        tracing::warn!("Skipping malformed history line: {}", e);
                    }
                }
            }
            loaded
        } else {
            Vec::new()
        };
        Ok(Self {
            messages,
            path,
            buf_writer: None,
        })
    }

    fn ensure_writer(&mut self) {
        if self.buf_writer.is_none() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            if let Ok(file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                self.buf_writer = Some(std::io::BufWriter::new(file));
            }
        }
    }

    fn flush(&mut self) {
        if let Some(ref mut w) = self.buf_writer
            && let Err(e) = w.flush()
        {
            tracing::warn!("Failed to flush history: {}", e);
        }
    }

    pub fn push(&mut self, msg: ChatMessage) {
        self.append_to_file(&msg);
        self.messages.push(msg);
    }

    fn append_to_file(&mut self, msg: &ChatMessage) {
        self.ensure_writer();
        let tm = TimedMessage {
            ts: chrono::Local::now().to_rfc3339(),
            message: msg.clone(),
        };
        match serde_json::to_string(&tm) {
            Ok(json) => {
                if let Some(ref mut w) = self.buf_writer
                    && let Err(e) = writeln!(w, "{}", json)
                {
                    tracing::warn!("Failed to append history: {}", e);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to serialize history entry: {}", e);
            }
        }
    }

    pub fn record_assistant(
        &mut self,
        content: Option<String>,
        reasoning: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
    ) {
        let reasoning_content = if tool_calls.is_some() {
            reasoning
        } else {
            None
        };

        self.push(ChatMessage {
            role: "assistant".to_string(),
            content,
            reasoning_content,
            tool_calls,
            tool_call_id: None,
            name: None,
        });
    }

    pub fn record_tool_result(&mut self, tool_call_id: String, content: String) {
        self.push(ChatMessage {
            role: "tool".to_string(),
            content: Some(content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            name: None,
        });
    }

    pub fn update_tool_result(&mut self, tc_id: &str, content: String) {
        for msg in self.messages.iter_mut().rev() {
            if msg.tool_call_id.as_deref() == Some(tc_id) {
                msg.content = Some(content.clone());
                self.rewrite_file();
                return;
            }
        }
        self.record_tool_result(tc_id.to_string(), content);
    }

    fn rewrite_file(&mut self) {
        self.buf_writer = None;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(file) = std::fs::File::create(&self.path) {
            let mut writer = std::io::BufWriter::new(file);
            for msg in &self.messages {
                let tm = TimedMessage {
                    ts: chrono::Local::now().to_rfc3339(),
                    message: msg.clone(),
                };
                if let Ok(json) = serde_json::to_string(&tm) {
                    let _ = writeln!(writer, "{}", json);
                }
            }
            if let Err(e) = writer.flush() {
                tracing::warn!("Failed to flush rewritten history: {}", e);
            }
        }
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.buf_writer = None;
        if self.path.exists()
            && let Ok(file) = std::fs::File::create(&self.path)
        {
            drop(file);
        }
    }

    pub fn find_last_command_tc_id(&self) -> Option<String> {
        for msg in self.messages.iter().rev() {
            if let Some(tcs) = &msg.tool_calls {
                for tc in tcs {
                    if tc.function.name == "run_command" {
                        return Some(tc.id.clone());
                    }
                }
            }
        }
        None
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(PathBuf::from("data/conversation.jsonl"))
    }
}

impl Drop for History {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_and_load_jsonl() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("conversation.jsonl");

        {
            let mut history = History::new(path.clone());
            history.push(ChatMessage {
                role: "user".to_string(),
                content: Some("hello".to_string()),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: Some("user".to_string()),
            });
            history.push(ChatMessage {
                role: "assistant".to_string(),
                content: Some("hi".to_string()),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        let loaded = History::load(path).unwrap();
        assert_eq!(loaded.messages().len(), 2);
        assert_eq!(loaded.messages()[0].content.as_deref(), Some("hello"));
        assert_eq!(loaded.messages()[1].content.as_deref(), Some("hi"));
        assert_eq!(loaded.messages()[0].name.as_deref(), Some("user"));
    }

    #[test]
    fn record_assistant_with_tool_calls() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("conversation.jsonl");
        let mut history = History::new(path);

        history.record_assistant(
            Some("checking".to_string()),
            Some("thinking...".to_string()),
            Some(vec![crate::llm::types::ToolCall {
                id: "call_1".to_string(),
                function: crate::llm::types::FunctionCall {
                    name: "read_file".to_string(),
                    arguments: r#"{"path":"test.rs"}"#.to_string(),
                },
            }]),
        );

        assert_eq!(history.messages().len(), 1);
        assert!(history.messages()[0].reasoning_content.is_some());
        assert!(history.messages()[0].tool_calls.is_some());
    }

    #[test]
    fn record_assistant_without_tool_calls_drops_reasoning() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("conversation.jsonl");
        let mut history = History::new(path);

        history.record_assistant(
            Some("done".to_string()),
            Some("thinking...".to_string()),
            None,
        );

        assert_eq!(history.messages().len(), 1);
        assert!(history.messages()[0].reasoning_content.is_none());
    }

    #[test]
    fn update_tool_result_persists() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("conversation.jsonl");
        let mut history = History::new(path.clone());

        history.record_tool_result("tc_1".to_string(), "Pending...".to_string());
        history.update_tool_result("tc_1", "Done!".to_string());

        assert_eq!(history.messages()[0].content.as_deref(), Some("Done!"));

        let loaded = History::load(path).unwrap();
        assert_eq!(loaded.messages()[0].content.as_deref(), Some("Done!"));
    }

    #[test]
    fn clear_truncates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("conversation.jsonl");

        {
            let mut history = History::new(path.clone());
            history.push(ChatMessage {
                role: "user".to_string(),
                content: Some("hello".to_string()),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().len() > 0);

        let mut history = History::load(path.clone()).unwrap();
        history.clear();
        assert!(path.exists());
        assert!(history.messages().is_empty());

        let loaded = History::load(path.clone()).unwrap();
        assert!(loaded.messages().is_empty());
    }
}
