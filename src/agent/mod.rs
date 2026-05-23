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
//
// Mode transitions are type-state: Agent<AwakeMode> → Agent<SleepingMode> → Agent<AwakeMode>.
// The generic Agent<S: RunMode> holds all shared resources; S holds mode-specific state.
// Transition methods consume self and return the new type, enforced at compile time.

pub mod events;
pub mod execution;
pub mod group;
pub mod history;
pub mod modes;
mod recall;
pub mod run;
mod tooling;
mod turn;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::group::PendingApproval;
use crate::agent::group::ToolCallGroup;
use crate::agent::history::History;
use crate::agent::modes::AwakeMode;
use crate::agent::modes::RunMode;
use crate::agent::modes::SleepingMode;
use crate::config::Config;
use crate::llm::LlmClient;
use crate::mcp::McpClientManager;
use crate::memory::MemoryManager;
use crate::permission::PermissionEvaluator;
use crate::skills::SkillRegistry;

pub(crate) struct Agent<S: RunMode> {
    config: Config,
    llm: LlmClient,
    memory: Arc<MemoryManager>,
    skills: Arc<SkillRegistry>,
    run_state: S,
    mcp: McpClientManager,
    history: History,
    pending_approval: Option<PendingApproval>,
    tool_queue: VecDeque<ToolCallGroup>,
    iteration: usize,
    dirty: bool,
    recall_text: String,
    recall_top_score: f32,
    permissions: PermissionEvaluator,
    approved_path: PathBuf,
}

// ── Generic methods for all modes ─────────────────────────────────

impl<S: RunMode> Agent<S> {
    fn build_system_message(&self) -> crate::llm::types::ChatMessage {
        self.run_state
            .build_system_message(&self.memory, &self.skills)
    }

    fn all_tool_specs(&self) -> Vec<crate::llm::types::ToolSpec> {
        let mut specs = self.run_state.tool_specs();
        specs.extend_from_slice(self.mcp.tool_specs());
        specs
    }

    pub fn reset_iteration(&mut self) {
        self.iteration = 0;
        self.tool_queue.clear();
    }

    pub fn is_finished(&self) -> bool {
        self.run_state.is_finished()
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

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        if !self.llm.is_configured() {
            anyhow::bail!(
                "API key not configured. Set api_key in config, or set DEEPSEEK_API_KEY / OPENAI_API_KEY environment variable."
            );
        }
        self.mcp = McpClientManager::connect_all(&self.config.mcp).await?;
        Ok(())
    }

    fn max_iterations(&self) -> usize {
        self.run_state
            .max_iterations()
            .unwrap_or(self.config.llm.max_iterations as usize)
    }

    /// Reconstruct Agent with a new run mode. Consumes self, replaces
    /// run_state/history/permissions, and resets all per-turn state.
    fn into_mode<N: RunMode>(
        self,
        new_state: N,
        new_history: History,
    ) -> Agent<N> {
        let mut permissions = self.permissions;
        permissions.set_mode_rules(new_state.permission_rules());
        Agent {
            config: self.config,
            llm: self.llm,
            memory: self.memory,
            skills: self.skills,
            run_state: new_state,
            mcp: self.mcp,
            history: new_history,
            pending_approval: None,
            tool_queue: VecDeque::new(),
            iteration: 0,
            dirty: false,
            recall_text: String::new(),
            recall_top_score: 0.0,
            permissions,
            approved_path: self.approved_path,
        }
    }
}

// ── Awake-specific methods ────────────────────────────────────────

impl Agent<AwakeMode> {
    /// Create Agent for normal startup (cold boot or session resume).
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

        let llm = LlmClient::new(&config.llm);

        let builtin = awake.permission_rules();
        let approved_path = PathBuf::from(&config.permission.approved_path);
        let mut permissions = PermissionEvaluator::new(
            config.permission.default,
            builtin,
            config.permission.tools.clone(),
        );
        permissions.load_approved(&approved_path);

        let mut agent = Self {
            config,
            llm,
            memory,
            skills,
            run_state: awake,
            mcp: McpClientManager::new(),
            history,
            pending_approval: None,
            tool_queue: VecDeque::new(),
            iteration: 0,
            dirty: false,
            recall_text: String::new(),
            recall_top_score: 0.0,
            permissions,
            approved_path,
        };

