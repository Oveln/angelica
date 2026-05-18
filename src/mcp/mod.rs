use std::collections::HashMap;

use crate::llm::types::ToolSpec;

pub struct McpClientManager {
    tool_to_server: HashMap<String, String>,
    tool_specs: Vec<ToolSpec>,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            tool_to_server: HashMap::new(),
            tool_specs: Vec::new(),
        }
    }

    pub async fn connect_all(_config: &crate::config::McpConfig) -> anyhow::Result<Self> {
        Ok(Self::new())
    }

    pub fn tool_specs(&self) -> &[ToolSpec] {
        &self.tool_specs
    }

    pub async fn call_tool(
        &self,
        name: &str,
        _arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        if self.tool_to_server.contains_key(name) {
            Ok(format!("MCP tool '{}' called (not yet implemented)", name))
        } else {
            Err(anyhow::anyhow!("MCP tool '{}' not found", name))
        }
    }

    pub async fn disconnect_all(&mut self) {
        self.tool_to_server.clear();
        self.tool_specs.clear();
    }
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}
