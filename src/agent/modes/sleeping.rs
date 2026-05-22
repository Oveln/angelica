use std::sync::{Arc, Mutex};

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
    dream: Arc<Mutex<Option<String>>>,
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

        let dream: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let mut reg = ToolRegistry::new();
        reg.register(Box::new(tools::EditSoulTool::new(memory.clone())));
        reg.register(Box::new(tools::EditMemoryTool::new(memory.clone())));
        reg.register(Box::new(tools::EditProfileTool::new(memory)));
        reg.register(Box::new(tools::DreamTool::new(dream.clone())));

        Self {
            tools: reg,
            prompt_builder,
            dream,
        }
    }

    pub fn take_dream(&mut self) -> Option<String> {
        self.dream.lock().expect("dream lock poisoned").take()
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

    fn permission_rules(&self) -> Vec<(String, Vec<TargetRule>)> {
        Vec::new()
    }

    fn on_turn_complete(&mut self, _content: Option<&str>) {
        // Dream is now captured via the dreaming tool
    }

    fn skip_permissions(&self) -> bool {
        true
    }

    fn stream_to_tui(&self) -> bool {
        false
    }

    fn is_finished(&self) -> bool {
        self.dream.lock().expect("dream lock poisoned").is_some()
    }

    fn max_iterations(&self) -> Option<usize> {
        Some(SLEEP_MAX_ITERATIONS)
    }
}