        if agent.history.messages().is_empty() {
            let system_msg = agent
                .run_state
                .build_system_message(&agent.memory, &agent.skills);
            agent.history.push(system_msg);
        }

        agent
    }

    pub async fn shutdown(&mut self) {
        self.save_state();
        self.mcp.disconnect_all().await;
    }

    pub fn should_sleep(&self) -> bool {
        self.run_state.should_sleep()
    }

    pub fn push_user_message(&mut self, content: &str) {
        let full_content = self.build_user_turn_content(content);

        self.history.push(crate::llm::types::ChatMessage {
            role: "user".to_string(),
            content: Some(full_content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: Some("user".to_string()),
            usage: None,
        });

        self.dirty = true;
    }

    fn build_user_turn_content(&self, user_input: &str) -> String {
        let now = chrono::Local::now();
        let mut parts = Vec::new();

        parts.push(format!("当前时间：{}", now.format("%Y-%m-%d %H:%M")));

        let fatigue = self.run_state.fatigue_desc();
        if !fatigue.is_empty() {
            parts.push(format!("你的状态：{}", fatigue));
        }

        let (turns, tool_calls, _) = self.run_state.fatigue_info();
        if turns > 0 {
            parts.push(format!(
                "本轮已对话 {} 轮，使用了 {} 次工具",
                turns, tool_calls
            ));
        }

        if self.run_state.state().dream.is_some() {
            parts.push("你刚从梦中醒来，梦中的感受还隐约残留。".to_string());
        }

        if !self.recall_text.is_empty()
            && self.recall_top_score >= self.config.memory.recall_inject_threshold
        {
            let roll = rand::random::<f32>();
            if roll < self.config.memory.recall_inject_probability {
                parts.push(format!("唤起的记忆：\n{}", self.recall_text));
            }
        }

        format!(
            "[以下为系统上下文，不是用户的输入]\n{}\n\n[以下是用户的输入]\n{}",
            parts.join("\n"),
            user_input
        )
    }

    pub async fn save_if_dirty(&mut self) {
        if self.dirty {
            self.save_state();
            self.dirty = false;
        }
    }

    fn save_state(&self) {
        self.run_state.save_state(&self.config);
    }

    /// Transition into sleeping mode. Consumes self, returns Agent<SleepingMode>.
    /// Archives conversation, creates sleep history, resets per-turn state.
    pub fn transition_to_sleeping(self, snapshot_ts: String) -> Agent<SleepingMode> {
        self.save_state();

        let data_dir = self.config.state.data_dir();
        let history_folder = data_dir.join("history").join(&snapshot_ts);

        // Archive conversation before swapping history
        if let Err(e) = std::fs::create_dir_all(&history_folder) {
            tracing::error!("Failed to create history folder: {}", e);
        }
        if !self.history.messages().is_empty() {
            let archive_path = history_folder.join("conversation.jsonl");
            let now_ts = chrono::Local::now().to_rfc3339();
            if let Ok(mut file) = std::fs::File::create(&archive_path) {
                use std::io::Write;
                for (i, msg) in self.history.messages().iter().enumerate() {
                    let tm = crate::agent::history::TimedMessage {
                        ts: format!("{} (msg {})", now_ts, i),
                        message: msg.clone(),
                    };
                    let _ = writeln!(file, "{}", serde_json::to_string(&tm).unwrap_or_default());
                }
            }
        }

        // Build sleeping mode
        let (conversation_text, turns, tool_calls, fatigue_desc, fatigue_val) = {
            let awake = &self.run_state;
            let (t, tc, f) = awake.fatigue_info();
            let text = crate::sleep::build_conversation_text(self.history.messages());
            (text, t, tc, awake.fatigue_desc().to_string(), f)
        };

        let sleeping = SleepingMode::new(
            self.memory.clone(),
            conversation_text,
            turns,
            tool_calls,
            fatigue_desc,
            fatigue_val,
        );

        let sleep_history_path = history_folder.join("sleep.jsonl");
        let mut new_history = History::new(sleep_history_path);

        let system_msg = sleeping.build_system_message(&self.memory, &self.skills);
        new_history.push(system_msg);

        self.into_mode(sleeping, new_history)
    }
}

