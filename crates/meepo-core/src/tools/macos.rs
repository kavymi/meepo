//! macOS-specific tools using AppleScript

use async_trait::async_trait;
use serde_json::Value;
use anyhow::{Result, Context};
use tokio::process::Command;
use tracing::{debug, warn};

use super::{ToolHandler, json_schema};

/// Sanitize a string for safe use in AppleScript
/// Prevents injection attacks by escaping special characters
pub(crate) fn sanitize_applescript_string(input: &str) -> String {
    input
        .replace('\\', "\\\\")  // Escape backslashes first
        .replace('"', "\\\"")   // Escape double quotes
        .replace('\n', " ")     // Replace newlines with spaces
        .replace('\r', " ")     // Replace carriage returns with spaces
        .chars()
        .filter(|&c| c >= ' ' || c == '\t')  // Remove control characters except tab
        .collect()
}

/// Read emails from Mail.app
pub struct ReadEmailsTool;

#[async_trait]
impl ToolHandler for ReadEmailsTool {
    fn name(&self) -> &str {
        "read_emails"
    }

    fn description(&self) -> &str {
        "Read recent emails from Mail.app. Returns sender, subject, date, and preview for the latest emails."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "limit": {
                    "type": "number",
                    "description": "Number of emails to retrieve (default: 10, max: 50)"
                },
                "mailbox": {
                    "type": "string",
                    "description": "Mailbox to read from (default: 'inbox'). Options: inbox, sent, drafts, trash"
                },
                "search": {
                    "type": "string",
                    "description": "Optional search term to filter by subject or sender"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let limit = input.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(50);
        let mailbox = input.get("mailbox")
            .and_then(|v| v.as_str())
            .unwrap_or("inbox");
        let search = input.get("search")
            .and_then(|v| v.as_str());

        debug!("Reading {} emails from Mail.app ({})", limit, mailbox);

        let safe_mailbox = match mailbox.to_lowercase().as_str() {
            "inbox" => "inbox",
            "sent" => "sent mailbox",
            "drafts" => "drafts",
            "trash" => "trash",
            _ => "inbox",
        };

        let filter_clause = if let Some(term) = search {
            let safe_term = sanitize_applescript_string(term);
            format!(r#" whose (subject contains "{}" or sender contains "{}")"#, safe_term, safe_term)
        } else {
            String::new()
        };

        let script = format!(r#"
tell application "Mail"
    try
        set msgs to (messages 1 thru {} of {}{})
        set output to ""
        repeat with m in msgs
            set msgBody to content of m
            if length of msgBody > 500 then
                set msgBody to text 1 thru 500 of msgBody
            end if
            set output to output & "From: " & (sender of m) & "\n"
            set output to output & "Subject: " & (subject of m) & "\n"
            set output to output & "Date: " & (date received of m as string) & "\n"
            set output to output & "Preview: " & msgBody & "\n"
            set output to output & "---\n"
        end repeat
        return output
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#, limit, safe_mailbox, filter_clause);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
        )
        .await
        .map_err(|_| anyhow::anyhow!("AppleScript execution timed out after 30 seconds"))?
        .context("Failed to execute osascript")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            warn!("Failed to read emails: {}", error);
            Err(anyhow::anyhow!("Failed to read emails: {}", error))
        }
    }
}

/// Read calendar events
pub struct ReadCalendarTool;

#[async_trait]
impl ToolHandler for ReadCalendarTool {
    fn name(&self) -> &str {
        "read_calendar"
    }

    fn description(&self) -> &str {
        "Read calendar events from Calendar.app. Returns today's and upcoming events."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "days_ahead": {
                    "type": "number",
                    "description": "Number of days ahead to look (default: 1)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let days_ahead = input.get("days_ahead")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);

        debug!("Reading calendar events for next {} days", days_ahead);

        let script = format!(r#"
tell application "Calendar"
    try
        set startDate to current date
        set endDate to (current date) + ({} * days)
        set theEvents to (every event of calendar "Calendar" whose start date is greater than or equal to startDate and start date is less than or equal to endDate)
        set output to ""
        repeat with evt in theEvents
            set output to output & "Event: " & (summary of evt) & "\n"
            set output to output & "Start: " & (start date of evt as string) & "\n"
            set output to output & "End: " & (end date of evt as string) & "\n"
            set output to output & "---\n"
        end repeat
        return output
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#, days_ahead);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
        )
        .await
        .map_err(|_| anyhow::anyhow!("AppleScript execution timed out after 30 seconds"))?
        .context("Failed to execute osascript")?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(result)
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            warn!("Failed to read calendar: {}", error);
            Err(anyhow::anyhow!("Failed to read calendar: {}", error))
        }
    }
}

/// Send email via Mail.app
pub struct SendEmailTool;

#[async_trait]
impl ToolHandler for SendEmailTool {
    fn name(&self) -> &str {
        "send_email"
    }

