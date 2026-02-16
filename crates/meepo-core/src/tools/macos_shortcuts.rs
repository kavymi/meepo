//! macOS Apple Shortcuts tools — list and run shortcuts

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::ShortcutsProvider;

pub struct ListShortcutsTool {
    provider: Box<dyn ShortcutsProvider>,
}

impl Default for ListShortcutsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ListShortcutsTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_shortcuts_provider()
                .expect("Shortcuts provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ListShortcutsTool {
    fn name(&self) -> &str {
        "list_shortcuts"
    }

    fn description(&self) -> &str {
        "List all available Apple Shortcuts on this Mac."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Listing shortcuts");
        self.provider.list_shortcuts().await
    }
}

pub struct RunShortcutTool {
    provider: Box<dyn ShortcutsProvider>,
}

impl Default for RunShortcutTool {
    fn default() -> Self {
        Self::new()
    }
}

impl RunShortcutTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_shortcuts_provider()
                .expect("Shortcuts provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for RunShortcutTool {
    fn name(&self) -> &str {
        "run_shortcut"
    }

    fn description(&self) -> &str {
        "Run an Apple Shortcut by name, optionally passing input text."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "name": {
                    "type": "string",
                    "description": "Name of the shortcut to run"
                },
                "input": {
                    "type": "string",
                    "description": "Optional input text to pass to the shortcut"
                }
            }),
            vec!["name"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
        let shortcut_input = input.get("input").and_then(|v| v.as_str());
        debug!("Running shortcut: {}", name);
        self.provider.run_shortcut(name, shortcut_input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_list_shortcuts_schema() {
        let tool = ListShortcutsTool::new();
        assert_eq!(tool.name(), "list_shortcuts");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_run_shortcut_schema() {
        let tool = RunShortcutTool::new();
        assert_eq!(tool.name(), "run_shortcut");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"name".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_run_shortcut_missing_name() {
        let tool = RunShortcutTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
