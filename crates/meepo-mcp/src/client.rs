//! MCP client — connects to external MCP servers and discovers tools

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::protocol::McpTool;
use meepo_core::tools::ToolHandler;

/// Configuration for an external MCP server
#[derive(Debug, Clone)]
pub struct McpClientConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// MCP client that communicates with an external MCP server via STDIO
pub struct McpClient {
    config: McpClientConfig,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<tokio::process::ChildStdin>>,
    reader: Mutex<Option<BufReader<tokio::process::ChildStdout>>>,
    next_id: Mutex<u64>,
}

impl McpClient {
    /// Spawn and connect to an external MCP server
    pub async fn connect(config: McpClientConfig) -> Result<Arc<Self>> {
        info!(
            "Connecting to MCP server: {} ({})",
            config.name, config.command
        );

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server: {}", config.command))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to capture MCP server stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture MCP server stdout"))?;

        // Drain stderr in background so MCP server errors are visible in logs
        if let Some(stderr) = child.stderr.take() {
            let server_name = config.name.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        warn!("MCP server '{}' stderr: {}", server_name, line);
                    }
                }
            });
        }

        let client = Arc::new(Self {
            config,
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
            reader: Mutex::new(Some(BufReader::new(stdout))),
            next_id: Mutex::new(1),
        });

        // Send initialize with a 60s timeout to handle slow npm-based servers
        tokio::time::timeout(std::time::Duration::from_secs(60), client.initialize())
            .await
            .map_err(|_| {
                anyhow!(
                    "MCP server '{}' initialize timed out after 60s",
                    client.config.name
                )
            })??;

        Ok(client)
    }

    /// Send initialize handshake
    async fn initialize(&self) -> Result<()> {
        let result = self
            .send_request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "meepo",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )
            .await?;

        debug!("MCP initialize response: {:?}", result);

        // Send initialized notification
        self.send_notification("notifications/initialized", None)
            .await?;

        info!("MCP client connected to {}", self.config.name);
        Ok(())
    }

    /// Discover tools from the MCP server
    pub async fn discover_tools(self: &Arc<Self>) -> Result<Vec<Arc<dyn ToolHandler>>> {
        let result = self
            .send_request("tools/list", serde_json::json!({}))
            .await?;

        let tools: Vec<McpTool> = serde_json::from_value(
            result
                .get("tools")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();

        info!(
            "Discovered {} tools from MCP server {}",
            tools.len(),
            self.config.name
        );

        let handlers: Vec<Arc<dyn ToolHandler>> = tools
            .into_iter()
            .map(|tool| {
                let prefixed_name = format!("{}:{}", self.config.name, tool.name);
                Arc::new(DynamicMcpTool {
                    name: prefixed_name,
                    remote_name: tool.name,
                    description: tool.description,
                    schema: tool.input_schema,
                    client: self.clone(),
                }) as Arc<dyn ToolHandler>
            })
            .collect();

        Ok(handlers)
    }

    /// Call a tool on the MCP server
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<String> {
        let result = self
            .send_request(
                "tools/call",
                serde_json::json!({
                    "name": name,
                    "arguments": arguments,
                }),
            )
            .await?;

        // Extract text from content array
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            let texts: Vec<&str> = content
                .iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect();
            Ok(texts.join("\n"))
        } else {
            Ok(serde_json::to_string_pretty(&result)?)
        }
    }

    /// Send a JSON-RPC request and wait for response
    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let id = {
            let mut next = self.next_id.lock().await;
            let id = *next;
            *next += 1;
            id
        };

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let request_line = serde_json::to_string(&request)? + "\n";

        // Send request
        {
            let mut stdin_guard = self.stdin.lock().await;
            if let Some(ref mut stdin) = *stdin_guard {
                stdin.write_all(request_line.as_bytes()).await?;
                stdin.flush().await?;
            } else {
                return Err(anyhow!("MCP server stdin not available"));
            }
        }

        // Read response (skip notifications)
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(30), self.read_response(id))
                .await
                .map_err(|_| anyhow!("MCP request timed out after 30s"))??;

        if let Some(error) = response.get("error") {
            let msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow!("MCP error: {}", msg));
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Read until we get a response matching the given id
    async fn read_response(&self, expected_id: u64) -> Result<Value> {
        let mut reader_guard = self.reader.lock().await;
        let reader = reader_guard
            .as_mut()
            .ok_or_else(|| anyhow!("MCP server stdout not available"))?;

        loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line).await?;
            if bytes == 0 {
                return Err(anyhow!("MCP server closed connection"));
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let msg: Value = serde_json::from_str(line).with_context(|| {
                format!(
                    "Invalid JSON from MCP server: {}",
                    &line[..line.len().min(100)]
                )
            })?;

            // Check if this is our response (has matching id)
            if let Some(id) = msg.get("id").and_then(|i| i.as_u64())
                && id == expected_id
            {
                return Ok(msg);
            }

            // Otherwise it's a notification — log and continue
            debug!("MCP notification: {}", &line[..line.len().min(200)]);
        }
    }

    /// Send a JSON-RPC notification (no response expected)
    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });

        let line = serde_json::to_string(&notification)? + "\n";

        let mut stdin_guard = self.stdin.lock().await;
        if let Some(ref mut stdin) = *stdin_guard {
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }

        Ok(())
    }

    /// Shutdown the MCP server process
    pub async fn shutdown(&self) {
        // Try graceful shutdown first
        let _ = self
            .send_notification("notifications/cancelled", None)
            .await;

        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            let _ = child.kill().await;
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Best-effort cleanup — can't await in drop
        if let Ok(mut guard) = self.child.try_lock()
            && let Some(ref mut child) = *guard
        {
            let _ = child.start_kill();
        }
    }
}

