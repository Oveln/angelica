use std::sync::Arc;

use crate::agent::modes::RunMode;
use crate::config::Config;
use crate::llm::types::{ChatMessage, ToolSpec};
use crate::memory::MemoryManager;
use crate::permission::TargetRule;
use crate::prompt::{AwakePromptBuilder, PromptBuilder};
use crate::skills::SkillRegistry;
use crate::state::AgentState;
use crate::tools::Tool;
use crate::tools::ToolRegistry;

pub struct AwakeMode {
    tools: ToolRegistry,
    prompt_builder: AwakePromptBuilder,
    state: AgentState,
}

impl AwakeMode {
    pub fn new(config: &Config, memory: Arc<MemoryManager>, skills: Arc<SkillRegistry>) -> Self {
        let model_patch =
            crate::llm::patch::ModelPatch::new(&config.llm.model, config.llm.role_immersion);
        let prompt_builder = AwakePromptBuilder::new(model_patch);

        let history_dir = std::path::PathBuf::from(&config.state.conversation_path)
            .parent()
            .map(|p| p.join("history").to_string_lossy().to_string())
            .unwrap_or_else(|| "data/history".to_string());

        let tools = ToolRegistry::with_awake_defaults(
            memory,
            skills,
            &history_dir,
            &config.state.conversation_path,
        );

        let state_path = std::path::PathBuf::from(&config.state.path);
        let state = if state_path.exists() {
            AgentState::load(&state_path, &config.fatigue).unwrap_or_else(|e| {
                tracing::warn!("Failed to load state: {}, creating new", e);
                let mut s = AgentState::new(&config.fatigue);
                s.fatigue.on_wake();
                s
            })
        } else {
            let mut s = AgentState::new(&config.fatigue);
            s.fatigue.on_wake();
            s
        };

        Self {
            tools,
            prompt_builder,
            state,
        }
    }

    pub fn state(&self) -> &AgentState {
        &self.state
    }

    pub fn should_sleep(&self) -> bool {
        self.state.fatigue.should_sleep()
    }

    pub fn save_state(&self, config: &Config) {
        let state_path = std::path::PathBuf::from(&config.state.path);
        if let Err(e) = self.state.save(&state_path) {
            tracing::warn!("Failed to save state: {}", e);
        }
    }

    pub fn take_dream(&mut self) -> Option<String> {
        self.state.dream.take()
    }

    pub fn set_dream(&mut self, dream: String) {
        self.state.dream = Some(dream);
    }

    pub fn set_last_snapshot(&mut self, ts: String) {
        self.state.last_snapshot = Some(ts);
    }

    pub fn fatigue_desc(&self) -> &str {
        self.state.fatigue.describe()
    }

    pub fn fatigue_info(&self) -> (u32, u32, f64) {
        (
            self.state.fatigue.turns(),
            self.state.fatigue.tool_calls(),
            self.state.fatigue.fatigue(),
        )
    }

    pub fn new_with_fresh_state(
        &self,
        config: &Config,
        memory: Arc<MemoryManager>,
        skills: Arc<SkillRegistry>,
    ) -> Self {
        let mut mode = Self::new(config, memory, skills);
        mode.state = AgentState::new(&config.fatigue);
        mode.state.fatigue.on_wake();
        mode
    }

    fn builtin_rules(&self) -> Vec<(String, Vec<TargetRule>)> {
        self.tools.builtin_rules()
    }
}

impl RunMode for AwakeMode {
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
        self.builtin_rules()
    }

    fn on_turn_complete(&mut self, _content: Option<&str>) {
        self.state.fatigue.on_turn();
        if !self.state.fatigue.is_groggy() && self.state.dream.is_some() {
            self.state.dream = None;
        }
    }

    fn on_tool_calls(&mut self, count: usize) {
        for _ in 0..count {
            self.state.fatigue.on_tool_call();
        }
    }
}
