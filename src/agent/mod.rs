// Agent execution flow
//
// run()  ──► Agent::awake → initialize → Ready ──► run_loop ──► shutdown
//
// run_loop (receives UserAction, always ends with save_if_dirty):
//
//   SendMessage ──────────────► push_user_message
//                               iteration = 0, tool_queue.clear()
//                                   │
//   ApprovePending ─────────────┐   │
//   RejectTool ─────────────────┘   │
//         │                         │
//         ▼                         ▼
//   resolve pending            step()
//         │                         │
//         └──────────► step() ◄─────┘
//                          │
//    ┌─────────────────────┤
//    │                     │
//    ▼                     │
//  while tool_queue.pop()  │  iteration >= max_iterations ──► return false
//    │                     │
//    ▼                     │
//  process_one_group()     │
//    │                     │
//    ├─ auto ──► Continue ─┤─► (next queue item)
//    │                     │
//    ├─ need approval ─────┼──► return true  (wait for user)
//    │                     │
//    ▼ queue empty         │
//    │                     │
//    ▼                     │
//  call LLM ───────────────┘
//    │            ▲
//    │            │ (fill tool_queue, loop back)
//    │
//    ├─ error ──────────────► return false
//    └─ no tool_calls ──────► return false
//
//   Interrupt  ──► save_if_dirty, clear all state
//   Quit ──► break loop

pub mod events;
pub mod execution;
pub mod group;
pub mod history;
pub mod modes;
pub mod run;
mod tooling;
mod turn;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::events::AppEvent;
use crate::agent::group::PendingApproval;
use crate::agent::group::ToolCallGroup;
use crate::agent::history::History;
use crate::agent::modes::AwakeMode;
use crate::agent::modes::RunMode;
use crate::config::Config;
use crate::llm::LlmClient;
use crate::mcp::McpClientManager;
use crate::memory::MemoryManager;
use crate::permission::PermissionEvaluator;
use crate::skills::SkillRegistry;

pub(crate) struct Agent {
    config: Config,
    llm: LlmClient,
    memory: Arc<MemoryManager>,
    skills: Arc<SkillRegistry>,
    run_state: Box<dyn RunMode>,
    mcp: McpClientManager,
    history: History,
    pending_approval: Option<PendingApproval>,
    tool_queue: VecDeque<ToolCallGroup>,
    iteration: usize,
    dirty: bool,
    permissions: PermissionEvaluator,
    approved_path: PathBuf,
}

impl Agent {
    pub(super) fn new(
        config: Config,
        run_state: Box<dyn RunMode>,
        memory: Arc<MemoryManager>,
        skills: Arc<SkillRegistry>,
        history: History,
    ) -> Self {
        let llm = LlmClient::new(&config.llm);

        let builtin = run_state.permission_rules();
        let approved_path = PathBuf::from(&config.permission.approved_path);
        let permissions = PermissionEvaluator::new(
            config.permission.default,
            builtin,
            config.permission.tools.clone(),
        );

        Self {
            config,
            llm,
            memory,
            skills,
            run_state,
            mcp: McpClientManager::new(),
            history,
            pending_approval: None,
            tool_queue: VecDeque::new(),
            iteration: 0,
            dirty: false,
            permissions,
            approved_path,
        }
    }

    pub fn awake(config: Config) -> Self {
        let memory = Arc::new(MemoryManager::new(&config.memory));
        let skills = {
            let mut reg = SkillRegistry::new(&config.skills.directory);
            reg.discover();
            Arc::new(reg)
        };

        let awake = AwakeMode::new(&config, memory.clone(), skills.clone());

        let conversation_path = PathBuf::from(&config.state.conversation_path);
        let history = if conversation_path.exists() {
            History::load(conversation_path.clone()).unwrap_or_else(|e| {
                tracing::warn!("Failed to load history: {}, starting fresh", e);
                History::new(conversation_path)
            })
        } else {
            History::new(conversation_path)
        };

        let mut agent = Agent::new(config, Box::new(awake), memory, skills, history);
        agent.permissions.load_approved(&agent.approved_path);
        agent
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        if !self.llm.is_configured() {
            anyhow::bail!(
                "API key not configured. Set api_key in config, or set DEEPSEEK_API_KEY / OPENAI_API_KEY environment variable."
            );
        }
        self.mcp = McpClientManager::connect_all(&self.config.mcp).await?;
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        self.save_state();
        self.mcp.disconnect_all().await;
    }

