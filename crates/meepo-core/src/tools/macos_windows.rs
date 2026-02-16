//! macOS window management tools

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::WindowManagerProvider;

pub struct ListWindowsTool {
    provider: Box<dyn WindowManagerProvider>,
}

impl Default for ListWindowsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ListWindowsTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_window_manager_provider()
                .expect("Window manager not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ListWindowsTool {
    fn name(&self) -> &str {
        "list_windows"
    }

    fn description(&self) -> &str {
        "List all open windows with their app name, title, position, and size."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Listing windows");
        self.provider.list_windows().await
    }
}

pub struct MoveWindowTool {
    provider: Box<dyn WindowManagerProvider>,
}

impl Default for MoveWindowTool {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveWindowTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_window_manager_provider()
                .expect("Window manager not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for MoveWindowTool {
    fn name(&self) -> &str {
        "move_window"
    }

    fn description(&self) -> &str {
        "Move and optionally resize an application's window."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "app_name": {
                    "type": "string",
                    "description": "Name of the application whose window to move"
                },
                "x": {
                    "type": "number",
                    "description": "X position (pixels from left)"
                },
                "y": {
                    "type": "number",
                    "description": "Y position (pixels from top)"
                },
                "width": {
                    "type": "number",
                    "description": "Optional new width in pixels"
                },
                "height": {
                    "type": "number",
                    "description": "Optional new height in pixels"
                }
            }),
            vec!["app_name", "x", "y"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let app_name = input
            .get("app_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'app_name' parameter"))?;
        let x = input
            .get("x")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'x' parameter"))? as i32;
        let y = input
            .get("y")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'y' parameter"))? as i32;
        let width = input
            .get("width")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let height = input
            .get("height")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        debug!("Moving window of {} to ({}, {})", app_name, x, y);
        self.provider
            .move_window(app_name, x, y, width, height)
            .await
    }
}

pub struct MinimizeWindowTool {
    provider: Box<dyn WindowManagerProvider>,
}

impl Default for MinimizeWindowTool {
    fn default() -> Self {
        Self::new()
    }
}

impl MinimizeWindowTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_window_manager_provider()
                .expect("Window manager not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for MinimizeWindowTool {
    fn name(&self) -> &str {
        "minimize_window"
    }

    fn description(&self) -> &str {
        "Minimize a window. If no app specified, minimizes the frontmost window."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "app_name": {
                    "type": "string",
                    "description": "Application name (optional, defaults to frontmost app)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let app_name = input.get("app_name").and_then(|v| v.as_str());
        debug!("Minimizing window");
        self.provider.minimize_window(app_name).await
    }
}

pub struct FullscreenWindowTool {
    provider: Box<dyn WindowManagerProvider>,
}

impl Default for FullscreenWindowTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FullscreenWindowTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_window_manager_provider()
                .expect("Window manager not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for FullscreenWindowTool {
    fn name(&self) -> &str {
        "fullscreen_window"
    }

    fn description(&self) -> &str {
        "Toggle fullscreen on a window. If no app specified, toggles the frontmost window."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "app_name": {
                    "type": "string",
                    "description": "Application name (optional, defaults to frontmost app)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let app_name = input.get("app_name").and_then(|v| v.as_str());
        debug!("Toggling fullscreen");
        self.provider.fullscreen_window(app_name).await
    }
}

pub struct ArrangeWindowsTool {
    provider: Box<dyn WindowManagerProvider>,
}

impl Default for ArrangeWindowsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrangeWindowsTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_window_manager_provider()
                .expect("Window manager not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ArrangeWindowsTool {
    fn name(&self) -> &str {
        "arrange_windows"
    }

    fn description(&self) -> &str {
        "Arrange windows in a layout. Supported layouts: 'split' (side-by-side), 'cascade'."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "layout": {
                    "type": "string",
                    "description": "Layout name: 'split' (side-by-side) or 'cascade'"
                }
            }),
            vec!["layout"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let layout = input
            .get("layout")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'layout' parameter"))?;
        debug!("Arranging windows: {}", layout);
        self.provider.arrange_windows(layout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_list_windows_schema() {
        let tool = ListWindowsTool::new();
        assert_eq!(tool.name(), "list_windows");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_move_window_schema() {
        let tool = MoveWindowTool::new();
        assert_eq!(tool.name(), "move_window");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"app_name".to_string()));
        assert!(required.contains(&"x".to_string()));
        assert!(required.contains(&"y".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_move_window_missing_params() {
        let tool = MoveWindowTool::new();
        let result = tool
            .execute(serde_json::json!({"app_name": "Finder"}))
            .await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_arrange_windows_schema() {
        let tool = ArrangeWindowsTool::new();
        assert_eq!(tool.name(), "arrange_windows");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_arrange_windows_missing_layout() {
        let tool = ArrangeWindowsTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("layout"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_minimize_window_schema() {
        let tool = MinimizeWindowTool::new();
        assert_eq!(tool.name(), "minimize_window");
        // app_name is optional so required should be empty
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_fullscreen_window_schema() {
        let tool = FullscreenWindowTool::new();
        assert_eq!(tool.name(), "fullscreen_window");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_move_window_missing_all_params() {
        let tool = MoveWindowTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("app_name"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_arrange_windows_required_fields() {
        let tool = ArrangeWindowsTool::new();
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"layout".to_string()));
    }
}
