use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::agent::events::{DisplayEntry, DisplayRole, format_args_brief};
use crate::llm::types::{ChatMessage, Role, ToolCall};

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
        usage: Option<crate::usage::UsageMetrics>,
    ) {
        self.push(ChatMessage::assistant(
            content, reasoning, tool_calls, usage,
        ));
    }

    pub fn record_tool_result(&mut self, tool_call_id: String, content: String) {
        self.push(ChatMessage::tool_result(tool_call_id, content));
    }

    pub fn update_tool_result(&mut self, tc_id: &str, content: String) {
        for msg in self.messages.iter_mut().rev() {
            if msg.tool_call_id.as_deref() == Some(tc_id) {
                msg.content = Some(content.clone());
                self.patch_line_in_file(tc_id, &content);
                return;
            }
        }
        self.record_tool_result(tc_id.to_string(), content);
    }

    /// Update a single line in the JSONL file matching the given tool_call_id.
    /// Avoids rewriting the entire file, preserving original timestamps on other lines.
    fn patch_line_in_file(&mut self, tc_id: &str, content: &str) {
        self.flush();
        let Ok(file_content) = std::fs::read_to_string(&self.path) else {
            return;
        };
        let mut patched = false;
        let mut out = String::with_capacity(file_content.len());
        for line in file_content.lines() {
            if patched || line.trim().is_empty() {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if let Ok(tm) = serde_json::from_str::<TimedMessage>(line)
                && tm.message.tool_call_id.as_deref() == Some(tc_id)
            {
                let updated = TimedMessage {
                    ts: tm.ts,
                    message: ChatMessage {
                        content: Some(content.to_string()),
                        ..tm.message
                    },
                };
                if let Ok(json) = serde_json::to_string(&updated) {
                    out.push_str(&json);
                    out.push('\n');
                    patched = true;
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        if patched {
            // Invalidate buf_writer since we're replacing the file
            self.buf_writer = None;
            let _ = std::fs::write(&self.path, out);
        }
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn to_display_entries(&self) -> Vec<DisplayEntry> {
        let mut entries = Vec::new();
        for msg in &self.messages {
            match msg.role {
                Role::User => {
                    let raw = msg.content.as_deref().unwrap_or("");
                    let content = strip_baked_context(raw);
                    if content.is_empty() {
                        continue;
                    }
                    entries.push(DisplayEntry::Chat {
                        role: DisplayRole::User,
                        content: content.to_string(),
                        thinking: None,
                    });
                }
                Role::Assistant => {
                    let content = msg.content.as_deref().unwrap_or("");
                    if content.is_empty() && msg.tool_calls.is_none() {
                        continue;
                    }
                    if !content.is_empty() {
                        entries.push(DisplayEntry::Chat {
                            role: DisplayRole::Assistant,
                            content: content.to_string(),
                            thinking: msg.reasoning_content.clone(),
                        });
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tc in tool_calls {
                            let display = format!(
                                "{}({})",
                                tc.function.name,
                                format_args_brief(&tc.function.arguments)
                            );
                            entries.push(DisplayEntry::Tool {
                                call_id: tc.id.clone(),
                                name: tc.function.name.clone(),
                                args_display: display,
                                result: None,
                                diff_preview: None,
                            });
                        }
                    }
                }
                Role::Tool => {
                    if let Some(DisplayEntry::Tool { result, .. }) = entries.iter_mut().rev().find(
                        |e| matches!(e, DisplayEntry::Tool { call_id, .. } if *call_id == msg.tool_call_id.as_deref().unwrap_or("")),
                    ) {
                        *result = Some(
                            msg.content
                                .as_deref()
                                .unwrap_or("")
                                .chars()
                                .take(200)
                                .collect(),
                        );
                    }
                }
                _ => {}
            }
        }
        entries
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

fn strip_baked_context(content: &str) -> &str {
    const MARKER: &str = "[以下是用户的输入]\n";
    if let Some(pos) = content.find(MARKER) {
        content[pos + MARKER.len()..].trim_start()
    } else {
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::Role;
    use tempfile::TempDir;

    #[test]
    fn save_and_load_jsonl() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("conversation.jsonl");

        {
            let mut history = History::new(path.clone());
            let mut user_msg = ChatMessage::user("hello");
            user_msg.name = Some("user".to_string());
            history.push(user_msg);
            history.push(ChatMessage::assistant(
                Some("hi".to_string()),
                None,
                None,
                None,
            ));
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
            None,
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
            history.push(ChatMessage::user("hello"));
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

    /// Simulates: tool call with invalid JSON → tool result with error recorded
    #[test]
    fn invalid_tool_call_records_error_result() {
        let dir = TempDir::new().unwrap();
        let mut history = History::new(dir.path().join("conversation.jsonl"));

        history.record_tool_result(
            "call_bad_json".to_string(),
            "Invalid JSON in tool call arguments: expected value at line 1 column 1".to_string(),
        );

        assert_eq!(history.messages().len(), 1);
        let msg = &history.messages()[0];
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_bad_json"));
        assert!(msg.content.as_ref().unwrap().contains("Invalid JSON"));
    }

    /// Simulates: permission denied → tool result with denial message
    #[test]
    fn permission_denied_records_rejection_result() {
        let dir = TempDir::new().unwrap();
        let mut history = History::new(dir.path().join("conversation.jsonl"));

        history.record_tool_result(
            "call_denied".to_string(),
            "Tool 'run_command' denied by permission policy.".to_string(),
        );

        assert_eq!(history.messages().len(), 1);
        assert!(
            history.messages()[0]
                .content
                .as_ref()
                .unwrap()
                .contains("denied")
        );
    }

    /// Simulates: approval pending → approve → result updated
    #[test]
    fn approval_pending_then_approve_updates_result() {
        let dir = TempDir::new().unwrap();
        let mut history = History::new(dir.path().join("conversation.jsonl"));

        history.record_tool_result(
            "call_approve".to_string(),
            "Pending user approval...".to_string(),
        );
        assert_eq!(
            history.messages()[0].content.as_deref(),
            Some("Pending user approval...")
        );

        history.update_tool_result("call_approve", "File written successfully.".to_string());
        assert_eq!(
            history.messages()[0].content.as_deref(),
            Some("File written successfully.")
        );
    }

    /// Simulates: approval pending → reject → result updated with feedback
    #[test]
    fn approval_pending_then_reject_updates_result() {
        let dir = TempDir::new().unwrap();
        let mut history = History::new(dir.path().join("conversation.jsonl"));

        history.record_tool_result(
            "call_reject".to_string(),
            "Pending user approval...".to_string(),
        );
        history.update_tool_result(
            "call_reject",
            "User rejected this operation. Feedback: too risky".to_string(),
        );
        assert!(
            history.messages()[0]
                .content
                .as_ref()
                .unwrap()
                .contains("rejected")
        );
        assert!(
            history.messages()[0]
                .content
                .as_ref()
                .unwrap()
                .contains("too risky")
        );
    }

    /// Simulates: batched edit → only one pending result per tc_id, all updated on approve
    #[test]
    fn batched_edit_single_approval_flow() {
        let dir = TempDir::new().unwrap();
        let mut history = History::new(dir.path().join("conversation.jsonl"));

        for id in ["tc_1", "tc_2", "tc_3"] {
            history.record_tool_result(id.to_string(), "Pending user approval...".to_string());
        }

        let result = "3 edits applied to a.rs";
        for id in ["tc_1", "tc_2", "tc_3"] {
            history.update_tool_result(id, result.to_string());
        }

        for msg in history.messages() {
            assert_eq!(msg.content.as_deref(), Some(result));
        }
    }

    #[test]
    fn strip_context_basic() {
        assert_eq!(
            strip_baked_context(
                "[以下为系统上下文，不是用户的输入]\n当前时间：2026-05-22\n你的状态：精神饱满。\n\n[以下是用户的输入]\n感觉如何"
            ),
            "感觉如何"
        );
    }

    #[test]
    fn no_double_newline_passthrough() {
        assert_eq!(strip_baked_context("普通消息"), "普通消息");
    }

    #[test]
    fn empty_after_strip() {
        assert_eq!(
            strip_baked_context(
                "[以下为系统上下文，不是用户的输入]\n上下文\n\n[以下是用户的输入]\n"
            ),
            ""
        );
    }
}
