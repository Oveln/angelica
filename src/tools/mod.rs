pub mod edit_file;
pub mod list_dir;
pub mod notebook;
pub mod read_file;
pub mod recall;
pub mod run_command;
pub mod skill;
pub mod write_file;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::llm::types::ToolSpec;
use crate::memory::MemoryManager;
use crate::permission::TargetRule;
use crate::skills::SkillRegistry;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;

    fn preview(&self, _args: Value) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn to_spec(&self) -> ToolSpec {
        ToolSpec::new(self.name(), self.description(), self.parameters())
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String>;

    fn permission_target(&self, _args: &Value) -> Option<String> {
        None
    }

    fn default_rules(&self) -> Vec<TargetRule> {
        vec![]
    }
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

    pub fn with_awake_defaults(
        memory: Arc<MemoryManager>,
        skills: Arc<SkillRegistry>,
        archive_dir: &str,
        conversation_path: &str,
    ) -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(read_file::ReadFileTool));
        reg.register(Box::new(write_file::WriteFileTool));
        reg.register(Box::new(edit_file::EditFileTool));
        reg.register(Box::new(list_dir::ListDirTool));
        reg.register(Box::new(run_command::RunCommandTool));
        reg.register(Box::new(skill::SkillTool::new(skills)));
        reg.register(Box::new(notebook::NotebookTool::new(memory)));
        reg.register(Box::new(recall::RecallTool::new(
            std::path::PathBuf::from(archive_dir),
            std::path::PathBuf::from(conversation_path),
        )));
        reg
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

    pub fn builtin_rules(&self) -> Vec<(String, Vec<TargetRule>)> {
        self.tools
            .iter()
            .map(|(name, tool)| (name.clone(), tool.default_rules()))
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

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
    raw.lines()
        .filter(|line| *line != "\\ No newline at end of file")
        .collect::<Vec<_>>()
        .join("\n")
}
