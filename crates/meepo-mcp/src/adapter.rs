//! Adapter between Meepo's ToolRegistry and MCP protocol

use std::sync::Arc;
use serde_json::Value;
use tracing::debug;

use meepo_core::tools::{ToolExecutor, ToolRegistry};

use crate::protocol::{McpTool, ToolCallResult, ToolContent};

/// Adapts Meepo's ToolRegistry to MCP tool format
pub struct McpToolAdapter {
    registry: Arc<ToolRegistry>,
    denylist: Vec<String>,
}

impl McpToolAdapter {
    /// Create a new adapter wrapping a ToolRegistry
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            denylist: vec!["delegate_tasks".to_string()],
        }
    }

    /// Create with custom denylist
    pub fn with_denylist(registry: Arc<ToolRegistry>, denylist: Vec<String>) -> Self {
        Self { registry, denylist }
    }

    /// List all tools as MCP tool definitions
    pub fn list_tools(&self) -> Vec<McpTool> {
        self.registry
            .list_tools()
            .into_iter()
            .filter(|t| !self.denylist.contains(&t.name))
            .map(|t| McpTool {
                name: t.name,
                description: t.description,
                input_schema: t.input_schema,
            })
            .collect()
    }

    /// Execute a tool and return MCP-formatted result
    pub async fn call_tool(&self, name: &str, arguments: Value) -> ToolCallResult {
        if self.denylist.contains(&name.to_string()) {
            return ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: format!("Tool '{}' is not available via MCP", name),
                }],
                is_error: Some(true),
            };
        }

        debug!("MCP calling tool: {}", name);
        match self.registry.execute(name, arguments).await {
            Ok(result) => ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: result,
                }],
                is_error: None,
            },
            Err(e) => ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: format!("Error: {}", e),
                }],
                is_error: Some(true),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meepo_core::tools::ToolRegistry;

    #[test]
    fn test_adapter_creation() {
        let registry = Arc::new(ToolRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        assert!(adapter.list_tools().is_empty());
    }

    #[test]
    fn test_denylist_filters_tools() {
        let registry = Arc::new(ToolRegistry::new());
        let adapter = McpToolAdapter::with_denylist(registry, vec!["blocked".to_string()]);
        // Empty registry, but denylist is set
        assert_eq!(adapter.denylist, vec!["blocked".to_string()]);
    }

    #[tokio::test]
    async fn test_call_denylisted_tool() {
        let registry = Arc::new(ToolRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        let result = adapter.call_tool("delegate_tasks", serde_json::json!({})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("not available"));
    }

    #[tokio::test]
    async fn test_call_unknown_tool() {
        let registry = Arc::new(ToolRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        let result = adapter.call_tool("nonexistent", serde_json::json!({})).await;
        assert_eq!(result.is_error, Some(true));
    }
}