// ── Sleeping-specific methods ─────────────────────────────────────

impl Agent<SleepingMode> {
    pub async fn run_sleep_cycle(
        mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::agent::events::AppEvent>,
        snapshot_ts: String,
    ) -> Agent<AwakeMode> {
        use crate::agent::events::AppEvent;
        use crate::embedding::EmbeddingConfig;
        use crate::llm::LlmClient;
        use crate::sleep::consolidation;

        let data_dir = self.config.state.data_dir();
        let history_folder = data_dir.join("history").join(&snapshot_ts);

        let _ = event_tx.send(AppEvent::Sleeping).await;

        loop {
            if event_tx.is_closed() {
                break;
            }
            let continued = self.step(event_tx).await;
            if !continued && !self.is_finished() {
                // step() returned false without finishing (max iterations or LLM error).
                // Break to prevent busy-looping.
                tracing::warn!("Sleep step ended without finishing; breaking loop");
                break;
            }
        }

        let embed_config = EmbeddingConfig::from_config(&self.config.embedding);
        let transitioned =
            consolidation::phase_transition_and_embed(&self.memory, &embed_config).await;

        let llm = LlmClient::new(&self.config.llm);
        consolidation::phase_consolidate(&self.memory, &llm, &transitioned).await;

        consolidation::phase_compress(&self.memory, &llm).await;

        let dream = self.run_state.take_dream();
        let (turns, tool_calls, fatigue_val) = self.run_state.pre_sleep_stats();
        let dream_for_event = dream.clone();
        let sleep_record = serde_json::json!({
            "timestamp": snapshot_ts,
            "turns": turns,
            "tool_calls": tool_calls,
            "fatigue": fatigue_val,
            "dream": dream,
        });
        let sleep_path = history_folder.join("sleep.json");
        if let Err(e) = std::fs::write(
            &sleep_path,
            serde_json::to_string_pretty(&sleep_record).unwrap_or_default(),
        ) {
            tracing::error!("Failed to write sleep record: {}", e);
        }

        if let Err(e) =
            crate::data_git::commit_all(&data_dir, &format!("sleep done: {}", snapshot_ts))
        {
            tracing::error!("Failed to commit sleep data: {}", e);
        }

        let mut awake = self.transition_to_awake(dream);

        if let Err(e) = awake.initialize().await {
            tracing::error!("Failed to initialize agent after sleep: {}", e);
            let _ = event_tx
                .send(crate::agent::events::AppEvent::Error {
                    message: format!("Post-sleep initialization failed: {}", e),
                })
                .await;
        }

        if let Some(d) = dream_for_event {
            let _ = event_tx.send(AppEvent::WakingUp { dream: d }).await;
        }
        if let Some(evt) = awake.run_state.fatigue_update_event() {
            let _ = event_tx.try_send(evt);
        }

        awake
    }

    fn transition_to_awake(self, dream: Option<String>) -> Agent<AwakeMode> {
        {
            let state_path = std::path::PathBuf::from(&self.config.state.path);
            if state_path.exists()
                && let Err(e) = std::fs::remove_file(&state_path)
            {
                tracing::error!("Failed to remove state file: {}", e);
            }

            let conversation_path = std::path::PathBuf::from(&self.config.state.conversation_path);
            if conversation_path.exists()
                && let Err(e) = std::fs::write(&conversation_path, "")
            {
                tracing::error!("Failed to clear conversation history: {}", e);
            }
        }


        let awake = AwakeMode::build(
            &self.config,
            self.memory.clone(),
            self.skills.clone(),
            dream,
        );

        let conversation_path = PathBuf::from(&self.config.state.conversation_path);
        let mut new_history = History::new(conversation_path);

        let system_msg = awake.build_system_message(&self.memory, &self.skills);
        new_history.push(system_msg);

        let new_agent = self.into_mode(awake, new_history);

        new_agent.save_state();

        new_agent
    }
}

pub use run::run;
