use std::fs;
use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::{Tool, make_unified_diff};

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Parent directories are auto-created. The user will be asked to review the diff before writing."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn preview(&self, args: Value) -> anyhow::Result<Option<String>> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
        let new_content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

        let path = Path::new(path_str);
        let old_content = if path.exists() {
            fs::read_to_string(path).unwrap_or_default()
        } else {
            String::new()
        };

        if old_content == new_content {
            return Ok(Some(format!("(no changes) {}", path.display())));
        }

        let diff = make_unified_diff(path_str, &old_content, new_content);
        Ok(Some(diff))
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

        let path = Path::new(path_str);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                anyhow::anyhow!("Failed to create directory {}: {}", parent.display(), e)
            })?;
        }

        let existed = path.exists();
        fs::write(path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", path.display(), e))?;

        if existed {
            Ok(format!(
                "Wrote {} bytes to {}",
                content.len(),
                path.display()
            ))
        } else {
            Ok(format!(
                "Created {} ({} bytes)",
                path.display(),
                content.len()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("output.txt");

        let tool = WriteFileTool;
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "content": "hello"}))
            .await
            .unwrap();
        assert!(result.contains("Created"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello");
    }

    #[tokio::test]
    async fn write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a/b/c/file.txt");

        let tool = WriteFileTool;
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "content": "nested"}))
            .await
            .unwrap();
        assert!(result.contains("Created"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "nested");
    }

    #[test]
    fn preview_shows_diff() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "old content\n").unwrap();

        let tool = WriteFileTool;
        let preview = tool
            .preview(json!({"path": file.to_str().unwrap(), "content": "new content\n"}))
            .unwrap()
            .unwrap();
        assert!(preview.contains("---"));
        assert!(preview.contains("+++"));
        assert!(preview.contains("-old content"));
        assert!(preview.contains("+new content"));
    }
}
