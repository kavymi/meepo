//! Tool registry and executor system

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::api::ToolDefinition;

pub mod accessibility;
pub mod autonomous;
pub mod browser;
pub mod canvas;
pub mod code;
pub mod delegate;
pub mod filesystem;
pub mod lifestyle;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod macos_finder;
#[cfg(target_os = "macos")]
pub mod macos_keychain;
#[cfg(target_os = "macos")]
pub mod macos_media;
#[cfg(target_os = "macos")]
pub mod macos_messages;
#[cfg(target_os = "macos")]
pub mod macos_productivity;
#[cfg(target_os = "macos")]
pub mod macos_shortcuts;
#[cfg(target_os = "macos")]
pub mod macos_spotlight;
#[cfg(target_os = "macos")]
pub mod macos_system;
#[cfg(target_os = "macos")]
pub mod macos_terminal;
#[cfg(target_os = "macos")]
pub mod macos_windows;
pub mod memory;
pub mod rag;
pub mod sandbox_exec;
pub mod search;
pub mod system;
pub mod usage_stats;
pub mod watchers;

/// Trait for executing tools
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_name: &str, input: Value) -> Result<String>;
    fn list_tools(&self) -> Vec<ToolDefinition>;
}

/// Individual tool handler
#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, input: Value) -> Result<String>;
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<Arc<str>, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool handler
    pub fn register(&mut self, handler: Arc<dyn ToolHandler>) {
        let name: Arc<str> = Arc::from(handler.name());
        debug!("Registering tool: {}", name);
        self.tools.insert(name, handler);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.tools.get(name as &str).cloned()
    }

    /// Number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Get tool definitions for only the named tools
    pub fn filter_tools(&self, names: &[String]) -> Vec<ToolDefinition> {
        names
            .iter()
            .filter_map(|name| self.tools.get(name.as_str()))
            .map(|handler| ToolDefinition {
                name: handler.name().to_string(),
                description: handler.description().to_string(),
                input_schema: handler.input_schema(),
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for ToolRegistry {
    async fn execute(&self, tool_name: &str, input: Value) -> Result<String> {
        debug!("Executing tool: {} with input: {:?}", tool_name, input);

        let handler = self
            .tools
            .get(tool_name)
            .ok_or_else(|| anyhow!("Unknown tool: {}", tool_name))?;

        match handler.execute(input).await {
            Ok(result) => {
                debug!("Tool {} succeeded", tool_name);
                Ok(result)
            }
            Err(e) => {
                warn!("Tool {} failed: {}", tool_name, e);
                Err(e)
            }
        }
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|handler| ToolDefinition {
                name: handler.name().to_string(),
                description: handler.description().to_string(),
                input_schema: handler.input_schema(),
            })
            .collect()
    }
}

/// Helper function to create a JSON schema for tool input
pub fn json_schema(properties: Value, required: Vec<&str>) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTool;

    #[async_trait]
    impl ToolHandler for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }

        fn description(&self) -> &str {
            "A dummy tool for testing"
        }

        fn input_schema(&self) -> Value {
            json_schema(
                serde_json::json!({
                    "message": {
                        "type": "string",
                        "description": "Test message"
                    }
                }),
                vec!["message"],
            )
        }

        async fn execute(&self, _input: Value) -> Result<String> {
            Ok("dummy result".to_string())
        }
    }

    #[tokio::test]
    async fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));

        assert_eq!(registry.len(), 1);

        let result = registry
            .execute("dummy", serde_json::json!({"message": "test"}))
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "dummy result");
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_filter_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));

        let filtered = registry.filter_tools(&["dummy".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "dummy");

        let filtered_empty = registry.filter_tools(&["nonexistent".to_string()]);
        assert!(filtered_empty.is_empty());
    }

    #[test]
    fn test_registry_default() {
        let registry = ToolRegistry::default();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_get() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));

        assert!(registry.get("dummy").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_list_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "dummy");
        assert_eq!(tools[0].description, "A dummy tool for testing");
        assert!(tools[0].input_schema.get("properties").is_some());
    }

    #[test]
    fn test_registry_overwrite() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));
        registry.register(Arc::new(DummyTool));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_json_schema_helper() {
        let schema = json_schema(
            serde_json::json!({
                "name": {"type": "string"},
                "age": {"type": "number"}
            }),
            vec!["name"],
        );
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["name"].is_object());
        assert!(schema["properties"]["age"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "name");
    }

    #[test]
    fn test_json_schema_empty() {
        let schema = json_schema(serde_json::json!({}), vec![]);
        assert_eq!(schema["type"], "object");
        assert!(schema["required"].as_array().unwrap().is_empty());
    }

    struct FailingTool;

    #[async_trait]
    impl ToolHandler for FailingTool {
        fn name(&self) -> &str {
            "failing"
        }
        fn description(&self) -> &str {
            "Always fails"
        }
        fn input_schema(&self) -> Value {
            json_schema(serde_json::json!({}), vec![])
        }
        async fn execute(&self, _input: Value) -> Result<String> {
            Err(anyhow!("intentional failure"))
        }
    }

    #[tokio::test]
    async fn test_registry_execute_failing_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(FailingTool));

        let result = registry.execute("failing", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("intentional failure")
        );
    }

    #[test]
    fn test_filter_tools_partial_match() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool));
        registry.register(Arc::new(FailingTool));

        let filtered = registry.filter_tools(&[
            "dummy".to_string(),
            "nonexistent".to_string(),
            "failing".to_string(),
        ]);
        assert_eq!(filtered.len(), 2);
    }
}