    fn description(&self) -> &str {
        "Send an email using Mail.app. Composes and sends a message to the specified recipient."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "to": {
                    "type": "string",
                    "description": "Recipient email address"
                },
                "subject": {
                    "type": "string",
                    "description": "Email subject"
                },
                "body": {
                    "type": "string",
                    "description": "Email body content"
                },
                "cc": {
                    "type": "string",
                    "description": "Optional CC recipient email address"
                },
                "in_reply_to": {
                    "type": "string",
                    "description": "Optional subject line of email to reply to (enables threading)"
                }
            }),
            vec!["to", "subject", "body"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let to = input.get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'to' parameter"))?;
        let subject = input.get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'subject' parameter"))?;
        let body = input.get("body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'body' parameter"))?;
        let cc = input.get("cc").and_then(|v| v.as_str());
        let in_reply_to = input.get("in_reply_to").and_then(|v| v.as_str());

        if body.len() > 50_000 {
            return Err(anyhow::anyhow!("Email body too long ({} chars, max 50,000)", body.len()));
        }

        let safe_to = sanitize_applescript_string(to);
        let safe_subject = sanitize_applescript_string(subject);
        let safe_body = sanitize_applescript_string(body);

        let script = if let Some(reply_subject) = in_reply_to {
            let safe_reply_subject = sanitize_applescript_string(reply_subject);
            debug!("Replying to email with subject: {}", reply_subject);
            format!(r#"
tell application "Mail"
    try
        set targetMsgs to (every message of inbox whose subject contains "{}")
        if (count of targetMsgs) > 0 then
            set originalMsg to item 1 of targetMsgs
            set replyMsg to reply originalMsg with opening window
            set content of replyMsg to "{}"
            send replyMsg
            return "Reply sent (threaded)"
        else
            set newMessage to make new outgoing message with properties {{subject:"{}", content:"{}", visible:true}}
            tell newMessage
                make new to recipient at end of to recipients with properties {{address:"{}"}}
                send
            end tell
            return "Email sent (no original found for threading)"
        end if
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#, safe_reply_subject, safe_body, safe_subject, safe_body, safe_to)
        } else {
            debug!("Sending new email to: {}", to);
            let cc_block = if let Some(cc_addr) = cc {
                let safe_cc = sanitize_applescript_string(cc_addr);
                format!(r#"
                make new cc recipient at end of cc recipients with properties {{address:"{}"}}"#, safe_cc)
            } else {
                String::new()
            };

            format!(r#"
tell application "Mail"
    try
        set newMessage to make new outgoing message with properties {{subject:"{}", content:"{}", visible:true}}
        tell newMessage
            make new to recipient at end of to recipients with properties {{address:"{}"}}{}
            send
        end tell
        return "Email sent successfully"
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#, safe_subject, safe_body, safe_to, cc_block)
        };

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
        )
        .await
        .map_err(|_| anyhow::anyhow!("AppleScript execution timed out after 30 seconds"))?
        .context("Failed to execute osascript")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            warn!("Failed to send email: {}", error);
            Err(anyhow::anyhow!("Failed to send email: {}", error))
        }
    }
}

/// Create calendar event
pub struct CreateEventTool;

#[async_trait]
impl ToolHandler for CreateEventTool {
    fn name(&self) -> &str {
        "create_calendar_event"
    }

    fn description(&self) -> &str {
        "Create a new calendar event in Calendar.app."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "summary": {
                    "type": "string",
                    "description": "Event title/summary"
                },
                "start_time": {
                    "type": "string",
                    "description": "Start time in ISO8601 format or natural language"
                },
                "duration_minutes": {
                    "type": "number",
                    "description": "Duration in minutes (default: 60)"
                }
            }),
            vec!["summary", "start_time"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let summary = input.get("summary")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'summary' parameter"))?;
        let start_time = input.get("start_time")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'start_time' parameter"))?;
        let duration = input.get("duration_minutes")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        debug!("Creating calendar event: {}", summary);

        // Sanitize inputs to prevent AppleScript injection
        let safe_summary = sanitize_applescript_string(summary);
        let safe_start_time = sanitize_applescript_string(start_time);

        let script = format!(r#"
tell application "Calendar"
    try
        set startDate to date "{}"
        set endDate to startDate + ({} * minutes)
        tell calendar "Calendar"
            make new event with properties {{summary:"{}", start date:startDate, end date:endDate}}
        end tell
        return "Event created successfully"
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#, safe_start_time, duration, safe_summary);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
        )
        .await
        .map_err(|_| anyhow::anyhow!("AppleScript execution timed out after 30 seconds"))?
        .context("Failed to execute osascript")?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(result)
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            warn!("Failed to create event: {}", error);
            Err(anyhow::anyhow!("Failed to create event: {}", error))
        }
    }
}

