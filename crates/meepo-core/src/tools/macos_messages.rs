//! macOS Messages and FaceTime tools

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::MessagesProvider;

pub struct ReadMessagesTool {
    provider: Box<dyn MessagesProvider>,
}

impl Default for ReadMessagesTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadMessagesTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_messages_provider()
                .expect("Messages provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ReadMessagesTool {
    fn name(&self) -> &str {
        "read_messages"
    }

    fn description(&self) -> &str {
        "Read recent iMessages from a contact (phone number or email)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "contact": {
                    "type": "string",
                    "description": "Contact phone number or email address"
                },
                "limit": {
                    "type": "number",
                    "description": "Number of messages to retrieve (default: 20, max: 50)"
                }
            }),
            vec!["contact"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let contact = input
            .get("contact")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'contact' parameter"))?;
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);
        debug!("Reading messages from: {}", contact);
        self.provider.read_messages(contact, limit).await
    }
}

pub struct SendMessageTool {
    provider: Box<dyn MessagesProvider>,
}

impl Default for SendMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SendMessageTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_messages_provider()
                .expect("Messages provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SendMessageTool {
    fn name(&self) -> &str {
        "send_imessage"
    }

    fn description(&self) -> &str {
        "Send an iMessage to a contact (phone number or email)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "contact": {
                    "type": "string",
                    "description": "Contact phone number or email address"
                },
                "message": {
                    "type": "string",
                    "description": "Message text to send"
                }
            }),
            vec!["contact", "message"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let contact = input
            .get("contact")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'contact' parameter"))?;
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'message' parameter"))?;
        debug!("Sending message to: {}", contact);
        self.provider.send_message(contact, message).await
    }
}

pub struct StartFaceTimeTool {
    provider: Box<dyn MessagesProvider>,
}

impl Default for StartFaceTimeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl StartFaceTimeTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_messages_provider()
                .expect("Messages provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for StartFaceTimeTool {
    fn name(&self) -> &str {
        "start_facetime"
    }

    fn description(&self) -> &str {
        "Start a FaceTime audio or video call with a contact."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "contact": {
                    "type": "string",
                    "description": "Contact phone number or email address"
                },
                "audio_only": {
                    "type": "boolean",
                    "description": "If true, start audio-only call (default: false for video)"
                }
            }),
            vec!["contact"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let contact = input
            .get("contact")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'contact' parameter"))?;
        let audio_only = input
            .get("audio_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        debug!("Starting FaceTime with: {}", contact);
        self.provider.start_facetime(contact, audio_only).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_read_messages_schema() {
        let tool = ReadMessagesTool::new();
        assert_eq!(tool.name(), "read_messages");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"contact".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_read_messages_missing_contact() {
        let tool = ReadMessagesTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_send_message_schema() {
        let tool = SendMessageTool::new();
        assert_eq!(tool.name(), "send_imessage");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_start_facetime_schema() {
        let tool = StartFaceTimeTool::new();
        assert_eq!(tool.name(), "start_facetime");
    }
}
