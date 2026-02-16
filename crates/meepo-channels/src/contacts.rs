//! Apple Contacts channel adapter using AppleScript polling

use crate::bus::MessageChannel;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use meepo_core::types::{ChannelType, IncomingMessage, MessageKind, OutgoingMessage};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Apple Contacts channel adapter that polls Contacts.app for contacts
/// in a designated group and creates contacts from outgoing messages.
///
/// How it works:
///   - Polls Contacts.app for contacts in a specific group (e.g. "Meepo")
///   - New contacts are read as incoming messages, then moved out of the group
///   - Outgoing messages create new contacts with the response in the notes field
pub struct ContactsChannel {
    poll_interval: Duration,
    group_name: String,
    /// Tracks contact IDs we've already processed to avoid duplicates
    seen_ids: Arc<Mutex<HashSet<String>>>,
}

impl ContactsChannel {
    pub fn new(poll_interval: Duration, group_name: String) -> Self {
        Self {
            poll_interval,
            group_name,
            seen_ids: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Sanitize a string for safe use in AppleScript.
    fn escape_applescript(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .chars()
            .filter(|&c| c >= ' ' || c == '\t')
            .collect()
    }

    /// Poll Contacts.app for contacts in the configured group
    async fn poll_contacts(&self, tx: &mpsc::Sender<IncomingMessage>) -> Result<()> {
        let group = Self::escape_applescript(&self.group_name);

        let script = format!(
            r#"
tell application "Contacts"
    try
        if not (exists group "{group}") then
            return ""
        end if
        set output to ""
        set targetGroup to group "{group}"
        set groupPeople to every person of targetGroup
        repeat with p in groupPeople
            set pId to id of p
            set pFirst to ""
            set pLast to ""
            set pOrg to ""
            set pNote to ""
            set pEmail to ""
            set pPhone to ""
            try
                set pFirst to first name of p
            end try
            if pFirst is missing value then set pFirst to ""
            try
                set pLast to last name of p
            end try
            if pLast is missing value then set pLast to ""
            try
                set pOrg to organization of p
            end try
            if pOrg is missing value then set pOrg to ""
            try
                set pNote to note of p
            end try
            if pNote is missing value then set pNote to ""
            try
                set pEmail to value of first email of p
            end try
            if pEmail is missing value then set pEmail to ""
            try
                set pPhone to value of first phone of p
            end try
            if pPhone is missing value then set pPhone to ""
            set output to output & "<<CONTACT_START>>" & "\n"
            set output to output & "ID: " & pId & "\n"
            set output to output & "FirstName: " & pFirst & "\n"
            set output to output & "LastName: " & pLast & "\n"
            set output to output & "Organization: " & pOrg & "\n"
            set output to output & "Email: " & pEmail & "\n"
            set output to output & "Phone: " & pPhone & "\n"
            set output to output & "Note: " & pNote & "\n"
            set output to output & "<<CONTACT_END>>" & "\n"
        end repeat
        return output
    on error errMsg
        return "ERROR: " & errMsg
    end try
end tell
"#
        );

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new("osascript").arg("-e").arg(&script).output(),
        )
        .await
        .map_err(|_| anyhow!("Contacts.app polling timed out"))?
        .map_err(|e| anyhow!("Failed to run osascript: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Contacts.app poll failed: {}", stderr);
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() || stdout.starts_with("ERROR:") {
            if stdout.starts_with("ERROR:") {
                warn!("Contacts.app error: {}", stdout);
            }
            return Ok(());
        }

        for block in stdout.split("<<CONTACT_START>>") {
            let block = block.trim();
            if block.is_empty() || !block.contains("<<CONTACT_END>>") {
                continue;
            }

            let block = block.replace("<<CONTACT_END>>", "");
            let mut id = String::new();
            let mut first_name = String::new();
            let mut last_name = String::new();
            let mut organization = String::new();
            let mut email = String::new();
            let mut phone = String::new();
            let mut note = String::new();

            for line in block.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("ID: ") {
                    id = val.to_string();
                } else if let Some(val) = line.strip_prefix("FirstName: ") {
                    first_name = val.to_string();
                } else if let Some(val) = line.strip_prefix("LastName: ") {
                    last_name = val.to_string();
                } else if let Some(val) = line.strip_prefix("Organization: ") {
                    organization = val.to_string();
                } else if let Some(val) = line.strip_prefix("Email: ") {
                    email = val.to_string();
                } else if let Some(val) = line.strip_prefix("Phone: ") {
                    phone = val.to_string();
                } else if let Some(val) = line.strip_prefix("Note: ") {
                    note = val.to_string();
                }
            }

            if id.is_empty() {
                continue;
            }

            // Skip already-seen contacts
            {
                let mut seen = self.seen_ids.lock().await;
                if seen.contains(&id) {
                    continue;
                }
                seen.insert(id.clone());
            }

            // Build a human-readable name
            let display_name = match (first_name.as_str(), last_name.as_str()) {
                ("", "") => {
                    if organization.is_empty() {
                        "(unnamed contact)".to_string()
                    } else {
                        organization.clone()
                    }
                }
                (f, "") => f.to_string(),
                ("", l) => l.to_string(),
                (f, l) => format!("{} {}", f, l),
            };

            // Build content with all available fields
            let mut parts = vec![format!("Contact: {}", display_name)];
            if !organization.is_empty() {
                parts.push(format!("Organization: {}", organization));
            }
            if !email.is_empty() {
                parts.push(format!("Email: {}", email));
            }
            if !phone.is_empty() {
                parts.push(format!("Phone: {}", phone));
            }
            if !note.is_empty() {
                parts.push(format!("Note: {}", note));
            }
            let content = parts.join("\n");

            let msg_id = format!("contact_{}", id);

            let incoming = IncomingMessage {
                id: msg_id,
                sender: "Contacts.app".to_string(),
                content,
                channel: ChannelType::Contacts,
                timestamp: Utc::now(),
            };

            info!("New contact from Contacts.app: {}", display_name);

            if let Err(e) = tx.send(incoming).await {
                error!("Failed to send contact message to bus: {}", e);
            }

            // Remove the contact from the group so it isn't picked up again
            let remove_script = format!(
                r#"
tell application "Contacts"
    try
        set targetGroup to group "{group}"
        set targetPerson to first person whose id is "{id}"
        remove targetPerson from targetGroup
        save
    end try
end tell
"#,
                group = Self::escape_applescript(&self.group_name),
                id = Self::escape_applescript(&id),
            );

            if let Err(e) = Command::new("osascript")
                .arg("-e")
                .arg(&remove_script)
                .output()
                .await
            {
                warn!("Failed to remove contact from group: {}", e);
            }
        }

        Ok(())
    }

    /// Create a new contact in Contacts.app and add it to the configured group
    async fn create_contact(
        &self,
        first_name: &str,
        last_name: &str,
        email: &str,
        phone: &str,
        note: &str,
    ) -> Result<()> {
        let safe_group = Self::escape_applescript(&self.group_name);
        let safe_first = Self::escape_applescript(first_name);
        let safe_last = Self::escape_applescript(last_name);
        let safe_email = Self::escape_applescript(email);
        let safe_phone = Self::escape_applescript(phone);
        let safe_note = Self::escape_applescript(note);

        // Build property list for the new contact
        let mut props = Vec::new();
        if !safe_first.is_empty() {
            props.push(format!("first name:\"{}\"", safe_first));
        }
        if !safe_last.is_empty() {
            props.push(format!("last name:\"{}\"", safe_last));
        }
        if !safe_note.is_empty() {
            props.push(format!("note:\"{}\"", safe_note));
        }

        let props_str = if props.is_empty() {
            "first name:\"(meepo)\"".to_string()
        } else {
            props.join(", ")
        };

        let email_block = if safe_email.is_empty() {
            String::new()
        } else {
            format!(
                r#"
            make new email at end of emails of newPerson with properties {{label:"work", value:"{safe_email}"}}"#
            )
        };

        let phone_block = if safe_phone.is_empty() {
            String::new()
        } else {
            format!(
                r#"
            make new phone at end of phones of newPerson with properties {{label:"mobile", value:"{safe_phone}"}}"#
            )
        };

        let script = format!(
            r#"
tell application "Contacts"
    try
        if not (exists group "{safe_group}") then
            make new group with properties {{name:"{safe_group}"}}
        end if
        set newPerson to make new person with properties {{{props_str}}}{email_block}{phone_block}
        add newPerson to group "{safe_group}"
        save
        return "OK"
    on error errMsg
        return "ERROR: " & errMsg
    end try
end tell
"#
        );

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new("osascript").arg("-e").arg(&script).output(),
        )
        .await
        .map_err(|_| anyhow!("Contacts create timed out"))?
        .map_err(|e| anyhow!("Failed to run osascript: {}", e))?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout);
            if result.trim().starts_with("ERROR:") {
                return Err(anyhow!("Contacts.app error: {}", result.trim()));
            }
            info!("Contact created: {} {}", safe_first, safe_last);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Failed to create contact: {}", stderr))
        }
    }

    /// Parse outgoing message content into contact fields.
    /// Supports structured format:
    ///   FirstName: ...
    ///   LastName: ...
    ///   Email: ...
    ///   Phone: ...
    ///   Note: ...
    /// Or plain text (treated as a note with the first line as the name).
    fn parse_contact_fields(content: &str) -> (String, String, String, String, String) {
        let mut first_name = String::new();
        let mut last_name = String::new();
        let mut email = String::new();
        let mut phone = String::new();
        let mut note = String::new();

        let has_structured = content.lines().any(|l| {
            let l = l.trim();
            l.starts_with("FirstName:")
                || l.starts_with("LastName:")
                || l.starts_with("Email:")
                || l.starts_with("Phone:")
                || l.starts_with("Note:")
        });

        if has_structured {
            for line in content.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("FirstName:") {
                    first_name = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("LastName:") {
                    last_name = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("Email:") {
                    email = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("Phone:") {
                    phone = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("Note:") {
                    note = val.trim().to_string();
                }
            }
        } else {
            // Plain text: first line is name, rest is note
            match content.split_once('\n') {
                Some((name, body)) => {
                    let parts: Vec<&str> = name.trim().splitn(2, ' ').collect();
                    first_name = parts.first().unwrap_or(&"").to_string();
                    last_name = parts.get(1).unwrap_or(&"").to_string();
                    note = body.trim().to_string();
                }
                None => {
                    first_name = content.trim().to_string();
                }
            }
        }

        (first_name, last_name, email, phone, note)
    }
}

#[async_trait]
impl MessageChannel for ContactsChannel {
    async fn start(&self, tx: mpsc::Sender<IncomingMessage>) -> Result<()> {
        info!("Starting Contacts channel adapter");
        info!("Poll interval: {:?}", self.poll_interval);
        info!("Contacts group: {}", self.group_name);

        let poll_interval = self.poll_interval;
        let group_name = self.group_name.clone();
        let seen_ids = self.seen_ids.clone();

        let channel = ContactsChannel {
            poll_interval,
            group_name,
            seen_ids,
        };

        tokio::spawn(async move {
            info!("Contacts polling task started");
            let mut interval = tokio::time::interval(channel.poll_interval);

            loop {
                interval.tick().await;
                debug!("Polling Contacts.app for new contacts");

                if let Err(e) = channel.poll_contacts(&tx).await {
                    error!("Error polling Contacts.app: {}", e);
                }
            }
        });

        info!("Contacts channel adapter started");
        Ok(())
    }

    async fn send(&self, msg: OutgoingMessage) -> Result<()> {
        // Acknowledgments are silently ignored for Contacts
        if msg.kind == MessageKind::Acknowledgment {
            debug!("Skipping Contacts acknowledgment");
            return Ok(());
        }

        let (first_name, last_name, email, phone, note) = Self::parse_contact_fields(&msg.content);

        self.create_contact(&first_name, &last_name, &email, &phone, &note)
            .await
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Contacts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contacts_channel_creation() {
        let channel = ContactsChannel::new(Duration::from_secs(10), "Meepo".to_string());
        assert_eq!(channel.channel_type(), ChannelType::Contacts);
    }

    #[test]
    fn test_escape_applescript() {
        assert_eq!(
            ContactsChannel::escape_applescript("Hello \"world\""),
            "Hello \\\"world\\\""
        );
        assert_eq!(
            ContactsChannel::escape_applescript("line1\nline2"),
            "line1\\nline2"
        );
    }

    #[test]
    fn test_parse_contact_fields_structured() {
        let content = "FirstName: John\nLastName: Doe\nEmail: john@example.com\nPhone: +15551234567\nNote: Met at conference";
        let (first, last, email, phone, note) = ContactsChannel::parse_contact_fields(content);
        assert_eq!(first, "John");
        assert_eq!(last, "Doe");
        assert_eq!(email, "john@example.com");
        assert_eq!(phone, "+15551234567");
        assert_eq!(note, "Met at conference");
    }

    #[test]
    fn test_parse_contact_fields_plain_text() {
        let content = "Jane Smith\nSome notes about this person";
        let (first, last, _email, _phone, note) = ContactsChannel::parse_contact_fields(content);
        assert_eq!(first, "Jane");
        assert_eq!(last, "Smith");
        assert_eq!(note, "Some notes about this person");
    }

    #[test]
    fn test_parse_contact_fields_name_only() {
        let content = "Alice";
        let (first, last, _email, _phone, _note) = ContactsChannel::parse_contact_fields(content);
        assert_eq!(first, "Alice");
        assert_eq!(last, "");
    }

    #[tokio::test]
    async fn test_seen_ids_dedup() {
        let channel = ContactsChannel::new(Duration::from_secs(10), "Meepo".to_string());

        {
            let mut seen = channel.seen_ids.lock().await;
            seen.insert("contact_1".to_string());
        }

        {
            let seen = channel.seen_ids.lock().await;
            assert!(seen.contains("contact_1"));
            assert!(!seen.contains("contact_2"));
        }
    }
}
