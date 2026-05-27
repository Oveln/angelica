use tokio::sync::mpsc;

use super::Agent;
use super::events::{AppEvent, format_approval_label, format_tool_call_display};
use super::group::{BatchedEdit, GroupedEdit, PendingApproval, ToolCallGroup};
use super::modes::RunMode;
use crate::llm::types::ToolCall;
use crate::permission::PermissionAction;

pub(super) enum ProcessOutcome {
    Continue,
    NeedApproval,
}

impl<S: RunMode> Agent<S> {
    #[tracing::instrument(skip(self), fields(tool = name))]
    async fn execute_tool(&mut self, name: &str, args: serde_json::Value) -> String {
        let args_preview =
            crate::agent::truncate_str(&serde_json::to_string(&args).unwrap_or_default(), 200);
        tracing::debug!(args = %args_preview, "executing tool");
        if let Some(tool) = self.run_state.get_tool(name) {
            let result = tool.execute(args).await;
            match &result {
                Ok(r) => tracing::debug!(
                    tool = name,
                    result_len = r.len(),
                    result_preview = %crate::agent::truncate_str(r, 120),
                    "tool executed successfully"
                ),
                Err(e) => tracing::warn!(tool = name, error = %e, "tool execution failed"),
            }
            result.unwrap_or_else(|e| format!("Error: {}", e))
        } else {
            tracing::debug!(tool = name, "delegating to MCP");
            let result = self.mcp.call_tool(name, args).await;
            match &result {
                Ok(r) => tracing::debug!(
                    tool = name,
                    result_len = r.len(),
                    "mcp tool executed successfully"
                ),
                Err(e) => tracing::warn!(tool = name, error = %e, "mcp tool execution failed"),
            }
            result.unwrap_or_else(|e| format!("Error: {}", e))
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

        let display = format_tool_call_display(name, arguments);
        let _ = event_tx
            .send(AppEvent::ToolCalling {
                call_id: call_id.to_string(),
                name: name.to_string(),
                display,
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
            self.history.record_tool_result(call_id, result);
        }
    }

    #[tracing::instrument(skip(self, group, event_tx))]
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

    #[tracing::instrument(skip(self, tc, event_tx), fields(
        tool = %tc.function.name,
        call_id = %tc.id,
    ))]
    async fn process_single_call(
        &mut self,
        tc: ToolCall,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> ProcessOutcome {
        let stream_to_tui = self.run_state.stream_to_tui();
        let name = tc.function.name.clone();
        let tc_id = tc.id.clone();

        tracing::debug!(args = %tc.function.arguments, "processing tool call");

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
                let (tool_label, is_diff) = format_approval_label(&preview);

                if stream_to_tui {
                    let _ = event_tx
                        .send(AppEvent::ApprovalPending {
                            call_id: tc.id.clone(),
                            tool_name: name.clone(),
                            tool_target: target.clone(),
                            preview,
                            tool_label,
                            is_diff,
                        })
                        .await;
                }

                if self.run_state.accumulate_history() {
                    self.history
                        .record_tool_result(tc.id.clone(), "Pending user approval...".to_string());
                }
                self.pending_approval = Some(PendingApproval::Single {
                    tc_ids: vec![tc.id],
                    tool_name: name,
                    args,
                    display_args: tc.function.arguments.clone(),
                    preview: diff_preview,
                });

                ProcessOutcome::NeedApproval
            }
        }
    }

    #[tracing::instrument(skip(self, edits, event_tx), fields(
        path = %path,
        count = edits.len(),
    ))]
    async fn process_batched_edits(
        &mut self,
        path: String,
        edits: Vec<GroupedEdit>,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> ProcessOutcome {
        let stream_to_tui = self.run_state.stream_to_tui();

        tracing::debug!("processing batched tool calls");

        let action =
            self.evaluate_permission("edit_file", Some(&path), Some("edit_file (batched)"));

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

                self.pending_approval = Some(PendingApproval::BatchedEdit {
                    tc_ids: batched.iter().map(|b| b.tc_id.clone()).collect(),
                    path: path.clone(),
                    edits: batched,
                    display_args,
                    preview: extract_diff_preview(&preview),
                });

                let (tool_label, is_diff) = format_approval_label(&preview);
                if stream_to_tui {
                    let _ = event_tx
                        .send(AppEvent::ApprovalPending {
                            call_id: first_id,
                            tool_name: "edit_file".to_string(),
                            tool_target: Some(path),
                            preview,
                            tool_label,
                            is_diff,
                        })
                        .await;
                }

                ProcessOutcome::NeedApproval
            }
        }
    }

    #[tracing::instrument(skip(self), fields(
        tool = name,
        target = target.unwrap_or(""),
        label = label.unwrap_or(name),
    ))]
    fn evaluate_permission(
        &self,
        name: &str,
        target: Option<&str>,
        label: Option<&str>,
    ) -> PermissionAction {
        if self.run_state.skip_permissions() {
            return PermissionAction::Allow;
        }

        let display_name = label.unwrap_or(name);
        tracing::debug!(tool = display_name, target = ?target, "evaluating permission");
        let action = self.permissions.evaluate(name, target);
        tracing::debug!(tool = display_name, action = ?action, "permission evaluation result");
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

    #[tracing::instrument(skip(self, event_tx))]
    pub async fn approve_pending(&mut self, event_tx: &mpsc::Sender<AppEvent>) -> bool {
        let pending = match self.pending_approval.take() {
            Some(p) => p,
            None => {
                tracing::warn!("approve_pending called with no pending approval");
                return false;
            }
        };

        let stream_to_tui = self.run_state.stream_to_tui();
        let tc_ids = pending.tc_ids().to_vec();
        let tool_name = pending.tool_name().to_string();
        let display_args = pending.display_args().to_string();
        let preview = pending.preview().map(String::from);

        tracing::info!(
            tool = %tool_name,
            call_ids = ?tc_ids,
            "executing approved tool"
        );

        if stream_to_tui {
            for tc_id in &tc_ids {
                let display = format_tool_call_display(&tool_name, &display_args);
                let _ = event_tx
                    .send(AppEvent::ToolCalling {
                        call_id: tc_id.clone(),
                        name: tool_name.clone(),
                        display,
                    })
                    .await;
            }
        }

        let result = match &pending {
            PendingApproval::BatchedEdit { edits, path, .. } => {
                let edits: Vec<(String, String)> = edits
                    .iter()
                    .map(|e| (e.search.clone(), e.replace.clone()))
                    .collect();
                match crate::tools::edit_file::execute_batched(path, &edits) {
                    Ok(summary) => summary,
                    Err(e) => format!("Error: {}", e),
                }
            }
            PendingApproval::Single { args, .. } => {
                self.execute_tool(&tool_name, args.clone()).await
            }
        };

        for tc_id in &tc_ids {
            if self.run_state.accumulate_history() {
                self.history.update_tool_result(tc_id, result.clone());
            }
            if stream_to_tui {
                let _ = event_tx
                    .send(AppEvent::ToolResult {
                        call_id: tc_id.clone(),
                        name: tool_name.clone(),
                        result: result.clone(),
                        diff_preview: preview.clone(),
                    })
                    .await;
            }
        }
        true
    }

    #[tracing::instrument(skip(self, event_tx), fields(feedback))]
    pub async fn reject_pending(
        &mut self,
        feedback: &str,
        event_tx: &mpsc::Sender<AppEvent>,
    ) -> bool {
        let pending = match self.pending_approval.take() {
            Some(p) => p,
            None => {
                tracing::warn!("reject_pending called with no pending approval");
                return false;
            }
        };
        tracing::info!(
            tool = %pending.tool_name(),
            feedback = %feedback,
            "tool rejected by user"
        );
        let msg = if feedback.is_empty() {
            "User rejected this operation.".to_string()
        } else {
            format!("User rejected this operation. Feedback: {}", feedback)
        };
        let stream_to_tui = self.run_state.stream_to_tui();
        let tc_ids = pending.tc_ids().to_vec();
        for tc_id in &tc_ids {
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
    let lines: Vec<&str> = preview.lines().collect();
    let end = lines.iter().rposition(|l| is_diff_line(l)).unwrap_or(0);
    Some(lines[..=end].join("\n"))
}

fn is_diff_line(l: &str) -> bool {
    l.starts_with(' ')
        || l.starts_with('+')
        || l.starts_with('-')
        || l.starts_with('@')
        || l.starts_with("diff ")
        || l.starts_with("---")
        || l.starts_with("+++")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_diff_preview_from_unified_diff() {
        let diff = "--- a/foo.rs\n+++ b/foo.rs\n@@ -1,3 +1,3 @@\n-old\n+new\n context\n";
        let result = extract_diff_preview(diff);
        assert!(result.is_some());
        assert!(result.unwrap().contains("+new"));
    }

    #[test]
    fn extract_diff_preview_rejects_non_diff() {
        assert!(extract_diff_preview("just some text").is_none());
        assert!(extract_diff_preview("$ ls").is_none());
    }

    #[test]
    fn extract_diff_preview_from_diff_command() {
        let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let result = extract_diff_preview(diff);
        assert!(result.is_some());
    }

    #[test]
    fn extract_diff_preview_trims_trailing_non_diff_lines() {
        let input =
            "--- a.rs\n+++ a.rs\n@@ -1 +1 @@\n-old\n+new\nsome trailing noise\nmore noise\n";
        let result = extract_diff_preview(input).unwrap();
        // Should end at the last diff line
        assert!(result.ends_with("+new"));
    }
}
