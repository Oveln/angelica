use std::sync::Arc;

use crate::agent::modes::RunMode;
use crate::llm::types::{ChatMessage, ToolSpec};
use crate::memory::MemoryManager;
use crate::permission::TargetRule;
use crate::prompt::{PromptBuilder, SleepingPromptBuilder};
use crate::skills::SkillRegistry;
use crate::sleep::tools;
use crate::tools::Tool;
use crate::tools::ToolRegistry;

const SLEEP_MAX_ITERATIONS: usize = 10;

pub struct SleepingMode {
    tools: ToolRegistry,
    prompt_builder: SleepingPromptBuilder,
    dream: Option<String>,
}

impl SleepingMode {
    pub fn new(
        memory: Arc<MemoryManager>,
        conversation_summary: String,
        turns: u32,
        tool_calls: u32,
        fatigue_desc: String,
    ) -> Self {
        let prompt_builder =
            SleepingPromptBuilder::new(conversation_summary, turns, tool_calls, fatigue_desc);

        let mut reg = ToolRegistry::new();
        reg.register(Box::new(tools::EditSoulTool::new(memory.clone())));
        reg.register(Box::new(tools::EditMemoryTool::new(memory.clone())));
        reg.register(Box::new(tools::EditProfileTool::new(memory)));

        Self {
            tools: reg,
            prompt_builder,
            dream: None,
        }
    }

    pub fn take_dream(&mut self) -> Option<String> {
        self.dream.take()
    }
}

impl RunMode for SleepingMode {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        self.tools.all_specs()
    }

    fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name)
    }

    fn build_system_message(&self, memory: &MemoryManager, skills: &SkillRegistry) -> ChatMessage {
        self.prompt_builder.build_system_message(memory, skills)
    }

    fn build_context_message(&self, _fatigue_desc: &str, _wake_dream: Option<&str>) -> ChatMessage {
        self.prompt_builder.build_context_message("", None)
    }

    fn permission_rules(&self) -> Vec<(String, Vec<TargetRule>)> {
        Vec::new()
    }

    fn on_turn_complete(&mut self, content: Option<&str>) {
        if let Some(c) = content {
            self.dream = Some(c.to_string());
        }
    }

    fn skip_permissions(&self) -> bool {
        true
    }

    fn stream_to_tui(&self) -> bool {
        false
    }

    fn is_finished(&self) -> bool {
        self.dream.is_some()
    }

    fn max_iterations(&self) -> Option<usize> {
        Some(SLEEP_MAX_ITERATIONS)
    }
}
