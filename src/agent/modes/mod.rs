use crate::agent::events::AppEvent;
use crate::llm::types::{ChatMessage, ToolSpec};
use crate::memory::MemoryManager;
use crate::permission::TargetRule;
use crate::skills::SkillRegistry;
use crate::tools::Tool;
use crate::usage::UsageScope;

pub mod awake;
pub mod sleeping;

pub use awake::AwakeMode;
pub use sleeping::SleepingMode;

pub trait RunMode: Send + Sync + 'static {
    fn tool_specs(&self) -> Vec<ToolSpec>;
    fn get_tool(&self, name: &str) -> Option<&dyn Tool>;

    fn build_system_message(&self, memory: &MemoryManager, skills: &SkillRegistry) -> ChatMessage;

    fn permission_rules(&self) -> Vec<(String, Vec<TargetRule>)>;

    fn on_context_update(&mut self, _prompt_tokens: u64) {}
    fn on_turn_complete(&mut self, _content: Option<&str>) {}
    fn on_tool_calls(&mut self, _count: usize) {}

    fn include_history(&self) -> bool {
        true
    }

    fn accumulate_history(&self) -> bool {
        true
    }

    fn skip_permissions(&self) -> bool {
        false
    }

    fn stream_to_tui(&self) -> bool {
        true
    }

    fn is_finished(&self) -> bool {
        false
    }

    fn max_iterations(&self) -> Option<usize> {
        None
    }

    /// Return fatigue state as a TUI event, if applicable.
    /// AwakeMode sends FatigueUpdate; SleepingMode returns None (default).
    fn fatigue_update_event(&self) -> Option<AppEvent> {
        None
    }

    /// Which usage scope this mode records metrics under.
    fn usage_scope(&self) -> UsageScope;

    /// Whether to run embedding-based recall after a text turn.
    fn should_recall(&self) -> bool {
        false
    }
}
