use std::fs;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::Tool;

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List entries in a directory. Returns each entry's name and type (file or directory)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path (default: current directory)"
                }
            },
            "required": []
        })
    }

    fn permission_target(&self, args: &Value) -> Option<String> {
        args["path"].as_str().map(String::from)
    }

    fn default_rules(&self) -> Vec<crate::permission::TargetRule> {
        use crate::permission::{PermissionAction, TargetRule};
        vec![TargetRule {
            target: "*".to_string(),
            action: PermissionAction::Allow,
        }]
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let path = std::path::Path::new(path_str);

        let entries = fs::read_dir(path)
            .map_err(|e| anyhow::anyhow!("Failed to read directory {}: {}", path.display(), e))?;

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| anyhow::anyhow!("{}", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

            if is_dir {
                dirs.push(format!("{}/", name));
            } else {
                files.push(name);
            }
        }

        dirs.sort();
        files.sort();

        let mut output = String::new();
        for d in &dirs {
            output.push_str(d);
            output.push('\n');
        }
        for f in &files {
            output.push_str(f);
            output.push('\n');
        }

        if output.is_empty() {
            output = "(empty directory)\n".to_string();
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file1.txt"), "").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let tool = ListDirTool;
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.contains("subdir/"));
        assert!(result.contains("file1.txt"));
        assert!(result.contains("file2.txt"));
    }
}
