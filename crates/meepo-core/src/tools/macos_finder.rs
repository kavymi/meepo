//! macOS Finder tools — file selection, reveal, tags, trash, recent files

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::FinderProvider;

pub struct FinderGetSelectionTool {
    provider: Box<dyn FinderProvider>,
}

impl Default for FinderGetSelectionTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FinderGetSelectionTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_finder_provider()
                .expect("Finder provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for FinderGetSelectionTool {
    fn name(&self) -> &str {
        "finder_get_selection"
    }

    fn description(&self) -> &str {
        "Get the file paths of the currently selected items in Finder."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Getting Finder selection");
        self.provider.get_selection().await
    }
}

pub struct FinderRevealTool {
    provider: Box<dyn FinderProvider>,
}

impl Default for FinderRevealTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FinderRevealTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_finder_provider()
                .expect("Finder provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for FinderRevealTool {
    fn name(&self) -> &str {
        "finder_reveal"
    }

    fn description(&self) -> &str {
        "Reveal a file or folder in Finder (opens the containing folder and selects the item)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file or folder to reveal"
                }
            }),
            vec!["path"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        self.provider.reveal_in_finder(path).await
    }
}

pub struct FinderTagTool {
    provider: Box<dyn FinderProvider>,
}

impl Default for FinderTagTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FinderTagTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_finder_provider()
                .expect("Finder provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for FinderTagTool {
    fn name(&self) -> &str {
        "finder_tag"
    }

    fn description(&self) -> &str {
        "Add or remove a macOS color tag on a file. Valid tags: Red, Orange, Yellow, Green, Blue, Purple, Gray."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file"
                },
                "tag": {
                    "type": "string",
                    "description": "Color tag name: Red, Orange, Yellow, Green, Blue, Purple, Gray"
                },
                "remove": {
                    "type": "boolean",
                    "description": "If true, remove all tags instead of adding (default: false)"
                }
            }),
            vec!["path", "tag"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let tag = input
            .get("tag")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'tag' parameter"))?;
        let remove = input
            .get("remove")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.provider.set_tag(path, tag, remove).await
    }
}

pub struct FinderQuickLookTool {
    provider: Box<dyn FinderProvider>,
}

impl Default for FinderQuickLookTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FinderQuickLookTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_finder_provider()
                .expect("Finder provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for FinderQuickLookTool {
    fn name(&self) -> &str {
        "finder_quick_look"
    }

    fn description(&self) -> &str {
        "Preview a file using macOS Quick Look."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to preview"
                }
            }),
            vec!["path"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        self.provider.quick_look(path).await
    }
}

pub struct TrashFileTool {
    provider: Box<dyn FinderProvider>,
}

impl Default for TrashFileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TrashFileTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_finder_provider()
                .expect("Finder provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for TrashFileTool {
    fn name(&self) -> &str {
        "trash_file"
    }

    fn description(&self) -> &str {
        "Move a file to the Trash (recoverable, not permanent delete)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to trash"
                }
            }),
            vec!["path"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        self.provider.trash_file(path).await
    }
}

pub struct EmptyTrashTool {
    provider: Box<dyn FinderProvider>,
}

impl Default for EmptyTrashTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EmptyTrashTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_finder_provider()
                .expect("Finder provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for EmptyTrashTool {
    fn name(&self) -> &str {
        "empty_trash"
    }

    fn description(&self) -> &str {
        "Permanently empty the macOS Trash."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Emptying trash");
        self.provider.empty_trash().await
    }
}

pub struct GetRecentFilesTool {
    provider: Box<dyn FinderProvider>,
}

impl Default for GetRecentFilesTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GetRecentFilesTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_finder_provider()
                .expect("Finder provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetRecentFilesTool {
    fn name(&self) -> &str {
        "get_recent_files"
    }

    fn description(&self) -> &str {
        "List recently opened files from the past N days."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "days": {
                    "type": "number",
                    "description": "Number of days to look back (default: 7, max: 30)"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of files to return (default: 20, max: 50)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let days = input.get("days").and_then(|v| v.as_u64()).unwrap_or(7);
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);
        self.provider.get_recent_files(days, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_finder_get_selection_schema() {
        let tool = FinderGetSelectionTool::new();
        assert_eq!(tool.name(), "finder_get_selection");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_finder_reveal_schema() {
        let tool = FinderRevealTool::new();
        assert_eq!(tool.name(), "finder_reveal");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_finder_reveal_missing_path() {
        let tool = FinderRevealTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_trash_file_schema() {
        let tool = TrashFileTool::new();
        assert_eq!(tool.name(), "trash_file");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_recent_files_schema() {
        let tool = GetRecentFilesTool::new();
        assert_eq!(tool.name(), "get_recent_files");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_finder_tag_schema() {
        let tool = FinderTagTool::new();
        assert_eq!(tool.name(), "finder_tag");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"path".to_string()));
        assert!(required.contains(&"tag".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_finder_tag_missing_params() {
        let tool = FinderTagTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());

        let result = tool.execute(serde_json::json!({"path": "/tmp/test"})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_finder_quick_look_schema() {
        let tool = FinderQuickLookTool::new();
        assert_eq!(tool.name(), "finder_quick_look");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"path".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_finder_quick_look_missing_path() {
        let tool = FinderQuickLookTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_trash_file_missing_path() {
        let tool = TrashFileTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_empty_trash_schema() {
        let tool = EmptyTrashTool::new();
        assert_eq!(tool.name(), "empty_trash");
    }
}
