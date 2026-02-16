//! SMS / iMessage Autopilot tools
//!
//! Proactive text messaging — send texts on behalf of the user, auto-reply when
//! busy, and generate conversation summaries.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Send an SMS/iMessage to a contact
pub struct SendSmsTool {
    db: Arc<KnowledgeDb>,
}

impl SendSmsTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for SendSmsTool {
    fn name(&self) -> &str {
        "send_sms"
    }

    fn description(&self) -> &str {
        "Send an iMessage or SMS to a contact. Looks up the contact by name or phone number \
         and sends the message via Messages.app. Logs the outgoing message in the knowledge graph."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "to": {
                    "type": "string",
                    "description": "Recipient: phone number (e.g., '+15551234567') or contact name"
                },
                "message": {
                    "type": "string",
                    "description": "Message text to send"
                }
            }),
            vec!["to", "message"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let to = input
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'to' parameter"))?;
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'message' parameter"))?;

        if message.len() > 5000 {
            return Err(anyhow::anyhow!("Message too long (max 5000 characters)"));
        }
        if to.len() > 200 {
            return Err(anyhow::anyhow!("Recipient too long (max 200 characters)"));
        }

        debug!("Sending SMS to: {}", to);

        // Sanitize for AppleScript injection
        let safe_to = to.replace('\"', "\\\"").replace('\\', "\\\\");
        let safe_message = message.replace('\"', "\\\"").replace('\\', "\\\\");

        // Send via AppleScript on macOS
        #[cfg(target_os = "macos")]
        {
            let script = format!(
                r#"tell application "Messages"
    set targetService to 1st account whose service type = iMessage
    set targetBuddy to participant "{}" of targetService
    send "{}" to targetBuddy
end tell"#,
                safe_to, safe_message
            );

            let output = tokio::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .await?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Failed to send message: {}", stderr));
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            return Err(anyhow::anyhow!(
                "SMS/iMessage sending is only available on macOS"
            ));
        }

        // Log in knowledge graph
        let _ = self
            .db
            .insert_entity(
                &format!("sms_sent:{}", to),
                "sent_message",
                Some(serde_json::json!({
                    "to": to,
                    "preview": &message[..message.len().min(100)],
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                })),
            )
            .await;

        Ok(format!("Message sent to {}: \"{}\"", to, message))
    }
}

/// Set up auto-reply for when the user is busy
pub struct SetAutoReplyTool {
    db: Arc<KnowledgeDb>,
}

impl SetAutoReplyTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for SetAutoReplyTool {
    fn name(&self) -> &str {
        "set_auto_reply"
    }

    fn description(&self) -> &str {
        "Configure auto-reply for incoming messages when you're busy. Checks your calendar \
         for current/upcoming meetings and sends a contextual auto-reply. Can be set for a \
         specific duration or until manually disabled."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "enabled": {
                    "type": "boolean",
                    "description": "Enable or disable auto-reply"
                },
                "message": {
                    "type": "string",
                    "description": "Auto-reply message template. Use {time} for estimated availability."
                },
                "duration_minutes": {
                    "type": "number",
                    "description": "Auto-disable after N minutes (0 = manual disable, default: 60)"
                },
                "contacts_only": {
                    "type": "boolean",
                    "description": "Only auto-reply to known contacts (default: true)"
                },
                "use_calendar": {
                    "type": "boolean",
                    "description": "Auto-detect busy periods from calendar (default: true)"
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
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("I'm currently busy and will get back to you soon.");
        let duration = input
            .get("duration_minutes")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);
        let contacts_only = input
            .get("contacts_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let use_calendar = input
            .get("use_calendar")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        debug!("Setting auto-reply: enabled={}", enabled);

        // Store auto-reply config in knowledge graph
        let config = serde_json::json!({
            "enabled": enabled,
            "message": message,
            "duration_minutes": duration,
            "contacts_only": contacts_only,
            "use_calendar": use_calendar,
            "activated_at": chrono::Utc::now().to_rfc3339(),
            "expires_at": if duration > 0 {
                Some((chrono::Utc::now() + chrono::Duration::minutes(duration as i64)).to_rfc3339())
            } else {
                None
            },
        });

        // Upsert the auto-reply config entity
        let _ = self
            .db
            .insert_entity("auto_reply_config", "auto_reply", Some(config.clone()))
            .await;

        if enabled {
            let expiry = if duration > 0 {
                format!("Auto-disables in {} minutes.", duration)
            } else {
                "Will stay active until manually disabled.".to_string()
            };

            Ok(format!(
                "Auto-reply enabled:\n\
                 - Message: \"{}\"\n\
                 - Contacts only: {}\n\
                 - Calendar-aware: {}\n\
                 - {}\n\n\
                 Note: The autonomous loop will check for incoming messages and send auto-replies \
                 when you're detected as busy.",
                message, contacts_only, use_calendar, expiry
            ))
        } else {
            Ok("Auto-reply disabled.".to_string())
        }
    }
}

/// Summarize recent message conversations
pub struct MessageSummaryTool {
    db: Arc<KnowledgeDb>,
}

impl MessageSummaryTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for MessageSummaryTool {
    fn name(&self) -> &str {
        "message_summary"
    }

