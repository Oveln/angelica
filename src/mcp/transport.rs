use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, oneshot};

use std::sync::Arc;

static NEXT_ID: AtomicI64 = AtomicI64::new(1);

/// JSON-RPC 2.0 request.
#[derive(serde::Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: i64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC 2.0 response.
#[derive(serde::Deserialize, Debug)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<i64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(serde::Deserialize, Debug)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

/// JSON-RPC notification (no id, no response).
#[derive(serde::Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

// ── Shared pending map ──────────────────────────────────────────

type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value>>>>>;

// ── Stdio transport ─────────────────────────────────────────────

/// Transport over a stdio subprocess using newline-delimited JSON-RPC 2.0.
pub struct StdioTransport {
    stdin: ChildStdin,
    pending: PendingMap,
    child: Child,
}

impl StdioTransport {
    /// Spawn a subprocess and set up the reader task.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().with_context(|| {
            format!("failed to spawn MCP server: {} {}", command, args.join(" "))
        })?;

        let stdin = child.stdin.take().context("no stdin from child")?;
        let stdout = child.stdout.take().context("no stdout from child")?;

        // Drain stderr to prevent pipe blocking.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "mcp::stderr", "{}", line);
                }
            });
        }

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        start_reader(stdout, pending.clone());

        Ok(Self {
            stdin,
            pending,
            child,
        })
    }

    /// Send a JSON-RPC request and await the response.
    pub async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        self.stdin
            .write_all(line.as_bytes())
            .await
            .context("write to MCP stdin")?;
        self.stdin.flush().await.context("flush MCP stdin")?;

        rx.await.context("MCP response channel dropped")?
    }

    /// Send a JSON-RPC notification (no response).
    pub async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };
        let mut line = serde_json::to_string(&notif)?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .context("write notification to MCP stdin")?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Graceful shutdown: kill the child process.
    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
    }
}

/// Background task reading stdout lines and dispatching responses.
fn start_reader(stdout: ChildStdout, pending: PendingMap) {
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            let parsed: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("MCP: invalid JSON from server: {}", e);
                    continue;
                }
            };

            let id = match parsed.get("id").and_then(|v| v.as_i64()) {
                Some(id) => id,
                None => {
                    // Notification from server — log and ignore.
                    tracing::debug!(target: "mcp::notification", "server notification: {}", line);
                    continue;
                }
            };

            let mut map = pending.lock().await;
            if let Some(tx) = map.remove(&id) {
                let resp: Result<JsonRpcResponse> = serde_json::from_str(&line)
                    .map_err(|e| anyhow::anyhow!("invalid JSON-RPC response: {}", e));
                let result = match resp {
                    Ok(r) => {
                        if let Some(err) = r.error {
                            Err(anyhow::anyhow!("MCP error {}: {}", err.code, err.message))
                        } else {
                            Ok(r.result.unwrap_or(Value::Null))
                        }
                    }
                    Err(e) => Err(e),
                };
                let _ = tx.send(result);
            }
        }
    });
}
