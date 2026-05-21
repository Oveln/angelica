use tokio::sync::mpsc;

use super::Agent;
use super::events::AppEvent;
use super::group::{BatchedEdit, GroupedEdit, PendingApproval, ToolCallGroup};
use crate::llm::types::ToolCall;
use crate::permission::PermissionAction;

pub(super) enum ProcessOutcome {
    Continue,
    NeedApproval,
}

impl Agent {
    async fn execute_tool(&self, name: &str, args: serde_json::Value) -> String {
        if let Some(tool) = self.run_state.get_tool(name) {
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

    async fn emit_tool_calling(
        &self,
        call_id: &str,
        name: &str,
        arguments: &str,
        event_tx: &mpsc::Sender<AppEvent>,
        stream_to_tui: bool,
    ) {
        if !stream_to_tui {
            return;
        }

        let _ = event_tx
            .send(AppEvent::ToolCalling {
                call_id: call_id.to_string(),
                name: name.to_string(),
                arguments: arguments.to_string(),
            })
            .await;
    }

    async fn record_tool_result_event(
        &mut self,
        call_id: String,
        name: String,
        result: String,
        diff_preview: Option<String>,
        event_tx: &mpsc::Sender<AppEvent>,
        stream_to_tui: bool,
    ) {
        if stream_to_tui {
            let _ = event_tx
                .send(AppEvent::ToolResult {
                    call_id: call_id.clone(),
                    name,
                    result: result.clone(),
                    diff_preview,
                })
                .await;
        }
        if self.run_state.accumulate_history() {
            self.history
                .record_tool_result(call_id, result);
        }
    }

    pub(super) async fn process_one_group(
        &mut self,
        group: ToolCallGroup,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> ProcessOutcome {
        match group {
            ToolCallGroup::Single { tc } => self.process_single_call(tc, event_tx).await,
            ToolCallGroup::BatchedEdits { path, edits } => {
                self.process_batched_edits(path, edits, event_tx).await
            }
        }
    }

    async fn process_single_call(
        &mut self,
        tc: ToolCall,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> ProcessOutcome {
        let stream_to_tui = self.run_state.stream_to_tui();
        let name = tc.function.name.clone();
        let tc_id = tc.id.clone();

        tracing::debug!(tool = name, call_id = tc_id, "processing tool call");

        let args: serde_json::Value = match serde_json::from_str(&tc.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("Invalid JSON in tool call arguments: {}", e);
                self.record_tool_result_event(
                    tc_id,
                    name.clone(),
                    msg,
                    None,
                    event_tx,
                    stream_to_tui,
                )
                .await;
                return ProcessOutcome::Continue;
            }
        };

        let target = self
            .run_state
            .get_tool(&name)
            .and_then(|t| t.permission_target(&args));

        let action = self.evaluate_permission(&name, target.as_deref(), None);

        match action {
            PermissionAction::Allow => {
                let diff_preview = self
                    .run_state
                    .get_tool(&name)
                    .and_then(|t| t.preview(args.clone()).ok().flatten());

                self.emit_tool_calling(
                    &tc.id,
                    &name,
                    &tc.function.arguments,
                    event_tx,
                    stream_to_tui,
                )
                .await;
                let result = self.execute_tool(&name, args).await;
                self.record_tool_result_event(
                    tc.id,
                    name.clone(),
                    result,
                    diff_preview,
                    event_tx,
                    stream_to_tui,
                )
                .await;
                ProcessOutcome::Continue
            }
            PermissionAction::Deny => {
                let msg = format!("Tool '{}' denied by permission policy.", name);
                self.record_tool_result_event(tc.id, name, msg, None, event_tx, stream_to_tui)
                    .await;
                ProcessOutcome::Continue
            }
            PermissionAction::Ask => {
                let preview = self.make_approval_preview(&name, &args, None);
                let diff_preview = extract_diff_preview(&preview);

                if stream_to_tui {
                    let _ = event_tx
                        .send(AppEvent::ApprovalPending {
                            call_id: tc.id.clone(),
                            tool_name: name.clone(),
                            tool_target: target.clone(),
                            preview,
                        })
                        .await;
                }

                if self.run_state.accumulate_history() {
                    self.history
                        .record_tool_result(tc.id.clone(), "Pending user approval...".to_string());
                }
                self.pending_approval = Some(PendingApproval {
                    tc_ids: vec![tc.id],
                    tool_name: name,
                    args,
                    display_args: tc.function.arguments.clone(),
                    batched_edits: None,
                    preview: diff_preview,
                });

                ProcessOutcome::NeedApproval
            }
        }
    }

    async fn process_batched_edits(
        &mut self,
        path: String,
        edits: Vec<GroupedEdit>,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> ProcessOutcome {
        let stream_to_tui = self.run_state.stream_to_tui();

        tracing::debug!(
            tool = "edit_file",
            path,
            count = edits.len(),
            "processing batched tool calls"
        );

        let action = self.evaluate_permission("edit_file", Some(&path), Some("edit_file (batched)"));

        match action {
            PermissionAction::Allow => {
                let searches_replaces: Vec<(String, String)> = edits
                    .iter()
                    .map(|e| (e.search.clone(), e.replace.clone()))
                    .collect();
                let display_args = serde_json::to_string(&serde_json::json!({
                    "path": path,
                    "count": edits.len(),
                }))
                .unwrap_or_default();

                let diff_preview =
                    crate::tools::edit_file::preview_batched(&path, &searches_replaces).ok();

                for e in &edits {
                    self.emit_tool_calling(
                        &e.tc_id,
                        "edit_file",
                        &display_args,
                        event_tx,
                        stream_to_tui,
                    )
                    .await;
                }
                let result =
                    match crate::tools::edit_file::execute_batched(&path, &searches_replaces) {
                        Ok(summary) => summary,
                        Err(e) => format!("Error: {}", e),
                    };
                for e in &edits {
                    self.record_tool_result_event(
                        e.tc_id.clone(),
                        "edit_file".to_string(),
                        result.clone(),
                        diff_preview.clone(),
                        event_tx,
                        stream_to_tui,
                    )
                    .await;
                }
                ProcessOutcome::Continue
            }
            PermissionAction::Deny => {
                let msg = "Tool 'edit_file' denied by permission policy.".to_string();
                for e in &edits {
                    self.record_tool_result_event(
                        e.tc_id.clone(),
                        "edit_file".to_string(),
                        msg.clone(),
                        None,
                        event_tx,
                        stream_to_tui,
                    )
                    .await;
                }
                ProcessOutcome::Continue
            }
            PermissionAction::Ask => {
                let count = edits.len();
                let first_id = edits.first().map(|e| e.tc_id.clone()).unwrap_or_default();
                let display_args = serde_json::to_string(&serde_json::json!({
                    "path": path,
                    "count": count,
                }))
                .unwrap_or_default();

                let batched: Vec<BatchedEdit> = edits
                    .iter()
                    .map(|e| BatchedEdit {
                        tc_id: e.tc_id.clone(),
                        search: e.search.clone(),
                        replace: e.replace.clone(),
                    })
                    .collect();

                let searches_replaces: Vec<(String, String)> = batched
                    .iter()
                    .map(|b| (b.search.clone(), b.replace.clone()))
                    .collect();

                let preview =
                    match crate::tools::edit_file::preview_batched(&path, &searches_replaces) {
                        Ok(p) => p,
                        Err(e) => format!("Preview failed: {}", e),
                    };

                for e in &edits {
                    if self.run_state.accumulate_history() {
                        self.history.record_tool_result(
                            e.tc_id.clone(),
                            "Pending user approval...".to_string(),
                        );
                    }
                }

                self.pending_approval = Some(PendingApproval {
                    tc_ids: batched.iter().map(|b| b.tc_id.clone()).collect(),
                    tool_name: "edit_file".to_string(),
                    args: serde_json::json!({"path": path}),
                    display_args,
                    batched_edits: Some(batched),
                    preview: extract_diff_preview(&preview),
                });

                if stream_to_tui {
                    let _ = event_tx
                        .send(AppEvent::ApprovalPending {
                            call_id: first_id,
                            tool_name: "edit_file".to_string(),
                            tool_target: Some(path),
                            preview,
                        })
                        .await;
                }

                ProcessOutcome::NeedApproval
            }
        }
    }

    fn evaluate_permission(&self, name: &str, target: Option<&str>, label: Option<&str>) -> PermissionAction {
        if self.run_state.skip_permissions() {
            return PermissionAction::Allow;
        }

        let display_name = label.unwrap_or(name);
        tracing::info!("evaluate: tool={}, target={:?}", display_name, target);
        let action = self.permissions.evaluate(name, target);
        tracing::info!("evaluate result: {:?}", action);
        action
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
        if let Some(tool) = self.run_state.get_tool(name) {
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

        let stream_to_tui = self.run_state.stream_to_tui();

        if stream_to_tui {
            for tc_id in &pending.tc_ids {
                let _ = event_tx
                    .send(AppEvent::ToolCalling {
                        call_id: tc_id.clone(),
                        name: pending.tool_name.clone(),
                        arguments: pending.display_args.clone(),
                    })
                    .await;
            }
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
            if self.run_state.accumulate_history() {
                self.history.update_tool_result(tc_id, result.clone());
            }
            if stream_to_tui {
                let _ = event_tx
                    .send(AppEvent::ToolResult {
                        call_id: tc_id.clone(),
                        name: pending.tool_name.clone(),
                        result: result.clone(),
                        diff_preview: pending.preview.clone(),
                    })
                    .await;
            }
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
        let stream_to_tui = self.run_state.stream_to_tui();
        for tc_id in &pending.tc_ids {
            if self.run_state.accumulate_history() {
                self.history.update_tool_result(tc_id, msg.clone());
            }
            if stream_to_tui {
                let _ = event_tx
                    .send(AppEvent::ToolRejected {
                        call_id: tc_id.clone(),
                        feedback: msg.clone(),
                    })
                    .await;
            }
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

fn extract_diff_preview(preview: &str) -> Option<String> {
    let first_line = preview.lines().next().unwrap_or("");
    if !first_line.starts_with("---") && !first_line.starts_with("diff ") {
        return None;
    }
    let all: Vec<&str> = preview.lines().collect();
    let diff_end = all
        .iter()
        .enumerate()
        .rev()
        .find(|(_, l)| {
            l.starts_with(' ')
                || l.starts_with('+')
                || l.starts_with('-')
                || l.starts_with('@')
                || l.starts_with("diff ")
                || l.starts_with("---")
                || l.starts_with("+++")
        })
        .map(|(i, _)| i)
        .unwrap_or(0);
    Some(all[..=diff_end].join("\n"))
}