    pub(super) fn save_state(&self) {
        if let Some(awake) = self.run_state.as_any().downcast_ref::<AwakeMode>() {
            awake.save_state(&self.config);
        }
    }

    fn build_system_message(&self) -> crate::llm::types::ChatMessage {
        self.run_state
            .build_system_message(&self.memory, &self.skills)
    }

    fn build_context_message(&self, wake_dream: Option<&str>) -> crate::llm::types::ChatMessage {
        let fatigue_desc = self.fatigue_desc();
        let dream = wake_dream.or_else(|| {
            self.run_state
                .as_any()
                .downcast_ref::<AwakeMode>()
                .and_then(|a| a.state().dream.as_deref())
        });
        self.run_state.build_context_message(&fatigue_desc, dream)
    }

    fn all_tool_specs(&self) -> Vec<crate::llm::types::ToolSpec> {
        let mut specs = self.run_state.tool_specs();
        specs.extend_from_slice(self.mcp.tool_specs());
        specs
    }

    pub fn push_user_message(&mut self, content: &str) {
        // Ensure system prompt is the first message in history
        if !self
            .history
            .messages()
            .iter()
            .any(|m| m.role == "system" && m.name.is_none())
        {
            let system_msg = self.build_system_message();
            self.history.push(system_msg);
        }

        let context_msg = self.build_context_message(None);
        self.history.push(context_msg);

        self.history.push(crate::llm::types::ChatMessage {
            role: "user".to_string(),
            content: Some(content.to_string()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: Some("user".to_string()),
        });

        self.dirty = true;
    }

    pub async fn save_if_dirty(&mut self) {
        if self.dirty {
            self.save_state();
            self.dirty = false;
        }
    }

    pub fn approve_permission(&mut self, tool: &str, target: String, persist: bool) {
        if persist {
            if let Err(e) = self
                .permissions
                .approve_always(tool, target, &self.approved_path)
            {
                tracing::warn!("Failed to persist permission rule: {}", e);
            }
        } else {
            self.permissions.approve_session(tool, target);
        }
    }

    pub fn is_finished(&self) -> bool {
        self.run_state.is_finished()
    }

    pub fn should_sleep(&self) -> bool {
        self.run_state
            .as_any()
            .downcast_ref::<AwakeMode>()
            .map(|a| a.should_sleep())
            .unwrap_or(false)
    }

    fn fatigue_desc(&self) -> String {
        self.run_state
            .as_any()
            .downcast_ref::<AwakeMode>()
            .map(|a| a.fatigue_desc().to_string())
            .unwrap_or_default()
    }

    fn send_fatigue_update(&self, event_tx: &tokio::sync::mpsc::Sender<AppEvent>) {
        if let Some(awake) = self.run_state.as_any().downcast_ref::<AwakeMode>() {
            let (turns, tool_calls, fatigue) = awake.fatigue_info();
            let desc = awake.fatigue_desc().to_string();
            let _ = event_tx.try_send(AppEvent::FatigueUpdate {
                fatigue,
                turns,
                tool_calls,
                desc,
            });
        }
    }

    fn max_iterations(&self) -> usize {
        self.run_state
            .max_iterations()
            .unwrap_or(self.config.llm.max_iterations as usize)
    }

    fn config(&self) -> &Config {
        &self.config
    }

    fn memory(&self) -> &Arc<MemoryManager> {
        &self.memory
    }

    fn skills(&self) -> &Arc<SkillRegistry> {
        &self.skills
    }

    pub fn reset_iteration(&mut self) {
        self.iteration = 0;
        self.tool_queue.clear();
    }

    pub fn history_messages(&self) -> &[crate::llm::types::ChatMessage] {
        self.history.messages()
    }

    pub(crate) fn run_state_as_awake(&self) -> &AwakeMode {
        self.run_state
            .as_any()
            .downcast_ref::<AwakeMode>()
            .expect("expected AwakeMode")
    }

    pub(crate) fn run_state_as_awake_mut(&mut self) -> &mut AwakeMode {
        self.run_state
            .as_any_mut()
            .downcast_mut::<AwakeMode>()
            .expect("expected AwakeMode")
    }

    pub(crate) fn run_state_as_sleeping_mut(&mut self) -> &mut crate::agent::modes::SleepingMode {
        self.run_state
            .as_any_mut()
            .downcast_mut::<crate::agent::modes::SleepingMode>()
            .expect("expected SleepingMode")
    }
}

pub use run::run;
