//! MCP server implementation over STDIO
//!
//! Reads JSON-RPC requests from stdin, dispatches to handler, writes responses to stdout.

use anyhow::{Result, Context};
use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info, warn};

use crate::adapter::McpToolAdapter;
use crate::protocol::*;

/// MCP server that communicates over STDIO
pub struct McpServer {
    adapter: McpToolAdapter,
}

impl McpServer {
    /// Create a new MCP server wrapping a tool adapter
    pub fn new(adapter: McpToolAdapter) -> Self {
        Self { adapter }
    }

    /// Run the MCP server over STDIO (stdin/stdout)
    pub async fn serve_stdio(&self) -> Result<()> {
        info!("MCP server starting on STDIO");

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            debug!("MCP received: {}", &line[..line.len().min(200)]);

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    warn!("Invalid JSON-RPC request: {}", e);
                    let err_response = JsonRpcResponse::error(
                        Value::Null,
                        -32700,
                        format!("Parse error: {}", e),
                    );
                    write_response(&mut stdout, &err_response).await?;
                    continue;
                }
            };

            let response = self.handle_request(request).await;

            if let Some(resp) = response {
                write_response(&mut stdout, &resp).await?;
            }
        }

        info!("MCP server STDIO closed");
        Ok(())
    }

    /// Handle a single JSON-RPC request
    async fn handle_request(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = request.id.clone().unwrap_or(Value::Null);

        match request.method.as_str() {
            "initialize" => {
                let result = InitializeResult {
                    protocol_version: "2024-11-05".to_string(),
                    capabilities: ServerCapabilities {
                        tools: ToolsCapability { list_changed: false },
                    },
                    server_info: ServerInfo {
                        name: "meepo".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                    },
                };
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(result).unwrap(),
                ))
            }

            "notifications/initialized" => {
                info!("MCP client initialized");
                None // Notifications don't get responses
            }

            "tools/list" => {
                let tools = self.adapter.list_tools();
                info!("MCP tools/list: returning {} tools", tools.len());
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::json!({ "tools": tools }),
                ))
            }

            "tools/call" => {
                let name = request.params.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let arguments = request.params.get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                if name.is_empty() {
                    return Some(JsonRpcResponse::error(
                        id,
                        INVALID_PARAMS,
                        "Missing 'name' parameter".to_string(),
                    ));
                }

                info!("MCP tools/call: {}", name);
                let result = self.adapter.call_tool(name, arguments).await;
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(result).unwrap(),
                ))
            }

            "ping" => {
                Some(JsonRpcResponse::success(id, serde_json::json!({})))
            }

            _ => {
                warn!("MCP unknown method: {}", request.method);
                // Notifications (no id) shouldn't get error responses
                if request.id.is_none() {
                    None
                } else {
                    Some(JsonRpcResponse::error(
                        id,
                        METHOD_NOT_FOUND,
                        format!("Unknown method: {}", request.method),
                    ))
                }
            }
        }
    }
}

/// Write a JSON-RPC response to stdout (newline-delimited)
async fn write_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    response: &JsonRpcResponse,
) -> Result<()> {
    let json = serde_json::to_string(response)
        .context("Failed to serialize response")?;
    debug!("MCP sending: {}", &json[..json.len().min(200)]);
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use meepo_core::tools::ToolRegistry;

    fn make_server() -> McpServer {
        let registry = Arc::new(ToolRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        McpServer::new(adapter)
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let server = make_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "initialize".to_string(),
            params: serde_json::json!({}),
        };
        let resp = server.handle_request(req).await.unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "meepo");
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let server = make_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(2)),
            method: "tools/list".to_string(),
            params: serde_json::json!({}),
        };
        let resp = server.handle_request(req).await.unwrap();
        let result = resp.result.unwrap();
        assert!(result["tools"].is_array());
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_name() {
        let server = make_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(3)),
            method: "tools/call".to_string(),
            params: serde_json::json!({}),
        };
        let resp = server.handle_request(req).await.unwrap();
        assert!(resp.error.is_some());
    }

    #[tokio::test]
    async fn test_handle_ping() {
        let server = make_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(4)),
            method: "ping".to_string(),
            params: serde_json::json!({}),
        };
        let resp = server.handle_request(req).await.unwrap();
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_handle_unknown_method() {
        let server = make_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(5)),
            method: "unknown/method".to_string(),
            params: serde_json::json!({}),
        };
        let resp = server.handle_request(req).await.unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_notification_no_response() {
        let server = make_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: serde_json::json!({}),
        };
        let resp = server.handle_request(req).await;
        assert!(resp.is_none());
    }
}
