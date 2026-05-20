// Agent execution flow
//
// run()  ──► Agent::new → initialize → Ready ──► run_loop ──► shutdown
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
//   ClearHistory ──► clear_history
//   Quit ──► break loop

pub mod events;
pub mod execution;
pub mod group;
pub mod history;
pub mod run;

use std::collections::VecDeque;
use std::sync::Arc;

use crate::agent::group::PendingApproval;
use crate::agent::group::ToolCallGroup;
use crate::agent::history::History;
use crate::config::Config;
use crate::llm::LlmClient;
use crate::mcp::McpClientManager;
use crate::memory::MemoryManager;
use crate::session::SessionManager;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

pub(crate) const SYSTEM_PROMPT_BASE: &str =
    "You are Angelica (祈芷), a helpful AI assistant. You can use tools to help the user.

You can return multiple tool calls in a single response to perform independent operations in parallel.";

pub(crate) struct Agent {
    config: Config,
    llm: LlmClient,
    memory: Arc<MemoryManager>,
    sessions: Arc<SessionManager>,
    skills: SkillRegistry,
    tools: ToolRegistry,
    mcp: McpClientManager,
    history: History,
    pending_approval: Option<PendingApproval>,
    tool_queue: VecDeque<ToolCallGroup>,
    iteration: usize,
    dirty: bool,
}

impl Agent {
    pub fn new(config: Config) -> Self {
        let llm = LlmClient::new(&config.llm);
        let memory = Arc::new(MemoryManager::new(&config.memory));
        let sessions = Arc::new(SessionManager::new(&config.session));
        let mut skills = SkillRegistry::new(&config.skills.directory);
        skills.discover();

        let tools = ToolRegistry::with_defaults(memory.clone(), sessions.clone());

        Self {
            config,
            llm,
            memory,
            sessions,
            skills,
            tools,
            mcp: McpClientManager::new(),
            history: History::new(),
            pending_approval: None,
            tool_queue: VecDeque::new(),
            iteration: 0,
            dirty: false,
        }
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
        if self.dirty {
            let _ = self.save_session().await;
        }
        self.mcp.disconnect_all().await;
    }

    pub async fn save_session(&self) -> anyhow::Result<()> {
        self.sessions.save_session(self.history.messages())?;
        Ok(())
    }

    pub async fn build_system_message(&self) -> crate::llm::types::ChatMessage {
        use crate::llm::types::ChatMessage;

        let mut content = String::new();

        let soul = self.memory.read_soul();
        if !soul.trim().is_empty() {
            content.push_str(&soul);
            content.push_str("\n\n");
        }

        content.push_str(SYSTEM_PROMPT_BASE);

        for spec in self.all_tool_specs() {
            if let Some(desc) = spec.function.description {
                content.push_str(&format!("\n- **{}**: {}", spec.function.name, desc));
            }
        }

        for skill in self.skills.get_all_skills() {
            content.push_str(&format!(
                "\n\n## Skill: {}\n{}",
                skill.name, skill.instructions
            ));
        }

        let agent_mem = self.memory.read_agent_memory();
        if !agent_mem.trim().is_empty() {
            content.push_str(&format!("\n\n## Your Memory\n{}", agent_mem));
        }

        let user_profile = self.memory.read_user_profile();
        if !user_profile.trim().is_empty() {
            content.push_str(&format!("\n\n## User Profile\n{}", user_profile));
        }

        ChatMessage {
            role: "system".to_string(),
            content: Some(content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    fn all_tool_specs(&self) -> Vec<crate::llm::types::ToolSpec> {
        let mut specs = self.tools.all_specs();
        specs.extend_from_slice(self.mcp.tool_specs());
        specs
    }

    pub fn push_user_message(&mut self, content: &str) {
        self.history.push(crate::llm::types::ChatMessage {
            role: "user".to_string(),
            content: Some(content.to_string()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        self.dirty = true;
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.dirty = false;
    }

    pub async fn save_if_dirty(&mut self) {
        if self.dirty {
            let _ = self.save_session().await;
            self.dirty = false;
        }
    }
}

pub use run::run;
