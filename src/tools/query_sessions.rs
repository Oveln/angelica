use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::session::SessionManager;
use crate::tools::Tool;

pub struct QuerySessionsTool {
    sessions: Arc<SessionManager>,
}

impl QuerySessionsTool {
    pub fn new(sessions: Arc<SessionManager>) -> Self {
        Self { sessions }
    }
}

#[async_trait]
impl Tool for QuerySessionsTool {
    fn name(&self) -> &str {
        "query_sessions"
    }

    fn description(&self) -> &str {
        "Search past conversation sessions by keyword. Returns a list of matching sessions with previews."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "keyword": {
                    "type": "string",
                    "description": "Search keyword to filter sessions. Empty string returns all recent sessions."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default 10)",
                    "default": 10
                }
            },
            "required": ["keyword"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let keyword = args["keyword"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'keyword'"))?;
        let limit = args["limit"].as_u64().unwrap_or(10) as usize;

        let entries = self.sessions.query_sessions(keyword, limit)?;

        if entries.is_empty() {
            return Ok("No matching sessions found.".to_string());
        }

        let mut result = format!("Found {} session(s):\n\n", entries.len());
        for entry in &entries {
            result.push_str(&format!("{}\n", entry));
        }
        Ok(result)
    }
}