    fn description(&self) -> &str {
        "Summarize recent message conversations across all channels (iMessage, Discord, Slack). \
         Identifies key topics discussed, action items mentioned, and unanswered messages."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "hours": {
                    "type": "number",
                    "description": "Summarize messages from the last N hours (default: 24)"
                },
                "channel": {
                    "type": "string",
                    "description": "Filter to specific channel: imessage, discord, slack, all (default: all)"
                },
                "contact": {
                    "type": "string",
                    "description": "Filter to specific contact/user name"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let hours = input.get("hours").and_then(|v| v.as_u64()).unwrap_or(24);
        let channel = input
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let contact = input.get("contact").and_then(|v| v.as_str());

        debug!(
            "Summarizing messages: last {} hours, channel: {}",
            hours, channel
        );

        // Get recent conversations from knowledge graph
        let channel_filter = if channel == "all" {
            None
        } else {
            Some(channel)
        };
        let conversations = self
            .db
            .get_recent_conversations(channel_filter, 50)
            .await
            .unwrap_or_default();

        let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours as i64);
        let filtered: Vec<_> = conversations
            .iter()
            .filter(|c| c.created_at > cutoff)
            .filter(|c| channel == "all" || c.channel == channel)
            .filter(|c| contact.map(|name| c.sender.contains(name)).unwrap_or(true))
            .collect();

        if filtered.is_empty() {
            return Ok(format!(
                "No messages found in the last {} hours{}.",
                hours,
                contact.map(|c| format!(" from {}", c)).unwrap_or_default()
            ));
        }

        let messages_str = filtered
            .iter()
            .map(|c| {
                format!(
                    "[{}] {} ({}): {}",
                    c.created_at.format("%H:%M"),
                    c.sender,
                    c.channel,
                    &c.content[..c.content.len().min(200)]
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            "# Message Summary (last {} hours)\n\n\
             {} messages found{}:\n\n{}\n\n\
             Please provide:\n\
             1. **Key Topics** — main subjects discussed\n\
             2. **Action Items** — things that need follow-up\n\
             3. **Unanswered** — messages that haven't been responded to\n\
             4. **Highlights** — important or time-sensitive items",
            hours,
            filtered.len(),
            contact.map(|c| format!(" from {}", c)).unwrap_or_default(),
            messages_str
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<KnowledgeDb> {
        Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_sms.db")).unwrap())
    }

    #[test]
    fn test_send_sms_schema() {
        let tool = SendSmsTool::new(test_db());
        assert_eq!(tool.name(), "send_sms");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"to".to_string()));
        assert!(required.contains(&"message".to_string()));
    }

    #[test]
    fn test_set_auto_reply_schema() {
        let tool = SetAutoReplyTool::new(test_db());
        assert_eq!(tool.name(), "set_auto_reply");
    }

    #[test]
    fn test_message_summary_schema() {
        let tool = MessageSummaryTool::new(test_db());
        assert_eq!(tool.name(), "message_summary");
    }
}
