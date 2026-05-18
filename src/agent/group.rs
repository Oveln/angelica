use crate::llm::types::ToolCall;

pub(super) struct BatchedEdit {
    pub tc_id: String,
    pub search: String,
    pub replace: String,
}

pub(super) struct PendingApproval {
    pub tc_ids: Vec<String>,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub batched_edits: Option<Vec<BatchedEdit>>,
}

pub(super) fn needs_tty(command: &str) -> bool {
    let trimmed = command.trim_start();
    trimmed.starts_with("sudo ")
        || trimmed.starts_with("sudo\t")
        || trimmed == "sudo"
        || trimmed.starts_with("su ")
        || trimmed.starts_with("su\t")
        || trimmed == "su"
        || trimmed.starts_with("ssh ")
        || trimmed.starts_with("passwd")
}

// ── Tool call grouping ──

pub(super) struct GroupedEdit {
    pub tc_id: String,
    pub search: String,
    pub replace: String,
}

pub(super) enum ToolCallGroup {
    Single {
        tc: ToolCall,
    },
    BatchedEdits {
        path: String,
        edits: Vec<GroupedEdit>,
    },
}

/// Group consecutive `edit_file` calls that target the same file.
/// Non-edit calls and edit calls for different files break the batch.
pub(super) fn group_tool_calls(tcs: Vec<ToolCall>) -> Vec<ToolCallGroup> {
    let mut groups: Vec<ToolCallGroup> = Vec::new();
    let mut pending_edits: Vec<GroupedEdit> = Vec::new();
    let mut pending_path: Option<String> = None;

    for tc in tcs {
        if tc.function.name == "edit_file" {
            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or_default();
            let path = args["path"].as_str().unwrap_or("").to_string();
            let search = args["search"].as_str().unwrap_or("").to_string();
            let replace = args["replace"].as_str().unwrap_or("").to_string();

            if pending_path.as_deref() == Some(&path) {
                pending_edits.push(GroupedEdit {
                    tc_id: tc.id,
                    search,
                    replace,
                });
            } else {
                if let Some(path) = pending_path.take() {
                    groups.push(ToolCallGroup::BatchedEdits {
                        path,
                        edits: std::mem::take(&mut pending_edits),
                    });
                }
                pending_path = Some(path);
                pending_edits.push(GroupedEdit {
                    tc_id: tc.id,
                    search,
                    replace,
                });
            }
        } else {
            if let Some(path) = pending_path.take() {
                groups.push(ToolCallGroup::BatchedEdits {
                    path,
                    edits: std::mem::take(&mut pending_edits),
                });
            }
            groups.push(ToolCallGroup::Single { tc });
        }
    }

    if let Some(path) = pending_path.take() {
        groups.push(ToolCallGroup::BatchedEdits {
            path,
            edits: std::mem::take(&mut pending_edits),
        });
    }

    groups
}
