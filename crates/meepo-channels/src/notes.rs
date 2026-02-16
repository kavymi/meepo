//! Apple Notes channel adapter using AppleScript polling

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

/// Apple Notes channel adapter that polls Notes.app for new notes
/// in a designated folder and creates notes from outgoing messages.
pub struct NotesChannel {
    poll_interval: Duration,
    folder_name: String,
    /// Tag prefix used to identify notes meant for Meepo (e.g. "#meepo")
    tag_prefix: String,
    /// Tracks note IDs we've already processed to avoid duplicates
    seen_ids: Arc<Mutex<HashSet<String>>>,
}

impl NotesChannel {
    pub fn new(poll_interval: Duration, folder_name: String, tag_prefix: String) -> Self {
        Self {
            poll_interval,
            folder_name,
            tag_prefix,
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

    /// Poll Notes.app for notes in the configured folder whose name starts with the tag prefix
    async fn poll_notes(&self, tx: &mpsc::Sender<IncomingMessage>) -> Result<()> {
        let folder = Self::escape_applescript(&self.folder_name);
        let prefix = Self::escape_applescript(&self.tag_prefix);

        let script = format!(
            r#"
tell application "Notes"
    try
        if not (exists folder "{folder}") then
            return ""
        end if
        set output to ""
        set targetFolder to folder "{folder}"
        set allNotes to every note of targetFolder
        repeat with n in allNotes
            set nName to name of n
            if nName starts with "{prefix}" then
                set nId to id of n
                set nBody to plaintext of n
                if length of nBody > 4000 then
                    set nBody to text 1 thru 4000 of nBody
                end if
                set output to output & "<<NOTE_START>>" & "\n"
                set output to output & "ID: " & nId & "\n"
                set output to output & "Name: " & nName & "\n"
                set output to output & "Body: " & nBody & "\n"
                set output to output & "<<NOTE_END>>" & "\n"
            end if
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
        .map_err(|_| anyhow!("Notes.app polling timed out"))?
        .map_err(|e| anyhow!("Failed to run osascript: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Notes.app poll failed: {}", stderr);
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() || stdout.starts_with("ERROR:") {
            if stdout.starts_with("ERROR:") {
                warn!("Notes.app error: {}", stdout);
            }
            return Ok(());
        }

        for block in stdout.split("<<NOTE_START>>") {
            let block = block.trim();
            if block.is_empty() || !block.contains("<<NOTE_END>>") {
                continue;
            }

            let block = block.replace("<<NOTE_END>>", "");
            let mut id = String::new();
            let mut name = String::new();
            let mut body = String::new();

            for line in block.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("ID: ") {
                    id = val.to_string();
                } else if let Some(val) = line.strip_prefix("Name: ") {
                    name = val.to_string();
                } else if let Some(val) = line.strip_prefix("Body: ") {
                    body = val.to_string();
                }
            }

            if id.is_empty() || name.is_empty() {
                continue;
            }

            // Skip already-seen notes
            {
                let mut seen = self.seen_ids.lock().await;
                if seen.contains(&id) {
                    continue;
                }
                seen.insert(id.clone());
            }

            // Strip the tag prefix from the name for cleaner content
            let stripped_name = name
                .strip_prefix(&self.tag_prefix)
                .unwrap_or(&name)
                .trim()
                .to_string();

            let content = if stripped_name.is_empty() {
                body.clone()
            } else if body.is_empty() {
                stripped_name.clone()
            } else {
                format!("{}\n\n{}", stripped_name, body)
            };

            let msg_id = format!("note_{}", id);

            let incoming = IncomingMessage {
                id: msg_id,
                sender: "Notes.app".to_string(),
                content,
                channel: ChannelType::Notes,
                timestamp: Utc::now(),
            };

            info!("New note from Notes.app: {}", name);

            if let Err(e) = tx.send(incoming).await {
                error!("Failed to send note message to bus: {}", e);
            }

            // Rename the note to remove the tag prefix so it isn't picked up again
            let rename_script = format!(
                r#"
tell application "Notes"
    try
        set targetFolder to folder "{folder}"
        set targetNotes to (every note of targetFolder whose id is "{id}")
        repeat with n in targetNotes
            set oldName to name of n
            if oldName starts with "{prefix}" then
                set newName to text {prefix_len} thru -1 of oldName
                set name of n to newName
            end if
        end repeat
    end try
end tell
"#,
                folder = Self::escape_applescript(&self.folder_name),
                id = Self::escape_applescript(&id),
                prefix = Self::escape_applescript(&self.tag_prefix),
                prefix_len = self.tag_prefix.len() + 1,
            );

            if let Err(e) = Command::new("osascript")
                .arg("-e")
                .arg(&rename_script)
                .output()
                .await
            {
                warn!("Failed to rename processed note: {}", e);
            }
        }

        Ok(())
    }

    /// Create a new note in Notes.app
    async fn create_note(&self, name: &str, body: &str) -> Result<()> {
        let safe_folder = Self::escape_applescript(&self.folder_name);
        let safe_name = Self::escape_applescript(name);
        let safe_body = Self::escape_applescript(body);

        let script = format!(
            r#"
tell application "Notes"
    try
        if not (exists folder "{safe_folder}") then
            make new folder with properties {{name:"{safe_folder}"}}
        end if
        tell folder "{safe_folder}"
            make new note with properties {{name:"{safe_name}", body:"{safe_body}"}}
        end tell
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
        .map_err(|_| anyhow!("Notes create timed out"))?
        .map_err(|e| anyhow!("Failed to run osascript: {}", e))?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout);
            if result.trim().starts_with("ERROR:") {
                return Err(anyhow!("Notes.app error: {}", result.trim()));
            }
            info!("Note created: {}", safe_name);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Failed to create note: {}", stderr))
        }
    }
}

#[async_trait]
impl MessageChannel for NotesChannel {
    async fn start(&self, tx: mpsc::Sender<IncomingMessage>) -> Result<()> {
        info!("Starting Notes channel adapter");
        info!("Poll interval: {:?}", self.poll_interval);
        info!("Notes folder: {}", self.folder_name);
        info!("Tag prefix: {}", self.tag_prefix);

        let poll_interval = self.poll_interval;
        let folder_name = self.folder_name.clone();
        let tag_prefix = self.tag_prefix.clone();
        let seen_ids = self.seen_ids.clone();

        let channel = NotesChannel {
            poll_interval,
            folder_name,
            tag_prefix,
            seen_ids,
        };

        tokio::spawn(async move {
            info!("Notes polling task started");
            let mut interval = tokio::time::interval(channel.poll_interval);

            loop {
                interval.tick().await;
                debug!("Polling Notes.app for new notes");

                if let Err(e) = channel.poll_notes(&tx).await {
                    error!("Error polling Notes.app: {}", e);
                }
            }
        });

        info!("Notes channel adapter started");
        Ok(())
    }

    async fn send(&self, msg: OutgoingMessage) -> Result<()> {
        // Acknowledgments are silently ignored for Notes
        if msg.kind == MessageKind::Acknowledgment {
            debug!("Skipping Notes acknowledgment");
            return Ok(());
        }

        // Extract a title from the first line of content, rest becomes body
        let (title, body) = match msg.content.split_once('\n') {
            Some((first, rest)) => (first.trim().to_string(), rest.trim().to_string()),
            None => (msg.content.clone(), String::new()),
        };

        self.create_note(&title, &body).await
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notes_channel_creation() {
        let channel = NotesChannel::new(
            Duration::from_secs(10),
            "Meepo".to_string(),
            "#meepo".to_string(),
        );
        assert_eq!(channel.channel_type(), ChannelType::Notes);
    }

    #[test]
    fn test_escape_applescript() {
        assert_eq!(
            NotesChannel::escape_applescript("Hello \"world\""),
            "Hello \\\"world\\\""
        );
        assert_eq!(
            NotesChannel::escape_applescript("line1\nline2"),
            "line1\\nline2"
        );
    }

    #[tokio::test]
    async fn test_seen_ids_dedup() {
        let channel = NotesChannel::new(
            Duration::from_secs(10),
            "Meepo".to_string(),
            "#meepo".to_string(),
        );

        {
            let mut seen = channel.seen_ids.lock().await;
            seen.insert("note_1".to_string());
        }

        {
            let seen = channel.seen_ids.lock().await;
            assert!(seen.contains("note_1"));
            assert!(!seen.contains("note_2"));
        }
    }
}
