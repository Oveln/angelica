use crate::llm::types::{ChatMessage, ToolSpec};
use crate::memory::MemoryManager;
use crate::permission::TargetRule;
use crate::skills::SkillRegistry;
use crate::tools::Tool;

pub mod awake;
pub mod sleeping;

pub use awake::AwakeMode;
pub use sleeping::SleepingMode;

pub trait RunMode: Send + Sync + 'static {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    fn tool_specs(&self) -> Vec<ToolSpec>;
    fn get_tool(&self, name: &str) -> Option<&dyn Tool>;

    fn build_system_message(&self, memory: &MemoryManager, skills: &SkillRegistry) -> ChatMessage;
    fn build_context_message(&self, fatigue_desc: &str, wake_dream: Option<&str>) -> ChatMessage;

    fn permission_rules(&self) -> Vec<(String, Vec<TargetRule>)>;

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
}
