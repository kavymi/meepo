//! macOS Terminal automation and developer tools

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::TerminalProvider;

pub struct ListTerminalTabsTool {
    provider: Box<dyn TerminalProvider>,
}

impl ListTerminalTabsTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_terminal_provider()
                .expect("Terminal provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ListTerminalTabsTool {
    fn name(&self) -> &str {
        "list_terminal_tabs"
    }

    fn description(&self) -> &str {
        "List open Terminal.app tabs with their titles and running processes."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Listing terminal tabs");
        self.provider.list_terminal_tabs().await
    }
}

pub struct SendTerminalCommandTool {
    provider: Box<dyn TerminalProvider>,
}

impl SendTerminalCommandTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_terminal_provider()
                .expect("Terminal provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SendTerminalCommandTool {
    fn name(&self) -> &str {
        "send_terminal_command"
    }

    fn description(&self) -> &str {
        "Send a command to Terminal.app (executes in a tab)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "tab_index": {
                    "type": "number",
                    "description": "Tab index to send to (optional, defaults to front window)"
                }
            }),
            vec!["command"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?;
        let tab_index = input
            .get("tab_index")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        debug!("Sending terminal command");
        self.provider
            .send_terminal_command(command, tab_index)
            .await
    }
}

pub struct GetOpenPortsTool {
    provider: Box<dyn TerminalProvider>,
}

impl GetOpenPortsTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_terminal_provider()
                .expect("Terminal provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetOpenPortsTool {
    fn name(&self) -> &str {
        "get_open_ports"
    }

    fn description(&self) -> &str {
        "List all listening TCP ports and their associated processes."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Getting open ports");
        self.provider.get_open_ports().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_list_terminal_tabs_schema() {
        let tool = ListTerminalTabsTool::new();
        assert_eq!(tool.name(), "list_terminal_tabs");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_send_terminal_command_schema() {
        let tool = SendTerminalCommandTool::new();
        assert_eq!(tool.name(), "send_terminal_command");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema.get("required").cloned().unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"command".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_send_terminal_command_missing() {
        let tool = SendTerminalCommandTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_open_ports_schema() {
        let tool = GetOpenPortsTool::new();
        assert_eq!(tool.name(), "get_open_ports");
    }
}
