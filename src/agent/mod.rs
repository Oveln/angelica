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
//   ApprovePendingWithResult ───┤   │
//   RejectCommand ──────────────┘   │
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
pub mod group;
pub mod history;

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::agent::events::{AppEvent, UserAction};
use crate::agent::group::{
    BatchedEdit, PendingApproval, ToolCallGroup, group_tool_calls, needs_tty,
};
use crate::agent::history::History;
use crate::config::Config;
use crate::llm::types::ChatMessage;
use crate::llm::{AppStreamEvent, StreamFinal};
use crate::mcp::McpClientManager;
use crate::memory::MemoryManager;
use crate::session::SessionManager;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

const SYSTEM_PROMPT_BASE: &str =
    "You are Angelica (祈芷), a helpful AI assistant. You can use tools to help the user.

You can return multiple tool calls in a single response to perform independent operations in parallel.";

enum ProcessOutcome {
    Continue,
    NeedApproval,
}

pub struct Agent {
    config: Config,
    llm: crate::llm::LlmClient,
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
        let llm = crate::llm::LlmClient::new(&config.llm);
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

    pub async fn build_system_message(&self) -> ChatMessage {
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

    async fn execute_tool(&self, name: &str, args: serde_json::Value) -> String {
        if let Some(tool) = self.tools.get(name) {
            match tool.execute(args).await {
                Ok(result) => result,
                Err(e) => format!("Error: {}", e),
            }
        } else {
            match self.mcp.call_tool(name, args).await {
                Ok(result) => result,
                Err(e) => format!("Error: {}", e),
            }
        }
    }

    async fn process_one_group(
        &mut self,
        group: ToolCallGroup,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> ProcessOutcome {
        match group {
            ToolCallGroup::Single { tc } => {
                let name = tc.function.name.clone();
                let args: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

                let _ = event_tx
                    .send(AppEvent::ToolCalling {
                        name: name.clone(),
                        arguments: tc.function.arguments.clone(),
                    })
                    .await;

                if self.tools.is_auto_execute(&name) {
                    let result = self.execute_tool(&name, args).await;
                    let _ = event_tx
                        .send(AppEvent::ToolResult {
                            name: name.clone(),
                            result: result.clone(),
                        })
                        .await;
                    self.history.record_tool_result(tc.id, result);
                    ProcessOutcome::Continue
                } else {
                    let (preview, is_tty, command) = self.make_approval_preview(&name, &args, None);

                    self.history
                        .record_tool_result(tc.id.clone(), "Pending user approval...".to_string());
                    self.pending_approval = Some(PendingApproval {
                        tc_ids: vec![tc.id],
                        tool_name: name,
                        args,
                        batched_edits: None,
                    });

                    let _ = event_tx
                        .send(AppEvent::ApprovalPending {
                            preview,
                            is_tty_command: is_tty,
                            command,
                        })
                        .await;

                    ProcessOutcome::NeedApproval
                }
            }
            ToolCallGroup::BatchedEdits { path, edits } => {
                let count = edits.len();
                let display_args = serde_json::to_string(&serde_json::json!({
                    "path": path,
                    "count": count,
                }))
                .unwrap_or_default();

                let _ = event_tx
                    .send(AppEvent::ToolCalling {
                        name: "edit_file".to_string(),
                        arguments: display_args,
                    })
                    .await;

                let searches_replaces: Vec<(String, String)> = edits
                    .iter()
                    .map(|e| (e.search.clone(), e.replace.clone()))
                    .collect();

                let preview =
                    match crate::tools::edit_file::preview_batched(&path, &searches_replaces) {
                        Ok(p) => p,
                        Err(e) => format!("Preview failed: {}", e),
                    };

                for e in &edits {
                    self.history.record_tool_result(
                        e.tc_id.clone(),
                        "Pending user approval...".to_string(),
                    );
                }

                let batched: Vec<BatchedEdit> = edits
                    .iter()
                    .map(|e| BatchedEdit {
                        tc_id: e.tc_id.clone(),
                        search: e.search.clone(),
                        replace: e.replace.clone(),
                    })
                    .collect();

                self.pending_approval = Some(PendingApproval {
                    tc_ids: batched.iter().map(|b| b.tc_id.clone()).collect(),
                    tool_name: "edit_file".to_string(),
                    args: serde_json::json!({"path": path}),
                    batched_edits: Some(batched),
                });

                let _ = event_tx
                    .send(AppEvent::ApprovalPending {
                        preview,
                        is_tty_command: false,
                        command: None,
                    })
                    .await;

                ProcessOutcome::NeedApproval
            }
        }
    }

    pub async fn step(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        let max_iterations = self.config.llm.max_iterations as usize;

        loop {
            while let Some(group) = self.tool_queue.pop_front() {
                match self.process_one_group(group, event_tx).await {
                    ProcessOutcome::Continue => continue,
                    ProcessOutcome::NeedApproval => return true,
                }
            }

            if self.iteration >= max_iterations {
                let _ = event_tx
                    .send(AppEvent::TextDone {
                        full_text: "[Reached maximum iterations]".to_string(),
                    })
                    .await;
                let _ = event_tx.send(AppEvent::TurnComplete).await;
                return false;
            }

            let messages = self.history.messages();
            let system_msg = self.build_system_message().await;
            let mut all_messages = vec![system_msg];
            all_messages.extend_from_slice(messages);

            let specs = self.all_tool_specs();

            let (stream_tx, mut stream_rx) = mpsc::channel::<AppStreamEvent>(512);

            let fwd_tx = event_tx.clone();
            let drain_handle = tokio::spawn(async move {
                while let Some(evt) = stream_rx.recv().await {
                    match evt {
                        AppStreamEvent::ThinkingDelta { delta } => {
                            let _ = fwd_tx.send(AppEvent::ThinkingDelta { delta }).await;
                        }
                        AppStreamEvent::TextDelta { delta } => {
                            let _ = fwd_tx.send(AppEvent::TextDelta { delta }).await;
                        }
                        AppStreamEvent::ToolCallStart { .. }
                        | AppStreamEvent::ToolCallArgsDelta { .. } => {}
                        AppStreamEvent::Done => break,
                    }
                }
            });

            let llm_result = self
                .llm
                .stream_complete(&all_messages, &specs, &stream_tx)
                .await;

            let _ = drain_handle.await;

            let result = match llm_result {
                Ok(r) => r,
                Err(e) => {
                    let _ = event_tx
                        .send(AppEvent::Error {
                            message: format!("LLM error: {}", e),
                        })
                        .await;
                    return false;
                }
            };

            self.iteration += 1;

            let StreamFinal {
                reasoning,
                content,
                tool_calls,
            } = result;

            self.history
                .record_assistant(content.clone(), reasoning, tool_calls.clone());
            self.dirty = true;

            let full_text = content.unwrap_or_default();
            let _ = event_tx
                .send(AppEvent::TextDone {
                    full_text: full_text.clone(),
                })
                .await;

            let Some(tcs) = tool_calls else {
                let _ = event_tx.send(AppEvent::TurnComplete).await;
                return false;
            };

            self.tool_queue = group_tool_calls(tcs).into();
        }
    }

    fn make_approval_preview(
        &self,
        name: &str,
        args: &serde_json::Value,
        batched_edits: Option<&[(String, String)]>,
    ) -> (String, bool, Option<String>) {
        if let Some(edits) = batched_edits {
            let path = args["path"].as_str().unwrap_or("?");
            match crate::tools::edit_file::preview_batched(path, edits) {
                Ok(preview) => return (preview, false, None),
                Err(e) => {
                    return (
                        format!("Preview failed: {}\n\nArguments: {}", e, args),
                        false,
                        None,
                    );
                }
            }
        }
        if let Some(tool) = self.tools.get(name) {
            match tool.preview(args.clone()) {
                Ok(Some(preview)) => return (preview, false, None),
                Ok(None) => {}
                Err(e) => {
                    return (
                        format!("Preview failed: {}\n\nArguments: {}", e, args),
                        false,
                        None,
                    );
                }
            }
        }

        if name == "run_command" {
            let cmd = args["command"].as_str().unwrap_or("").to_string();
            let is_tty = needs_tty(&cmd);
            return (format!("$ {}", cmd), is_tty, Some(cmd.clone()));
        }

        (format!("[{}]\nArguments: {}", name, args), false, None)
    }

    pub async fn approve_pending(&mut self, event_tx: &mpsc::Sender<AppEvent>) {
        let pending = self.pending_approval.take().expect("no pending approval");

        let result = if let Some(ref batched) = pending.batched_edits {
            let edits: Vec<(String, String)> = batched
                .iter()
                .map(|e| (e.search.clone(), e.replace.clone()))
                .collect();
            let path = pending.args["path"].as_str().unwrap_or("?");
            match crate::tools::edit_file::execute_batched(path, &edits) {
                Ok(summary) => summary,
                Err(e) => format!("Error: {}", e),
            }
        } else {
            self.execute_tool(&pending.tool_name, pending.args).await
        };

        let _ = event_tx
            .send(AppEvent::CommandResult {
                output: result.clone(),
            })
            .await;

        for tc_id in &pending.tc_ids {
            self.history.update_tool_result(tc_id, result.clone());
        }
    }

    pub async fn approve_pending_with_result(
        &mut self,
        output: String,
        event_tx: &mpsc::Sender<AppEvent>,
    ) {
        let pending = self.pending_approval.take().expect("no pending approval");

        let _ = event_tx
            .send(AppEvent::CommandResult {
                output: output.clone(),
            })
            .await;

        for tc_id in &pending.tc_ids {
            self.history.update_tool_result(tc_id, output.clone());
        }
    }

    pub async fn reject_command(&mut self, feedback: &str, event_tx: &mpsc::Sender<AppEvent>) {
        let pending = self.pending_approval.take().expect("no pending approval");
        let msg = if feedback.is_empty() {
            "User rejected this operation.".to_string()
        } else {
            format!("User rejected this operation. Feedback: {}", feedback)
        };
        let _ = event_tx
            .send(AppEvent::CommandRejected {
                feedback: msg.clone(),
            })
            .await;

        for tc_id in &pending.tc_ids {
            self.history.update_tool_result(tc_id, msg.clone());
        }
    }

    pub async fn approve_and_step(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        self.approve_pending(event_tx).await;
        self.step(event_tx).await
    }

    pub async fn approve_and_step_with_result(
        &mut self,
        output: String,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        self.approve_pending_with_result(output, event_tx).await;
        self.step(event_tx).await
    }

    pub async fn reject_and_step(
        &mut self,
        feedback: &str,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        self.reject_command(feedback, event_tx).await;
        self.step(event_tx).await
    }

    pub fn push_user_message(&mut self, content: &str) {
        self.history.push(ChatMessage {
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

async fn run_loop(
    agent: &mut Agent,
    user_rx: &mut mpsc::Receiver<UserAction>,
    event_tx: &mpsc::Sender<AppEvent>,
) {
    while let Some(action) = user_rx.recv().await {
        match action {
            UserAction::SendMessage { content } => {
                agent.push_user_message(&content);
                agent.iteration = 0;
                agent.tool_queue.clear();
                let _ = agent.step(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::ApprovePending => {
                let _ = agent.approve_and_step(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::ApprovePendingWithResult { output } => {
                let _ = agent.approve_and_step_with_result(output, event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::RejectCommand { feedback } => {
                let _ = agent
                    .reject_and_step(feedback.as_deref().unwrap_or(""), event_tx)
                    .await;
                agent.save_if_dirty().await;
            }
            UserAction::Interrupt => {
                agent.save_if_dirty().await;
                agent.pending_approval = None;
                agent.tool_queue.clear();
                agent.iteration = 0;
                agent.history.clear();
                agent.dirty = false;
            }
            UserAction::ClearHistory => {
                agent.clear_history();
            }
            UserAction::Quit => {
                break;
            }
        }
    }
}

pub async fn run(
    config: Config,
    mut user_rx: mpsc::Receiver<UserAction>,
    event_tx: mpsc::Sender<AppEvent>,
) {
    let mut agent = Agent::new(config);

    if let Err(e) = agent.initialize().await {
        let _ = event_tx
            .send(AppEvent::Error {
                message: format!("Initialization failed: {}", e),
            })
            .await;
        return;
    }

    run_loop(&mut agent, &mut user_rx, &event_tx).await;

    agent.shutdown().await;
}
