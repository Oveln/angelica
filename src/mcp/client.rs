use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::timeout;

use super::transport::StdioTransport;
use crate::config::McpServerConfig;
use crate::llm::types::ToolSpec;

/// Result of the MCP initialize handshake.
#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    version: Option<String>,
}

/// A connected MCP server client.
pub struct McpClient {
    name: String,
    transport: StdioTransport,
    tools: Vec<DiscoveredTool>,
}

/// Tool discovered from an MCP server via tools/list.
#[derive(Debug)]
struct DiscoveredTool {
    name: String,
    description: Option<String>,
    input_schema: Value,
}

impl McpClient {
    /// Connect to a single MCP server, complete the handshake, and discover tools.
    pub async fn connect(name: &str, config: &McpServerConfig) -> Result<Self> {
        match config.transport.as_str() {
            "stdio" => Self::connect_stdio(name, config).await,
            other => bail!("unsupported MCP transport: {}", other),
        }
    }

    async fn connect_stdio(name: &str, config: &McpServerConfig) -> Result<Self> {
        let command = config
            .command
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("MCP server '{}' missing command", name))?;

        let mut transport = timeout(
            Duration::from_secs(10),
            StdioTransport::spawn(command, &config.args, &config.env),
        )
        .await
        .context(format!("timeout spawning MCP server '{}'", name))??;

        // Handshake: initialize.
        let init_result = timeout(
            Duration::from_secs(10),
            transport.request(
                "initialize",
                Some(json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "angelica", "version": "0.1.0" }
                })),
            ),
        )
        .await
        .context(format!("timeout in initialize handshake with '{}'", name))?
        .context(format!("initialize failed for '{}'", name))?;

        let _server_info: ServerInfo =
            serde_json::from_value(init_result.get("serverInfo").cloned().ok_or_else(|| {
                anyhow::anyhow!("initialize response missing 'serverInfo' field")
            })?)
            .context("invalid initialize response")?;

        // Send initialized notification.
        transport
            .notify("notifications/initialized", None)
            .await
            .context("sending initialized notification")?;

        // Discover tools.
        let tools_result = timeout(
            Duration::from_secs(10),
            transport.request("tools/list", Some(json!({}))),
        )
        .await
        .context(format!("timeout listing tools from '{}'", name))?
        .context(format!("tools/list failed for '{}'", name))?;

        let tools = parse_tool_list(&tools_result)
            .context(format!("invalid tools/list response from '{}'", name))?;

        tracing::info!("MCP server '{}' connected: {} tool(s)", name, tools.len());

        Ok(Self {
            name: name.to_string(),
            transport,
            tools,
        })
    }

    /// Tool specs for this server, ready to send to the LLM.
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .map(|t| {
                let desc = t.description.as_deref().unwrap_or("(no description)");
                ToolSpec::new(&t.name, desc, t.input_schema.clone())
            })
            .collect()
    }

    /// Whether this server owns the named tool.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t.name == name)
    }

    /// Call a tool on this server.
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String> {
        let result = timeout(
            Duration::from_secs(60),
            self.transport.request(
                "tools/call",
                Some(json!({
                    "name": name,
                    "arguments": arguments
                })),
            ),
        )
        .await
        .context(format!(
            "timeout calling tool '{}' on '{}'",
            name, self.name
        ))?
        .context(format!("tools/call '{}' on '{}' failed", name, self.name))?;

        // MCP tools/call returns { content: [{ type: "text", text: "..." }, ...], isError?: bool }
        extract_tool_result(&result)
    }

    /// Shut down the connection.
    pub async fn shutdown(&mut self) {
        tracing::info!("disconnecting MCP server '{}'", self.name);
        self.transport.shutdown().await;
    }
}

/// Parse the tools/list response into discovered tools.
fn parse_tool_list(value: &Value) -> Result<Vec<DiscoveredTool>> {
    let tools_val = value
        .get("tools")
        .ok_or_else(|| anyhow::anyhow!("missing 'tools' field"))?;

    let tools_arr = tools_val
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("'tools' is not an array"))?;

    let mut tools = Vec::new();
    for t in tools_arr {
        let name = t
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("tool missing 'name'"))?
            .to_string();
        let description = t
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);
        let input_schema = t
            .get("inputSchema")
            .cloned()
            .unwrap_or(json!({"type": "object", "properties": {}}));
        tools.push(DiscoveredTool {
            name,
            description,
            input_schema,
        });
    }
    Ok(tools)
}

/// Extract human-readable text from a tools/call result.
fn extract_tool_result(value: &Value) -> Result<String> {
    let is_error = value
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let content = value
        .get("content")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    if c.get("type").and_then(|v| v.as_str()) == Some("text") {
                        c.get("text").and_then(|v| v.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    if content.is_empty() {
        // Fallback: return the raw JSON if no structured content.
        return Ok(serde_json::to_string_pretty(value)?);
    }

    if is_error {
        bail!("MCP tool error: {}", content)
    }

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_list_basic() {
        let value = json!({
            "tools": [
                {
                    "name": "read_file",
                    "description": "Read a file",
                    "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } } }
                },
                {
                    "name": "search",
                    "inputSchema": { "type": "object" }
                }
            ]
        });

        let tools = parse_tool_list(&value).unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[0].description.as_deref(), Some("Read a file"));
        assert!(tools[1].description.is_none());
    }

    #[test]
    fn extract_text_result() {
        let value = json!({
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "text", "text": "world" }
            ]
        });
        assert_eq!(extract_tool_result(&value).unwrap(), "hello\nworld");
    }

    #[test]
    fn extract_error_result() {
        let value = json!({
            "isError": true,
            "content": [{ "type": "text", "text": "file not found" }]
        });
        assert!(extract_tool_result(&value).is_err());
    }

    #[test]
    fn extract_fallback_raw_json() {
        let value = json!({ "some": "data" });
        let result = extract_tool_result(&value).unwrap();
        assert!(result.contains("some"));
    }

    fn mock_server_config() -> McpServerConfig {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let script = std::path::Path::new(&manifest_dir)
            .join("tests")
            .join("mock_mcp_server.py");
        McpServerConfig {
            transport: "stdio".to_string(),
            command: Some("python3".to_string()),
            args: vec![script.to_str().unwrap().to_string()],
            url: None,
            env: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn connect_handshake_and_tool_discovery() {
        let config = mock_server_config();
        let client = McpClient::connect("mock", &config)
            .await
            .expect("connect should succeed");

        let specs = client.tool_specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].function.name, "echo");
        assert_eq!(
            specs[0].function.description.as_deref(),
            Some("Echo back the input")
        );

        assert!(client.has_tool("echo"));
        assert!(!client.has_tool("nonexistent"));
    }

    #[tokio::test]
    async fn call_echo_tool() {
        let config = mock_server_config();
        let mut client = McpClient::connect("mock", &config)
            .await
            .expect("connect should succeed");

        let result = client
            .call_tool("echo", json!({"message": "hello world"}))
            .await
            .expect("call_tool should succeed");
        assert_eq!(result, "hello world");
    }

    #[tokio::test]
    async fn call_unknown_tool_returns_error() {
        let config = mock_server_config();
        let mut client = McpClient::connect("mock", &config)
            .await
            .expect("connect should succeed");

        let result = client.call_tool("no_such_tool", json!({})).await;
        assert!(result.is_err());
    }
}
