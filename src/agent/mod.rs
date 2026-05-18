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
    "You are Lilium, a helpful AI assistant. You can use tools to help the user.
When appropriate, use the update_agent_memory tool to remember important information.
Use the update_user_profile tool to store user preferences.
Use the update_soul tool to modify your personality if the user requests it.
Use the query_sessions tool to search past conversation history.
Use the run_command tool to execute shell commands when needed.
Use the read_file tool to read file contents.
Use the write_file tool to create or overwrite files.
Use the edit_file tool to make search/replace edits to files.
Use the list_dir tool to list directory contents.

You can return multiple tool calls in a single response to perform independent operations in parallel.
For example, you can read multiple files, run multiple commands, or mix reads and writes at the same time.";

pub struct Agent {
    config: Config,
    llm: crate::llm::LlmClient,
    memory: Arc<MemoryManager>,
    sessions: Arc<SessionManager>,
    skills: SkillRegistry,
    tools: ToolRegistry,
    mcp: McpClientManager,
    history: History,
    pending_approvals: VecDeque<PendingApproval>,
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
            pending_approvals: VecDeque::new(),
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

    fn auto_execute_names(&self) -> Vec<String> {
        self.tools.auto_execute_names()
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

    pub async fn react_loop(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        let max_iterations = self.config.llm.max_iterations as usize;
        let auto_names = self.auto_execute_names();

        for _ in 0..max_iterations {
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

            let _ = event_tx.send(AppEvent::ToolCallsStart).await;

            let grouped = group_tool_calls(tcs);

            for item in &grouped {
                match item {
                    ToolCallGroup::Single { tc } => {
                        let name = tc.function.name.clone();
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Null);

                        let _ = event_tx
                            .send(AppEvent::ToolCalling {
                                name: name.clone(),
                                arguments: tc.function.arguments.clone(),
                            })
                            .await;

                        if auto_names.contains(&name) {
                            let result = self.execute_tool(&name, args).await;
                            let _ = event_tx
                                .send(AppEvent::ToolResult {
                                    name: name.clone(),
                                    result: result.clone(),
                                })
                                .await;
                            self.history.record_tool_result(tc.id.clone(), result);
                        } else {
                            let (preview, is_tty, command) =
                                self.make_approval_preview(&name, &args, None);

                            self.history.record_tool_result(
                                tc.id.clone(),
                                "Pending user approval...".to_string(),
                            );
                            self.pending_approvals.push_back(PendingApproval {
                                tc_ids: vec![tc.id.clone()],
                                tool_name: name.clone(),
                                args: args.clone(),
                                batched_edits: None,
                            });

                            let _ = event_tx
                                .send(AppEvent::ApprovalPending {
                                    preview,
                                    is_tty_command: is_tty,
                                    command,
                                })
                                .await;
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

                        let preview = match crate::tools::edit_file::preview_batched(
                            path,
                            &searches_replaces,
                        ) {
                            Ok(p) => p,
                            Err(e) => format!("Preview failed: {}", e),
                        };

                        for e in edits {
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

                        self.pending_approvals.push_back(PendingApproval {
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
                    }
                }
            }

            if self.pending_approvals.front().is_some() {
                return true;
            }
        }

        let _ = event_tx
            .send(AppEvent::TextDone {
                full_text: "[Reached maximum iterations]".to_string(),
            })
            .await;
        let _ = event_tx.send(AppEvent::TurnComplete).await;
        false
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

    pub async fn approve_pending(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        let pending = self
            .pending_approvals
            .pop_front()
            .expect("no pending approval");

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

        self.present_next_approval_or_continue(event_tx).await
    }

    pub async fn approve_pending_with_result(
        &mut self,
        output: String,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        let pending = self
            .pending_approvals
            .pop_front()
            .expect("no pending approval");

        let _ = event_tx
            .send(AppEvent::CommandResult {
                output: output.clone(),
            })
            .await;

        for tc_id in &pending.tc_ids {
            self.history.update_tool_result(tc_id, output.clone());
        }

        self.present_next_approval_or_continue(event_tx).await
    }

    pub async fn reject_command(
        &mut self,
        feedback: &str,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        let pending = self
            .pending_approvals
            .pop_front()
            .expect("no pending approval");
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

        self.present_next_approval_or_continue(event_tx).await
    }

    async fn present_next_approval_or_continue(
        &mut self,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        if let Some(pending) = self.pending_approvals.front() {
            let batched: Option<Vec<(String, String)>> = pending.batched_edits.as_ref().map(|b| {
                b.iter()
                    .map(|e| (e.search.clone(), e.replace.clone()))
                    .collect()
            });
            let (preview, is_tty, command) =
                self.make_approval_preview(&pending.tool_name, &pending.args, batched.as_deref());
            let _ = event_tx
                .send(AppEvent::ApprovalPending {
                    preview,
                    is_tty_command: is_tty,
                    command,
                })
                .await;
            return true;
        }
        self.react_loop(event_tx).await
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
                let _ = agent.react_loop(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::ApprovePending => {
                let _ = agent.approve_pending(event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::ApprovePendingWithResult { output } => {
                let _ = agent.approve_pending_with_result(output, event_tx).await;
                agent.save_if_dirty().await;
            }
            UserAction::RejectCommand { feedback } => {
                let _ = agent
                    .reject_command(feedback.as_deref().unwrap_or(""), event_tx)
                    .await;
                agent.save_if_dirty().await;
            }
            UserAction::Interrupt => {
                agent.save_if_dirty().await;
                agent.pending_approvals.clear();
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

    let _ = event_tx.send(AppEvent::Ready).await;

    run_loop(&mut agent, &mut user_rx, &event_tx).await;

    agent.shutdown().await;
}
