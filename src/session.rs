use std::path::PathBuf;

use crate::config::SessionConfig;
use crate::llm::types::ChatMessage;

pub struct SessionManager {
    sessions_dir: PathBuf,
    current_session_id: String,
}

impl SessionManager {
    pub fn new(config: &SessionConfig) -> Self {
        let sessions_dir = PathBuf::from(&config.directory);
        let current_session_id = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        Self {
            sessions_dir,
            current_session_id,
        }
    }

    pub fn save_session(&self, messages: &[ChatMessage]) -> anyhow::Result<()> {
        let session_dir = self.sessions_dir.join(&self.current_session_id);
        std::fs::create_dir_all(&session_dir)?;

        let session_json = serde_json::to_string_pretty(messages)?;
        std::fs::write(session_dir.join("session.json"), &session_json)?;

        Ok(())
    }

    pub fn save_summary(&self, summary: &str) -> anyhow::Result<()> {
        let session_dir = self.sessions_dir.join(&self.current_session_id);
        std::fs::create_dir_all(&session_dir)?;
        std::fs::write(session_dir.join("summary.md"), summary)?;
        Ok(())
    }

    pub fn query_sessions(&self, keyword: &str, limit: usize) -> anyhow::Result<Vec<SessionEntry>> {
        if !self.sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let keyword_lower = keyword.to_lowercase();

        for entry in std::fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let session_id = entry.file_name().to_string_lossy().to_string();
            let session_json_path = entry.path().join("session.json");

            let content = if session_json_path.exists() {
                std::fs::read_to_string(&session_json_path).unwrap_or_default()
            } else {
                continue;
            };

            let matches = if keyword.is_empty() {
                true
            } else {
                content.to_lowercase().contains(&keyword_lower)
            };

            if matches {
                entries.push(SessionEntry {
                    session_id,
                    path: entry.path(),
                    preview: Self::extract_preview(&content),
                });
            }
        }

        entries.sort_by(|a, b| b.session_id.cmp(&a.session_id));
        entries.truncate(limit);
        Ok(entries)
    }

    pub fn load_session_messages(&self, session_id: &str) -> anyhow::Result<Vec<ChatMessage>> {
        let path = self.sessions_dir.join(session_id).join("session.json");
        let content = std::fs::read_to_string(&path)?;
        let messages: Vec<ChatMessage> = serde_json::from_str(&content)?;
        Ok(messages)
    }

    fn extract_preview(json_content: &str) -> String {
        serde_json::from_str::<Vec<ChatMessage>>(json_content)
            .ok()
            .and_then(|msgs| {
                msgs.iter()
                    .find(|m| m.role == "user")
                    .and_then(|m| m.content.clone())
            })
            .unwrap_or_else(|| "(no preview)".to_string())
            .chars()
            .take(100)
            .collect()
    }

    pub fn session_id(&self) -> &str {
        &self.current_session_id
    }
}

pub struct SessionEntry {
    pub session_id: String,
    pub path: PathBuf,
    pub preview: String,
}

impl std::fmt::Display for SessionEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.session_id, self.preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_manager(dir: &TempDir) -> SessionManager {
        SessionManager {
            sessions_dir: dir.path().join("sessions"),
            current_session_id: "2026-05-18_14-30-22".to_string(),
        }
    }

    #[test]
    fn save_and_load_session() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some("hello".to_string()),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("hi there".to_string()),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];

        mgr.save_session(&messages).unwrap();

        let loaded = mgr.load_session_messages("2026-05-18_14-30-22").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].content.as_deref(), Some("hello"));
    }

    #[test]
    fn query_sessions_by_keyword() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some("rust programming".to_string()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        mgr.save_session(&messages).unwrap();

        let results = mgr.query_sessions("rust", 10).unwrap();
        assert_eq!(results.len(), 1);

        let results = mgr.query_sessions("python", 10).unwrap();
        assert_eq!(results.len(), 0);
    }
}
