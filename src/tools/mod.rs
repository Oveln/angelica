pub mod edit_file;
pub mod list_dir;
pub mod query_sessions;
pub mod read_file;
pub mod run_command;
pub mod update_agent_memory;
pub mod update_soul;
pub mod update_user_profile;
pub mod write_file;

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::llm::types::ToolSpec;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn requires_approval(&self) -> bool {
        false
    }

    fn preview(&self, _args: Value) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn to_spec(&self) -> ToolSpec {
        ToolSpec::new(self.name(), self.description(), self.parameters())
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn all_specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.to_spec()).collect()
    }

    pub fn auto_execute_names(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|(_, t)| !t.requires_approval())
            .map(|(name, _)| name.clone())
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Produce a unified diff between old and new content.
/// Uses `similar::TextDiff` for proper hunks with context, matching the format
/// the TUI diff renderer expects (`--- a/`, `+++ b/`, `@@` hunk headers).
pub fn make_unified_diff(path: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let a = format!("a/{path}");
    let b = format!("b/{path}");
    let diff = similar::TextDiff::from_lines(old, new);
    let raw = diff
        .unified_diff()
        .context_radius(3)
        .header(&a, &b)
        .to_string();
    // Strip git's "\ No newline at end of file" markers — they're noise
    // that breaks the visual adjacency of removed/added pairs.
    raw.lines()
        .filter(|line| *line != "\\ No newline at end of file")
        .collect::<Vec<_>>()
        .join("\n")
}
