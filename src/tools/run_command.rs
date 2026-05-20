use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::Tool;

pub struct RunCommandTool;

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command. The user will be asked to approve before execution."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 30)",
                    "default": 30
                }
            },
            "required": ["command"]
        })
    }

    fn permission_target(&self, args: &Value) -> Option<String> {
        args["command"].as_str().map(String::from)
    }

    fn default_rules(&self) -> Vec<crate::permission::TargetRule> {
        use crate::permission::{PermissionAction, TargetRule};
        vec![TargetRule {
            target: "*".to_string(),
            action: PermissionAction::Ask,
        }]
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;
        let timeout_secs = args["timeout"].as_u64().unwrap_or(30);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output(),
        )
        .await;

        match output {
            Ok(Ok(output)) => {
                let mut result = String::new();
                if !output.stdout.is_empty() {
                    result.push_str(&String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    result.push_str("\n[stderr]\n");
                    result.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                if !output.status.success() {
                    result.push_str(&format!(
                        "\n[exit code: {}]",
                        output.status.code().unwrap_or(-1)
                    ));
                }
                if result.is_empty() {
                    result = "[no output]".to_string();
                }
                Ok(result)
            }
            Ok(Err(e)) => Ok(format!("[error: {}]", e)),
            Err(_) => Ok(format!("[Timeout after {}s]", timeout_secs)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_echo() {
        let tool = RunCommandTool;
        let result = tool
            .execute(json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn run_fails() {
        let tool = RunCommandTool;
        let result = tool.execute(json!({"command": "exit 1"})).await.unwrap();
        assert!(result.contains("exit code: 1"));
    }
}