/// Open application
pub struct OpenAppTool;

#[async_trait]
impl ToolHandler for OpenAppTool {
    fn name(&self) -> &str {
        "open_app"
    }

    fn description(&self) -> &str {
        "Open a macOS application by name."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "app_name": {
                    "type": "string",
                    "description": "Name of the application to open (e.g., 'Safari', 'Terminal')"
                }
            }),
            vec!["app_name"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let app_name = input.get("app_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'app_name' parameter"))?;

        // Prevent path traversal — only allow app names, not paths
        if app_name.contains('/') || app_name.contains('\\') {
            return Err(anyhow::anyhow!("App name cannot contain path separators"));
        }
        if app_name.len() > 100 {
            return Err(anyhow::anyhow!("App name too long (max 100 characters)"));
        }

        debug!("Opening application: {}", app_name);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("open")
                .arg("-a")
                .arg(app_name)
                .output()
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command execution timed out after 30 seconds"))?
        .context("Failed to execute open command")?;

        if output.status.success() {
            Ok(format!("Successfully opened {}", app_name))
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            warn!("Failed to open app: {}", error);
            Err(anyhow::anyhow!("Failed to open app: {}", error))
        }
    }
}

/// Get clipboard content
pub struct GetClipboardTool;

#[async_trait]
impl ToolHandler for GetClipboardTool {
    fn name(&self) -> &str {
        "get_clipboard"
    }

    fn description(&self) -> &str {
        "Get the current content of the system clipboard."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        debug!("Reading clipboard content");

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("pbpaste")
                .output()
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command execution timed out after 30 seconds"))?
        .context("Failed to execute pbpaste")?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(result)
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            warn!("Failed to read clipboard: {}", error);
            Err(anyhow::anyhow!("Failed to read clipboard: {}", error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[test]
    fn test_read_emails_schema() {
        let tool = ReadEmailsTool;
        assert_eq!(tool.name(), "read_emails");
        assert!(!tool.description().is_empty());
        let schema = tool.input_schema();
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_read_calendar_schema() {
        let tool = ReadCalendarTool;
        assert_eq!(tool.name(), "read_calendar");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_send_email_schema() {
        let tool = SendEmailTool;
        assert_eq!(tool.name(), "send_email");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema.get("required").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default();
        assert!(required.contains(&"to".to_string()));
        assert!(required.contains(&"subject".to_string()));
        assert!(required.contains(&"body".to_string()));
    }

    #[test]
    fn test_create_event_schema() {
        let tool = CreateEventTool;
        assert_eq!(tool.name(), "create_calendar_event");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema.get("required").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default();
        assert!(required.contains(&"summary".to_string()));
        assert!(required.contains(&"start_time".to_string()));
    }

    #[test]
    fn test_open_app_schema() {
        let tool = OpenAppTool;
        assert_eq!(tool.name(), "open_app");
        let schema = tool.input_schema();
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_get_clipboard_schema() {
        let tool = GetClipboardTool;
        assert_eq!(tool.name(), "get_clipboard");
    }

    #[test]
    fn test_sanitize_applescript_string() {
        // Test backslash escaping
        assert_eq!(sanitize_applescript_string("test\\path"), "test\\\\path");

        // Test quote escaping
        assert_eq!(sanitize_applescript_string("test\"quote"), "test\\\"quote");

        // Test newline replacement
        assert_eq!(sanitize_applescript_string("test\nline"), "test line");
        assert_eq!(sanitize_applescript_string("test\rline"), "test line");

        // Test control character removal
        let with_control = "test\x01\x02\x03text";
        assert_eq!(sanitize_applescript_string(with_control), "testtext");

        // Test combined attack string
        let attack = "test\"; do shell script \"rm -rf /\" --\"";
        let safe = sanitize_applescript_string(attack);
        assert!(!safe.contains('\n'));
        assert!(safe.contains("\\\""));
    }

    #[tokio::test]
    async fn test_send_email_missing_params() {
        let tool = SendEmailTool;
        let result = tool.execute(serde_json::json!({
            "to": "test@test.com"
        })).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_event_missing_params() {
        let tool = CreateEventTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_open_app_missing_params() {
        let tool = OpenAppTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
