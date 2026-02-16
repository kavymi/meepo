//! meepo-apple-mcp — Standalone MCP server for Apple apps via AppleScript
//!
//! Exposes Mail, Calendar, Contacts, Reminders, Notes, and Music tools
//! over the Model Context Protocol (STDIO JSON-RPC 2.0).
//!
//! Usage:
//!   meepo-apple-mcp
//!
//! Connect from Meepo via config:
//!   [[mcp.clients]]
//!   name = "apple"
//!   command = "meepo-apple-mcp"

mod applescript;
mod tools;

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info, warn};

/// JSON-RPC 2.0 request
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// Write a JSON-RPC success response
async fn write_success<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    id: Value,
    result: Value,
) -> Result<()> {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    });
    let json = serde_json::to_string(&response)?;
    debug!("Sending: {}", &json[..json.len().min(200)]);
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

/// Write a JSON-RPC error response
async fn write_error<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    id: Value,
    code: i64,
    message: &str,
) -> Result<()> {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    });
    let json = serde_json::to_string(&response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    info!(
        "meepo-apple-mcp v{} starting on STDIO",
        env!("CARGO_PKG_VERSION")
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        debug!("Received: {}", &line[..line.len().min(200)]);

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                warn!("Invalid JSON-RPC: {}", e);
                write_error(
                    &mut stdout,
                    Value::Null,
                    -32700,
                    &format!("Parse error: {}", e),
                )
                .await?;
                continue;
            }
        };

        let id = request.id.clone().unwrap_or(Value::Null);

        match request.method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false }
                    },
                    "serverInfo": {
                        "name": "meepo-apple-mcp",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                });
                write_success(&mut stdout, id, result).await?;
            }

            "notifications/initialized" => {
                info!("MCP client initialized");
                // Notifications don't get responses
            }

            "tools/list" => {
                let tool_defs = tools::all_tools();
                info!("tools/list: returning {} tools", tool_defs.len());
                write_success(&mut stdout, id, serde_json::json!({ "tools": tool_defs })).await?;
            }

            "tools/call" => {
                let name = request
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let arguments = request
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                if name.is_empty() {
                    write_error(&mut stdout, id, -32602, "Missing 'name' parameter").await?;
                    continue;
                }

                info!("tools/call: {}", name);

                let result = match tools::execute(name, arguments).await {
                    Ok(text) => serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": text,
                        }]
                    }),
                    Err(e) => serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {}", e),
                        }],
                        "isError": true,
                    }),
                };

                write_success(&mut stdout, id, result).await?;
            }

            "ping" => {
                write_success(&mut stdout, id, serde_json::json!({})).await?;
            }

            _ => {
                if request.id.is_some() {
                    warn!("Unknown method: {}", request.method);
                    write_error(
                        &mut stdout,
                        id,
                        -32601,
                        &format!("Unknown method: {}", request.method),
                    )
                    .await?;
                }
            }
        }
    }

    info!("STDIO closed, shutting down");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_initialize_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(serde_json::json!(1)));
    }

    #[test]
    fn test_parse_tools_list_request() {
        let json = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/list");
    }

    #[test]
    fn test_parse_tools_call_request() {
        let json = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_emails","arguments":{"limit":5}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/call");
        assert_eq!(req.params["name"], "read_emails");
        assert_eq!(req.params["arguments"]["limit"], 5);
    }

    #[test]
    fn test_parse_notification_no_id() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "notifications/initialized");
        assert!(req.id.is_none());
    }

    #[test]
    fn test_parse_request_missing_params() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "ping");
        assert_eq!(req.params, Value::Null);
    }
}
