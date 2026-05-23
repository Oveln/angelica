use std::sync::Arc;

use crate::agent::events::AppEvent;
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
use crate::usage::UsageScope;

pub struct AwakeMode {
    tools: ToolRegistry,
    prompt_builder: AwakePromptBuilder,
    state: AgentState,
}


impl AwakeMode {
    /// Create AwakeMode for normal startup. Loads persisted state if available,
    /// otherwise starts fresh with `on_wake()`.
    pub fn new(config: &Config, memory: Arc<MemoryManager>, skills: Arc<SkillRegistry>) -> Self {
        Self::build(config, memory, skills, None)
    }


    pub(crate) fn build(
        config: &Config,
        memory: Arc<MemoryManager>,
        skills: Arc<SkillRegistry>,
        wake_dream: Option<String>,
    ) -> Self {
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

        let state = match wake_dream {
            Some(dream) => {
                let mut s = AgentState::new(&config.fatigue);
                s.fatigue.on_wake();
                s.dream = Some(dream);
                s
            }
            None => {
                let state_path = std::path::PathBuf::from(&config.state.path);
                if state_path.exists() {
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
                }
            }
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

    fn builtin_rules(&self) -> Vec<(String, Vec<TargetRule>)> {
        self.tools.builtin_rules()
    }
}

impl RunMode for AwakeMode {
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

    fn on_context_update(&mut self, prompt_tokens: u64) {
        self.state.fatigue.update_from_context(prompt_tokens);
    }

    fn on_turn_complete(&mut self, _content: Option<&str>) {
        self.state.fatigue.on_turn();
        if !self.state.fatigue.is_groggy() && self.state.dream.is_some() {
            self.state.dream = None;
        }
    }

    fn on_tool_calls(&mut self, count: usize) {
        self.state.fatigue.add_tool_calls(count as u32);
    }

    fn fatigue_update_event(&self) -> Option<AppEvent> {
        let (turns, tool_calls, fatigue) = self.fatigue_info();
        Some(AppEvent::FatigueUpdate {
            fatigue,
            turns,
            tool_calls,
            desc: self.fatigue_desc().to_string(),
        })
    }

    fn usage_scope(&self) -> UsageScope {
        UsageScope::Awake
    }

    fn should_recall(&self) -> bool {
        true
    }
}
