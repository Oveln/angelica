use std::fs;
use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::Tool;

const DEFAULT_MAX_LINES: usize = 200;
const HARD_MAX_LINES: usize = 500;

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a file's contents. Supports line range via start_line and max_lines. Returns line-numbered output for large files."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Starting line number (1-based, default 1)"
                },
                "max_lines": {
                    "type": "integer",
                    "description": "Maximum number of lines to return (default 200, max 500)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;

        let path = Path::new(path_str);
        let contents = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;

        let total_lines = contents.lines().count();
        let explicit_range = args
            .get("start_line")
            .or_else(|| args.get("max_lines"))
            .is_some();

        if !explicit_range && total_lines <= DEFAULT_MAX_LINES && contents.len() <= 16 * 1024 {
            return Ok(contents);
        }

        let start_line = match args.get("start_line").and_then(Value::as_u64) {
            Some(0) => return Err(anyhow::anyhow!("start_line must be 1-based")),
            Some(v) => v as usize,
            None => 1,
        };

        let max_lines = match args.get("max_lines").and_then(Value::as_u64) {
            Some(0) => return Err(anyhow::anyhow!("max_lines must be greater than 0")),
            Some(v) => (v as usize).min(HARD_MAX_LINES),
            None => DEFAULT_MAX_LINES,
        };

        if start_line > total_lines {
            return Ok(format!(
                "[NO CONTENT] start_line {} is beyond total_lines {}",
                start_line, total_lines
            ));
        }

        let lines: Vec<&str> = contents.lines().collect();
        let start_idx = start_line - 1;
        let end_idx = (start_idx + max_lines).min(total_lines);

        let mut output = String::new();
        for (offset, line) in lines[start_idx..end_idx].iter().enumerate() {
            let line_no = start_line + offset;
            output.push_str(&format!("{:>6}\u{2502} {}\n", line_no, line));
        }

        if end_idx < total_lines {
            output.push_str(&format!(
                "\n[TRUNCATED] Lines {}-{} of {}. Continue with start_line={}.",
                start_line,
                end_idx,
                total_lines,
                end_idx + 1
            ));
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_small_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let tool = ReadFileTool;
        let result = tool
            .execute(json!({"path": file.to_str().unwrap()}))
            .await
            .unwrap();
        assert_eq!(result, "hello world");
    }

    #[tokio::test]
    async fn read_with_line_range() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        let body: String = (1..=10).map(|n| format!("line {}\n", n)).collect();
        std::fs::write(&file, &body).unwrap();

        let tool = ReadFileTool;
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "start_line": 3, "max_lines": 2}))
            .await
            .unwrap();
        assert!(result.contains("     3\u{2502} line 3"));
        assert!(result.contains("     4\u{2502} line 4"));
        assert!(!result.contains("line 5"));
    }

    #[tokio::test]
    async fn read_not_found() {
        let tool = ReadFileTool;
        let result = tool.execute(json!({"path": "/nonexistent/file.txt"})).await;
        assert!(result.is_err());
    }
}
