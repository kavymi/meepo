//! macOS system control tools — volume, dark mode, battery, WiFi, apps, etc.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::SystemControlProvider;

pub struct GetVolumeTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for GetVolumeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GetVolumeTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetVolumeTool {
    fn name(&self) -> &str {
        "get_volume"
    }

    fn description(&self) -> &str {
        "Get the current system volume level, input volume, and mute status."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Getting volume");
        self.provider.get_volume().await
    }
}

pub struct SetVolumeTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for SetVolumeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SetVolumeTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SetVolumeTool {
    fn name(&self) -> &str {
        "set_volume"
    }

    fn description(&self) -> &str {
        "Set the system output volume level (0-100)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "level": {
                    "type": "number",
                    "description": "Volume level from 0 to 100"
                }
            }),
            vec!["level"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let level = input
            .get("level")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'level' parameter"))? as u8;
        debug!("Setting volume to {}", level);
        self.provider.set_volume(level).await
    }
}

pub struct ToggleMuteTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for ToggleMuteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ToggleMuteTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ToggleMuteTool {
    fn name(&self) -> &str {
        "toggle_mute"
    }

    fn description(&self) -> &str {
        "Toggle the system audio mute on/off."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Toggling mute");
        self.provider.toggle_mute().await
    }
}

pub struct ToggleDarkModeTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for ToggleDarkModeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ToggleDarkModeTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ToggleDarkModeTool {
    fn name(&self) -> &str {
        "toggle_dark_mode"
    }

    fn description(&self) -> &str {
        "Enable or disable macOS dark mode, or toggle it."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "enabled": {
                    "type": "boolean",
                    "description": "Set to true for dark mode, false for light mode. Omit to toggle."
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let enabled = if let Some(v) = input.get("enabled").and_then(|v| v.as_bool()) {
            v
        } else {
            !self.provider.get_dark_mode().await?
        };
        debug!("Setting dark mode to {}", enabled);
        self.provider.set_dark_mode(enabled).await
    }
}

pub struct SetDoNotDisturbTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for SetDoNotDisturbTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SetDoNotDisturbTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SetDoNotDisturbTool {
    fn name(&self) -> &str {
        "set_do_not_disturb"
    }

    fn description(&self) -> &str {
        "Enable or disable macOS Do Not Disturb / Focus mode."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "enabled": {
                    "type": "boolean",
                    "description": "true to enable DND, false to disable"
                }
            }),
            vec!["enabled"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let enabled = input
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing 'enabled' parameter"))?;
        self.provider.set_do_not_disturb(enabled).await
    }
}

pub struct GetBatteryStatusTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for GetBatteryStatusTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GetBatteryStatusTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetBatteryStatusTool {
    fn name(&self) -> &str {
        "get_battery_status"
    }

    fn description(&self) -> &str {
        "Get battery percentage, charging state, and time remaining."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        self.provider.get_battery_status().await
    }
}

pub struct GetWifiInfoTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for GetWifiInfoTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GetWifiInfoTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetWifiInfoTool {
    fn name(&self) -> &str {
        "get_wifi_info"
    }

    fn description(&self) -> &str {
        "Get current WiFi network name (SSID) and IP address."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        self.provider.get_wifi_info().await
    }
}

pub struct GetDiskUsageTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for GetDiskUsageTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GetDiskUsageTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetDiskUsageTool {
    fn name(&self) -> &str {
        "get_disk_usage"
    }

    fn description(&self) -> &str {
        "Get disk space usage for the root volume (free/used/total)."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        self.provider.get_disk_usage().await
    }
}

pub struct LockScreenTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for LockScreenTool {
    fn default() -> Self {
        Self::new()
    }
}

impl LockScreenTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for LockScreenTool {
    fn name(&self) -> &str {
        "lock_screen"
    }

    fn description(&self) -> &str {
        "Lock the screen immediately."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        self.provider.lock_screen().await
    }
}

pub struct SleepDisplayTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for SleepDisplayTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SleepDisplayTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SleepDisplayTool {
    fn name(&self) -> &str {
        "sleep_display"
    }

    fn description(&self) -> &str {
        "Put the display to sleep immediately."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        self.provider.sleep_display().await
    }
}

pub struct GetRunningAppsTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for GetRunningAppsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GetRunningAppsTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for GetRunningAppsTool {
    fn name(&self) -> &str {
        "get_running_apps"
    }

    fn description(&self) -> &str {
        "List all currently running (visible) applications."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        self.provider.get_running_apps().await
    }
}

pub struct QuitAppTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for QuitAppTool {
    fn default() -> Self {
        Self::new()
    }
}

impl QuitAppTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for QuitAppTool {
    fn name(&self) -> &str {
        "quit_app"
    }

    fn description(&self) -> &str {
        "Gracefully quit an application by name."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "app_name": {
                    "type": "string",
                    "description": "Name of the application to quit"
                }
            }),
            vec!["app_name"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let app_name = input
            .get("app_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'app_name' parameter"))?;
        if app_name.contains('/') || app_name.contains('\\') {
            return Err(anyhow::anyhow!("App name cannot contain path separators"));
        }
        self.provider.quit_app(app_name).await
    }
}

pub struct ForceQuitAppTool {
    provider: Box<dyn SystemControlProvider>,
}

impl Default for ForceQuitAppTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ForceQuitAppTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_system_control_provider()
                .expect("System control provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ForceQuitAppTool {
    fn name(&self) -> &str {
        "force_quit_app"
    }

    fn description(&self) -> &str {
        "Force-quit a hung application by name (sends SIGKILL)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "app_name": {
                    "type": "string",
                    "description": "Name of the application to force-quit"
                }
            }),
            vec!["app_name"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let app_name = input
            .get("app_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'app_name' parameter"))?;
        if app_name.contains('/') || app_name.contains('\\') {
            return Err(anyhow::anyhow!("App name cannot contain path separators"));
        }
        self.provider.force_quit_app(app_name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_volume_schema() {
        let tool = GetVolumeTool::new();
        assert_eq!(tool.name(), "get_volume");
        assert!(!tool.description().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_set_volume_schema() {
        let tool = SetVolumeTool::new();
        assert_eq!(tool.name(), "set_volume");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"level".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_set_volume_missing_level() {
        let tool = SetVolumeTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_toggle_dark_mode_schema() {
        let tool = ToggleDarkModeTool::new();
        assert_eq!(tool.name(), "toggle_dark_mode");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_quit_app_schema() {
        let tool = QuitAppTool::new();
        assert_eq!(tool.name(), "quit_app");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_quit_app_path_traversal() {
        let tool = QuitAppTool::new();
        let result = tool
            .execute(serde_json::json!({"app_name": "../evil"}))
            .await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_battery_schema() {
        let tool = GetBatteryStatusTool::new();
        assert_eq!(tool.name(), "get_battery_status");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_running_apps_schema() {
        let tool = GetRunningAppsTool::new();
        assert_eq!(tool.name(), "get_running_apps");
    }
}