/// A tool handler that wraps an external MCP tool
pub struct DynamicMcpTool {
    name: String,
    remote_name: String,
    description: String,
    schema: Value,
    client: Arc<McpClient>,
}

#[async_trait]
impl ToolHandler for DynamicMcpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, input: Value) -> Result<String> {
        debug!(
            "Executing MCP tool {} (remote: {})",
            self.name, self.remote_name
        );
        self.client.call_tool(&self.remote_name, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config() {
        let config = McpClientConfig {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            env: vec![],
        };
        assert_eq!(config.name, "test");
    }

    #[test]
    fn test_dynamic_tool_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        });
        // Can't construct DynamicMcpTool without client, but verify schema types
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_client_config_with_env() {
        let config = McpClientConfig {
            name: "test-server".to_string(),
            command: "/usr/bin/node".to_string(),
            args: vec![
                "server.js".to_string(),
                "--port".to_string(),
                "3000".to_string(),
            ],
            env: vec![
                ("NODE_ENV".to_string(), "production".to_string()),
                ("API_KEY".to_string(), "secret".to_string()),
            ],
        };
        assert_eq!(config.name, "test-server");
        assert_eq!(config.args.len(), 3);
        assert_eq!(config.env.len(), 2);
        assert_eq!(config.env[0].0, "NODE_ENV");
    }

    #[test]
    fn test_client_config_debug() {
        let config = McpClientConfig {
            name: "debug-test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: vec![],
        };
        let debug = format!("{:?}", config);
        assert!(debug.contains("debug-test"));
        assert!(debug.contains("echo"));
    }

    #[test]
    fn test_client_config_clone() {
        let config = McpClientConfig {
            name: "original".to_string(),
            command: "cmd".to_string(),
            args: vec!["arg1".to_string()],
            env: vec![("K".to_string(), "V".to_string())],
        };
        let cloned = config.clone();
        assert_eq!(cloned.name, "original");
        assert_eq!(cloned.args.len(), 1);
        assert_eq!(cloned.env.len(), 1);
    }

    #[tokio::test]
    async fn test_connect_nonexistent_command() {
        let config = McpClientConfig {
            name: "bad".to_string(),
            command: "/nonexistent/binary/path".to_string(),
            args: vec![],
            env: vec![],
        };
        let result = McpClient::connect(config).await;
        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(err.contains("Failed to spawn"));
    }
}
