//! macOS productivity tools — clipboard write, frontmost document

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::ProductivityProvider;

pub struct SetClipboardTool {
    provider: Box<dyn ProductivityProvider>,
}

impl Default for SetClipboardTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SetClipboardTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_productivity_provider()
                .expect("Productivity provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SetClipboardTool {
    fn name(&self) -> &str {
        "set_clipboard"
    }

    fn description(&self) -> &str {
        "Write text to the system clipboard."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "text": {
                    "type": "string",
                    "description": "Text to copy to the clipboard"
                }
            }),
            vec!["text"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let text = input
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;
        debug!("Setting clipboard ({} chars)", text.len());
        self.provider.set_clipboard(text).await
    }
}

pub struct GetFrontmostDocumentTool {
    provider: Box<dyn ProductivityProvider>,
}

impl Default for GetFrontmostDocumentTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GetFrontmostDocumentTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_productivity_provider()
                .expect("Productivity provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetFrontmostDocumentTool {
    fn name(&self) -> &str {
        "get_frontmost_document"
    }

    fn description(&self) -> &str {
        "Get the file path of the document open in the frontmost application."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Getting frontmost document");
        self.provider.get_frontmost_document().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_set_clipboard_schema() {
        let tool = SetClipboardTool::new();
        assert_eq!(tool.name(), "set_clipboard");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"text".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_set_clipboard_missing_text() {
        let tool = SetClipboardTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_frontmost_document_schema() {
        let tool = GetFrontmostDocumentTool::new();
        assert_eq!(tool.name(), "get_frontmost_document");
    }
}
