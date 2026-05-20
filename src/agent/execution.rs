use tokio::sync::mpsc;

use super::events::AppEvent;
use super::group::{BatchedEdit, PendingApproval, ToolCallGroup, group_tool_calls};
use super::Agent;

enum ProcessOutcome {
    Continue,
    NeedApproval,
}

impl Agent {
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
                let tc_id = tc.id.clone();
                let args: serde_json::Value =
                    match serde_json::from_str(&tc.function.arguments) {
                        Ok(v) => v,
                        Err(e) => {
                            let msg = format!("Invalid JSON in tool call arguments: {}", e);
                            self.history.record_tool_result(tc_id.clone(), msg.clone());
                            let _ = event_tx
                                .send(AppEvent::ToolResult {
                                    call_id: tc_id,
                                    name: name.clone(),
                                    result: msg,
                                })
                                .await;
                            return ProcessOutcome::Continue;
                        }
                    };

                if self.tools.is_auto_execute(&name) {
                    let _ = event_tx
                        .send(AppEvent::ToolCalling {
                            call_id: tc.id.clone(),
                            name: name.clone(),
                            arguments: tc.function.arguments.clone(),
                        })
                        .await;
                    let result = self.execute_tool(&name, args).await;
                    let _ = event_tx
                        .send(AppEvent::ToolResult {
                            call_id: tc.id.clone(),
                            name: name.clone(),
                            result: result.clone(),
                        })
                        .await;
                    self.history.record_tool_result(tc.id, result);
                    ProcessOutcome::Continue
                } else {
                    let preview = self.make_approval_preview(&name, &args, None);

                    let tc_id = tc.id.clone();
                    let _ = event_tx
                        .send(AppEvent::ApprovalPending {
                            call_id: tc_id.clone(),
                            tool_name: name.clone(),
                            preview,
                        })
                        .await;

                    self.history
                        .record_tool_result(tc_id, "Pending user approval...".to_string());
                    self.pending_approval = Some(PendingApproval {
                        tc_ids: vec![tc.id],
                        tool_name: name,
                        args,
                        display_args: tc.function.arguments.clone(),
                        batched_edits: None,
                    });

                    ProcessOutcome::NeedApproval
                }
            }
            ToolCallGroup::BatchedEdits { path, edits } => {
                let count = edits.len();
                let first_id = edits.first().map(|e| e.tc_id.clone()).unwrap_or_default();
                let display_args = serde_json::to_string(&serde_json::json!({
                    "path": path,
                    "count": count,
                }))
                .unwrap_or_default();

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
                    display_args,
                    batched_edits: Some(batched),
                });

                let _ = event_tx
                    .send(AppEvent::ApprovalPending {
                        call_id: first_id,
                        tool_name: "edit_file".to_string(),
                        preview,
                    })
                    .await;

                ProcessOutcome::NeedApproval
            }
        }
    }

    pub async fn step(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        use crate::llm::{AppStreamEvent, StreamFinal};

        let max_iterations = self.config.llm.max_iterations as usize;

        loop {
            if event_tx.is_closed() {
                return false;
            }
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

            drop(stream_tx);
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
    ) -> String {
        if let Some(edits) = batched_edits {
            let path = args["path"].as_str().unwrap_or("?");
            match crate::tools::edit_file::preview_batched(path, edits) {
                Ok(preview) => return preview,
                Err(e) => {
                    return format!("Preview failed: {}\n\nArguments: {}", e, args);
                }
            }
        }
        if let Some(tool) = self.tools.get(name) {
            match tool.preview(args.clone()) {
                Ok(Some(preview)) => return preview,
                Ok(None) => {}
                Err(e) => {
                    return format!("Preview failed: {}\n\nArguments: {}", e, args);
                }
            }
        }

        if name == "run_command" {
            let cmd = args["command"].as_str().unwrap_or("");
            return format!("$ {}", cmd);
        }

        format!("[{}]\nArguments: {}", name, args)
    }

    pub async fn approve_pending(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        let pending = match self.pending_approval.take() {
            Some(p) => p,
            None => return false,
        };

        for tc_id in &pending.tc_ids {
            let _ = event_tx
                .send(AppEvent::ToolCalling {
                    call_id: tc_id.clone(),
                    name: pending.tool_name.clone(),
                    arguments: pending.display_args.clone(),
                })
                .await;
        }

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

        for tc_id in &pending.tc_ids {
            self.history.update_tool_result(tc_id, result.clone());
            let _ = event_tx
                .send(AppEvent::ToolResult {
                    call_id: tc_id.clone(),
                    name: pending.tool_name.clone(),
                    result: result.clone(),
                })
                .await;
        }
        true
    }

    pub async fn reject_pending(
        &mut self,
        feedback: &str,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        let pending = match self.pending_approval.take() {
            Some(p) => p,
            None => return false,
        };
        let msg = if feedback.is_empty() {
            "User rejected this operation.".to_string()
        } else {
            format!("User rejected this operation. Feedback: {}", feedback)
        };
        for tc_id in &pending.tc_ids {
            self.history.update_tool_result(tc_id, msg.clone());
            let _ = event_tx
                .send(AppEvent::ToolRejected {
                    call_id: tc_id.clone(),
                    feedback: msg.clone(),
                })
                .await;
        }
        true
    }

    pub async fn approve_and_step(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        if !self.approve_pending(event_tx).await {
            return false;
        }
        self.step(event_tx).await
    }

    pub async fn reject_and_step(
        &mut self,
        feedback: &str,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        if !self.reject_pending(feedback, event_tx).await {
            return false;
        }
        self.step(event_tx).await
    }
}
