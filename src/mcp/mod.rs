mod client;
mod transport;

use std::collections::HashMap;

use anyhow::Result;
use serde_json::Value;

use crate::config::McpConfig;
use crate::llm::types::ToolSpec;

/// Manages connections to all configured MCP servers.
pub struct McpClientManager {
    clients: HashMap<String, client::McpClient>,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    /// Connect to all configured MCP servers.
    pub async fn connect_all(config: &McpConfig) -> Result<Self> {
        let mut clients = HashMap::new();

        for (name, server_config) in &config.servers {
            match client::McpClient::connect(name, server_config).await {
                Ok(client) => {
                    clients.insert(name.clone(), client);
                }
                Err(e) => {
                    tracing::warn!("MCP server '{}' failed to connect: {}", name, e);
                }
            }
        }

        Ok(Self { clients })
    }

    /// Aggregate tool specs from all connected servers.
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        let mut specs = Vec::new();
        for client in self.clients.values() {
            specs.extend(client.tool_specs());
        }
        specs
    }

    /// Call a tool on the server that owns it.
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String> {
        for client in self.clients.values_mut() {
            if client.has_tool(name) {
                return client.call_tool(name, arguments).await;
            }
        }
        Err(anyhow::anyhow!(
            "MCP tool '{}' not found on any server",
            name
        ))
    }

    /// Shut down all connections.
    pub async fn disconnect_all(&mut self) {
        for (_, mut client) in self.clients.drain() {
            client.shutdown().await;
        }
    }
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}
