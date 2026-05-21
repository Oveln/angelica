use crate::llm::types::ToolCall;

pub(crate) struct BatchedEdit {
    pub tc_id: String,
    pub search: String,
    pub replace: String,
}

pub(crate) enum PendingApproval {
    Single {
        tc_ids: Vec<String>,
        tool_name: String,
        args: serde_json::Value,
        display_args: String,
        preview: Option<String>,
    },
    BatchedEdit {
        tc_ids: Vec<String>,
        path: String,
        edits: Vec<BatchedEdit>,
        display_args: String,
        preview: Option<String>,
    },
}

impl PendingApproval {
    pub fn tc_ids(&self) -> &[String] {
        match self {
            PendingApproval::Single { tc_ids, .. } => tc_ids,
            PendingApproval::BatchedEdit { tc_ids, .. } => tc_ids,
        }
    }

    pub fn tool_name(&self) -> &str {
        match self {
            PendingApproval::Single { tool_name, .. } => tool_name,
            PendingApproval::BatchedEdit { .. } => "edit_file",
        }
    }

    pub fn display_args(&self) -> &str {
        match self {
            PendingApproval::Single { display_args, .. } => display_args,
            PendingApproval::BatchedEdit { display_args, .. } => display_args,
        }
    }

    pub fn preview(&self) -> Option<&str> {
        match self {
            PendingApproval::Single { preview, .. } => preview.as_deref(),
            PendingApproval::BatchedEdit { preview, .. } => preview.as_deref(),
        }
    }
}

// ── Tool call grouping ──

pub(crate) struct GroupedEdit {
    pub tc_id: String,
    pub search: String,
    pub replace: String,
}

pub(crate) enum ToolCallGroup {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::{FunctionCall, ToolCall};

    fn tc(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: format!("call_{}", name),
            function: FunctionCall {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    fn edit_tc(id: &str, path: &str, search: &str, replace: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            function: FunctionCall {
                name: "edit_file".to_string(),
                arguments: serde_json::json!({"path": path, "search": search, "replace": replace})
                    .to_string(),
            },
        }
    }

    fn unwrap_batched(group: &ToolCallGroup) -> (&str, usize) {
        match group {
            ToolCallGroup::BatchedEdits { path, edits } => (path, edits.len()),
            _ => panic!("expected BatchedEdits"),
        }
    }

    fn is_single(group: &ToolCallGroup) -> bool {
        matches!(group, ToolCallGroup::Single { .. })
    }

    #[test]
    fn empty_input() {
        let groups = group_tool_calls(vec![]);
        assert!(groups.is_empty());
    }

    #[test]
    fn single_non_edit_tools_stay_separate() {
        let tcs = vec![
            tc("read_file", r#"{"path":"a.rs"}"#),
            tc("run_command", r#"{"command":"ls"}"#),
        ];
        let groups = group_tool_calls(tcs);
        assert_eq!(groups.len(), 2);
        assert!(is_single(&groups[0]));
        assert!(is_single(&groups[1]));
    }

    #[test]
    fn edits_same_file_are_batched() {
        let tcs = vec![
            edit_tc("1", "a.rs", "old1", "new1"),
            edit_tc("2", "a.rs", "old2", "new2"),
            edit_tc("3", "a.rs", "old3", "new3"),
        ];
        let groups = group_tool_calls(tcs);
        assert_eq!(groups.len(), 1);
        assert_eq!(unwrap_batched(&groups[0]), ("a.rs", 3));
    }

    #[test]
    fn edits_different_files_are_separate_batches() {
        let tcs = vec![
            edit_tc("1", "a.rs", "old1", "new1"),
            edit_tc("2", "b.rs", "old2", "new2"),
        ];
        let groups = group_tool_calls(tcs);
        assert_eq!(groups.len(), 2);
        assert_eq!(unwrap_batched(&groups[0]), ("a.rs", 1));
        assert_eq!(unwrap_batched(&groups[1]), ("b.rs", 1));
    }

    #[test]
    fn mixed_edits_and_other_tools() {
        let tcs = vec![
            edit_tc("1", "a.rs", "old1", "new1"),
            edit_tc("2", "a.rs", "old2", "new2"),
            tc("read_file", r#"{"path":"a.rs"}"#),
            edit_tc("3", "a.rs", "old3", "new3"),
        ];
        let groups = group_tool_calls(tcs);
        assert_eq!(groups.len(), 3);
        assert_eq!(unwrap_batched(&groups[0]), ("a.rs", 2));
        assert!(is_single(&groups[1]));
        assert_eq!(unwrap_batched(&groups[2]), ("a.rs", 1));
    }

    #[test]
    fn edit_with_invalid_json_is_treated_as_single() {
        let tcs = vec![ToolCall {
            id: "bad".to_string(),
            function: FunctionCall {
                name: "edit_file".to_string(),
                arguments: "not json".to_string(),
            },
        }];
        let groups = group_tool_calls(tcs);
        assert_eq!(groups.len(), 1);
        let (path, count) = unwrap_batched(&groups[0]);
        assert_eq!(path, "");
        assert_eq!(count, 1);
    }

    #[test]
    fn pending_approval_single_accessors() {
        let pa = PendingApproval::Single {
            tc_ids: vec!["c1".to_string()],
            tool_name: "run_command".to_string(),
            args: serde_json::json!({"command": "ls"}),
            display_args: r#"{"command":"ls"}"#.to_string(),
            preview: Some("$ ls".to_string()),
        };
        assert_eq!(pa.tc_ids(), &["c1".to_string()]);
        assert_eq!(pa.tool_name(), "run_command");
        assert_eq!(pa.display_args(), r#"{"command":"ls"}"#);
        assert_eq!(pa.preview(), Some("$ ls"));
    }

    #[test]
    fn pending_approval_batched_accessors() {
        let pa = PendingApproval::BatchedEdit {
            tc_ids: vec!["c1".to_string(), "c2".to_string()],
            path: "a.rs".to_string(),
            edits: vec![BatchedEdit {
                tc_id: "c1".into(),
                search: "a".into(),
                replace: "b".into(),
            }],
            display_args: "{}".to_string(),
            preview: None,
        };
        assert_eq!(pa.tc_ids().len(), 2);
        assert_eq!(pa.tool_name(), "edit_file");
        assert!(pa.preview().is_none());
    }
}
