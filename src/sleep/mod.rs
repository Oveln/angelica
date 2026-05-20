pub mod tools;

use std::path::Path;

use crate::llm::types::ChatMessage;

pub fn build_conversation_text(messages: &[ChatMessage]) -> String {
    if messages.is_empty() {
        return "（无对话记录）".to_string();
    }

    let mut text = String::new();
    for msg in messages {
        match msg.role.as_str() {
            "user" => {
                let name = msg.name.as_deref().unwrap_or("user");
                let content = msg.content.as_deref().unwrap_or("");
                if name == "context" {
                    text.push_str(&format!("[context] {}\n", content));
                } else {
                    text.push_str(&format!("用户: {}\n", content));
                }
            }
            "assistant" => {
                if let Some(content) = &msg.content {
                    text.push_str(&format!("祈芷: {}\n", content));
                }
            }
            "tool" => {
                if let Some(content) = &msg.content {
                    let preview: String = content.chars().take(200).collect();
                    text.push_str(&format!("[tool result] {}\n", preview));
                }
            }
            _ => {}
        }
    }
    text
}

pub fn cleanup_old_snapshots(snapshots_dir: &Path, max_snapshots: usize) -> anyhow::Result<()> {
    if !snapshots_dir.exists() {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(snapshots_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .collect();

    if entries.len() <= max_snapshots {
        return Ok(());
    }

    entries.sort_by_key(|e| e.file_name());
    let to_remove = entries.len() - max_snapshots;
    for entry in entries.iter().take(to_remove) {
        if let Err(e) = std::fs::remove_dir_all(entry.path()) {
            tracing::warn!("Failed to remove old snapshot {:?}: {}", entry.path(), e);
        }
    }
    Ok(())
}
