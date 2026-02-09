# Email Channel & Folder Access Tools — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add email as a message channel (auto-trigger on `[meepo]` subject, reply with threading), enhance email tools, and add `list_directory`/`search_files` tools for ~/Coding access.

**Architecture:** Three parallel workstreams: (1) `ChannelType::Email` + `EmailChannel` in meepo-channels using AppleScript polling, (2) filesystem tools in meepo-core as new `ToolHandler` impls, (3) enhanced email tools in macos.rs. All wired through config and CLI registration.

**Tech Stack:** Rust, async_trait, tokio, AppleScript (osascript), `glob` crate for file pattern matching.

---

### Task 1: Add `Email` variant to `ChannelType`

**Files:**
- Modify: `crates/meepo-core/src/types.rs`

**Step 1: Add the Email variant and update Display/match arms**

In `crates/meepo-core/src/types.rs`, add `Email` to the `ChannelType` enum:

```rust
pub enum ChannelType {
    Discord,
    Slack,
    IMessage,
    Email,
    Internal,
}
```

And update the `Display` impl:

```rust
Self::Email => write!(f, "email"),
```

**Step 2: Verify compilation**

Run: `cargo check -p meepo-core`
Expected: PASS (no other code references Email yet)

**Step 3: Commit**

```bash
git add crates/meepo-core/src/types.rs
git commit -m "feat: add Email variant to ChannelType"
```

---

### Task 2: Add `glob` dependency to meepo-core

**Files:**
- Modify: `Cargo.toml` (workspace root) — add `glob` to workspace dependencies
- Modify: `crates/meepo-core/Cargo.toml` — add `glob` dependency

**Step 1: Add glob to workspace Cargo.toml**

Add under `[workspace.dependencies]`:
```toml
glob = "0.3"
```

**Step 2: Add glob to meepo-core Cargo.toml**

Add under `[dependencies]`:
```toml
glob = { workspace = true }
```

**Step 3: Verify compilation**

Run: `cargo check -p meepo-core`
Expected: PASS

**Step 4: Commit**

```bash
git add Cargo.toml crates/meepo-core/Cargo.toml
git commit -m "chore: add glob dependency for filesystem tools"
```

---

### Task 3: Create `ListDirectoryTool`

**Files:**
- Create: `crates/meepo-core/src/tools/filesystem.rs`
- Modify: `crates/meepo-core/src/tools/mod.rs`

**Step 1: Create filesystem.rs with ListDirectoryTool**

Create `crates/meepo-core/src/tools/filesystem.rs`:

```rust
//! Filesystem access tools for browsing and searching local directories

use async_trait::async_trait;
use serde_json::Value;
use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use tracing::debug;

use super::{ToolHandler, json_schema};

/// Validate that a path is within one of the allowed directories
fn validate_allowed_path(path: &str, allowed_dirs: &[PathBuf]) -> Result<PathBuf> {
    if path.contains("..") {
        return Err(anyhow::anyhow!("Path contains '..' which is not allowed"));
    }

    let expanded = shellexpand(path);
    let canonical = expanded.canonicalize()
        .with_context(|| format!("Path does not exist: {}", expanded.display()))?;

    for allowed in allowed_dirs {
        let allowed_canonical = allowed.canonicalize()
            .unwrap_or_else(|_| allowed.clone());
        if canonical.starts_with(&allowed_canonical) {
            return Ok(canonical);
        }
    }

    Err(anyhow::anyhow!(
        "Access denied: '{}' is not within allowed directories",
        canonical.display()
    ))
}

fn shellexpand(s: &str) -> PathBuf {
    let mut result = s.to_string();
    if result.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            result = format!("{}{}", home.display(), &result[1..]);
        }
    }
    PathBuf::from(result)
}

/// List directory contents
pub struct ListDirectoryTool {
    allowed_dirs: Vec<PathBuf>,
}

impl ListDirectoryTool {
    pub fn new(allowed_dirs: Vec<String>) -> Self {
        Self {
            allowed_dirs: allowed_dirs.iter().map(|d| shellexpand(d)).collect(),
        }
    }
}

#[async_trait]
impl ToolHandler for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories at a given path. Only accessible within configured allowed directories."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Directory path to list (supports ~/)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List recursively (default: false, max depth: 3)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Optional glob pattern to filter files (e.g. '*.rs', '*.py')"
                }
            }),
            vec!["path"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path_str = input.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let recursive = input.get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let pattern = input.get("pattern")
            .and_then(|v| v.as_str());

        let validated_path = validate_allowed_path(path_str, &self.allowed_dirs)?;
        debug!("Listing directory: {}", validated_path.display());

        let mut entries = Vec::new();
        list_dir_recursive(&validated_path, &validated_path, recursive, 0, 3, pattern, &mut entries)?;

        if entries.is_empty() {
            return Ok("Directory is empty or no files match the pattern.".to_string());
        }

        Ok(entries.join("\n"))
    }
}

fn list_dir_recursive(
    base: &Path,
    dir: &Path,
    recursive: bool,
    depth: usize,
    max_depth: usize,
    pattern: Option<&str>,
    entries: &mut Vec<String>,
) -> Result<()> {
    let mut dir_entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    dir_entries.sort_by_key(|e| e.file_name());

    for entry in dir_entries {
        let path = entry.path();
        let name = path.strip_prefix(base)
            .unwrap_or(&path)
            .display()
            .to_string();

        // Skip hidden files
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }

        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            entries.push(format!("{}/ (dir)", name));
            if recursive && depth < max_depth {
                list_dir_recursive(base, &path, recursive, depth + 1, max_depth, pattern, entries)?;
            }
        } else {
            // Check pattern if provided
            if let Some(pat) = pattern {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if !glob::Pattern::new(pat)
                    .map(|p| p.matches(&file_name))
                    .unwrap_or(false)
                {
                    continue;
                }
            }

            let size = metadata.len();
            let modified = metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            let size_str = if size < 1024 {
                format!("{} B", size)
            } else if size < 1024 * 1024 {
                format!("{:.1} KB", size as f64 / 1024.0)
            } else {
                format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
            };

            entries.push(format!("{} ({}, {})", name, size_str, modified));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_directory_tool_schema() {
        let tool = ListDirectoryTool::new(vec!["~/Coding".to_string()]);
        assert_eq!(tool.name(), "list_directory");
        assert!(!tool.description().is_empty());
        let schema = tool.input_schema();
        assert!(schema.get("properties").is_some());
    }

    #[tokio::test]
    async fn test_list_directory_allowed() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        // Create some test files
        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("world.txt"), "hello world").unwrap();
        std::fs::create_dir(temp.path().join("subdir")).unwrap();

        let tool = ListDirectoryTool::new(vec![temp_path.clone()]);
        let result = tool.execute(serde_json::json!({
            "path": temp_path
        })).await.unwrap();

        assert!(result.contains("hello.rs"));
        assert!(result.contains("world.txt"));
        assert!(result.contains("subdir/"));
    }

    #[tokio::test]
    async fn test_list_directory_pattern_filter() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("world.txt"), "hello world").unwrap();

        let tool = ListDirectoryTool::new(vec![temp_path.clone()]);
        let result = tool.execute(serde_json::json!({
            "path": temp_path,
            "pattern": "*.rs"
        })).await.unwrap();

        assert!(result.contains("hello.rs"));
        assert!(!result.contains("world.txt"));
    }

    #[tokio::test]
    async fn test_list_directory_denied() {
        let tool = ListDirectoryTool::new(vec!["~/Coding".to_string()]);
        let result = tool.execute(serde_json::json!({
            "path": "/etc"
        })).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_directory_path_traversal_blocked() {
        let tool = ListDirectoryTool::new(vec!["~/Coding".to_string()]);
        let result = tool.execute(serde_json::json!({
            "path": "~/Coding/../../etc"
        })).await;
        assert!(result.is_err());
    }
}
```

**Step 2: Register the module in mod.rs**

Add to `crates/meepo-core/src/tools/mod.rs`:
```rust
pub mod filesystem;
```

**Step 3: Verify compilation and run tests**

Run: `cargo test -p meepo-core -- filesystem`
Expected: Tests pass (allowed/denied/pattern/traversal)

**Step 4: Commit**

```bash
git add crates/meepo-core/src/tools/filesystem.rs crates/meepo-core/src/tools/mod.rs
git commit -m "feat: add ListDirectoryTool with security validation"
```

---

### Task 4: Add `SearchFilesTool` to filesystem.rs

**Files:**
- Modify: `crates/meepo-core/src/tools/filesystem.rs`

**Step 1: Add SearchFilesTool below ListDirectoryTool**

Append to `crates/meepo-core/src/tools/filesystem.rs` (before the `#[cfg(test)]` block):

```rust
/// Search file contents within a directory
pub struct SearchFilesTool {
    allowed_dirs: Vec<PathBuf>,
}

impl SearchFilesTool {
    pub fn new(allowed_dirs: Vec<String>) -> Self {
        Self {
            allowed_dirs: allowed_dirs.iter().map(|d| shellexpand(d)).collect(),
        }
    }
}

#[async_trait]
impl ToolHandler for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for text patterns within files in a directory. Returns matching lines with file paths, line numbers, and context."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Directory to search in (supports ~/)"
                },
                "query": {
                    "type": "string",
                    "description": "Text or pattern to search for (case-insensitive)"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Optional glob pattern to filter files (e.g. '*.rs', '*.py')"
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of matching lines to return (default: 20)"
                }
            }),
            vec!["path", "query"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path_str = input.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let query = input.get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let file_pattern = input.get("file_pattern")
            .and_then(|v| v.as_str());
        let max_results = input.get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(100) as usize;

        let validated_path = validate_allowed_path(path_str, &self.allowed_dirs)?;
        debug!("Searching files in {} for '{}'", validated_path.display(), query);

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        let mut files_scanned = 0usize;
        let max_files = 1000;

        search_dir_recursive(
            &validated_path,
            &validated_path,
            &query_lower,
            file_pattern,
            max_results,
            max_files,
            &mut files_scanned,
            &mut results,
        )?;

        if results.is_empty() {
            return Ok(format!("No matches found for '{}' in {} ({} files scanned)",
                query, validated_path.display(), files_scanned));
        }

        let header = format!("Found {} matches in {} ({} files scanned):\n",
            results.len(), validated_path.display(), files_scanned);
        Ok(format!("{}{}", header, results.join("\n")))
    }
}

fn search_dir_recursive(
    base: &Path,
    dir: &Path,
    query: &str,
    file_pattern: Option<&str>,
    max_results: usize,
    max_files: usize,
    files_scanned: &mut usize,
    results: &mut Vec<String>,
) -> Result<()> {
    if results.len() >= max_results || *files_scanned >= max_files {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if results.len() >= max_results || *files_scanned >= max_files {
            break;
        }

        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/dirs
        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Skip common large directories
            if matches!(name.as_str(), "node_modules" | "target" | ".git" | "build" | "dist" | "__pycache__" | ".venv" | "venv") {
                continue;
            }
            search_dir_recursive(base, &path, query, file_pattern, max_results, max_files, files_scanned, results)?;
        } else {
            // Check file pattern
            if let Some(pat) = file_pattern {
                if !glob::Pattern::new(pat)
                    .map(|p| p.matches(&name))
                    .unwrap_or(false)
                {
                    continue;
                }
            }

            // Skip binary files (check first 512 bytes)
            if let Ok(content) = std::fs::read(&path) {
                *files_scanned += 1;
                let check_len = content.len().min(512);
                if content[..check_len].contains(&0) {
                    continue; // Skip binary
                }

                if let Ok(text) = String::from_utf8(content) {
                    let rel_path = path.strip_prefix(base)
                        .unwrap_or(&path)
                        .display()
                        .to_string();

                    for (line_num, line) in text.lines().enumerate() {
                        if results.len() >= max_results {
                            break;
                        }
                        if line.to_lowercase().contains(query) {
                            results.push(format!("{}:{}: {}",
                                rel_path,
                                line_num + 1,
                                line.chars().take(200).collect::<String>()));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
```

**Step 2: Add tests for SearchFilesTool to the existing test module**

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn test_search_files_tool_schema() {
        let tool = SearchFilesTool::new(vec!["~/Coding".to_string()]);
        assert_eq!(tool.name(), "search_files");
    }

    #[tokio::test]
    async fn test_search_files_found() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {\n    println!(\"hello world\");\n}\n").unwrap();
        std::fs::write(temp.path().join("other.txt"), "nothing here").unwrap();

        let tool = SearchFilesTool::new(vec![temp_path.clone()]);
        let result = tool.execute(serde_json::json!({
            "path": temp_path,
            "query": "println"
        })).await.unwrap();

        assert!(result.contains("hello.rs:2"));
        assert!(result.contains("println"));
    }

    #[tokio::test]
    async fn test_search_files_pattern_filter() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("hello.py"), "def main(): pass").unwrap();

        let tool = SearchFilesTool::new(vec![temp_path.clone()]);
        let result = tool.execute(serde_json::json!({
            "path": temp_path,
            "query": "main",
            "file_pattern": "*.rs"
        })).await.unwrap();

        assert!(result.contains("hello.rs"));
        assert!(!result.contains("hello.py"));
    }

    #[tokio::test]
    async fn test_search_files_no_match() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();

        let tool = SearchFilesTool::new(vec![temp_path.clone()]);
        let result = tool.execute(serde_json::json!({
            "path": temp_path,
            "query": "nonexistent_xyz_pattern"
        })).await.unwrap();

        assert!(result.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_search_files_denied() {
        let tool = SearchFilesTool::new(vec!["~/Coding".to_string()]);
        let result = tool.execute(serde_json::json!({
            "path": "/etc",
            "query": "test"
        })).await;
        assert!(result.is_err());
    }
```

**Step 3: Run tests**

Run: `cargo test -p meepo-core -- filesystem`
Expected: All filesystem tests pass

**Step 4: Commit**

```bash
git add crates/meepo-core/src/tools/filesystem.rs
git commit -m "feat: add SearchFilesTool with content search and pattern filtering"
```

---

### Task 5: Create `EmailChannel`

**Files:**
- Create: `crates/meepo-channels/src/email.rs`
- Modify: `crates/meepo-channels/src/lib.rs`

**Step 1: Create email.rs**

Create `crates/meepo-channels/src/email.rs`:

```rust
//! Email channel adapter using Mail.app AppleScript polling

use crate::bus::MessageChannel;
use meepo_core::types::{IncomingMessage, OutgoingMessage, ChannelType};
use tokio::sync::mpsc;
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use std::time::Duration;
use std::num::NonZeroUsize;
use tracing::{info, error, debug, warn};
use chrono::Utc;
use tokio::process::Command;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use lru::LruCache;

const MAX_EMAIL_SENDERS: usize = 500;

/// Email channel adapter that polls Mail.app for incoming emails
pub struct EmailChannel {
    poll_interval: Duration,
    subject_prefix: String,
    /// Maps message_id -> sender email for reply routing
    message_senders: Arc<Mutex<LruCache<String, EmailMeta>>>,
    /// Counter to track last processed message index
    last_check_time: Arc<RwLock<Option<String>>>,
}

/// Metadata about an email for reply threading
struct EmailMeta {
    sender: String,
    subject: String,
}

impl EmailChannel {
    pub fn new(poll_interval: Duration, subject_prefix: String) -> Self {
        Self {
            poll_interval,
            subject_prefix,
            message_senders: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(MAX_EMAIL_SENDERS).unwrap(),
            ))),
            last_check_time: Arc::new(RwLock::new(None)),
        }
    }

    /// Escape a string for use in AppleScript
    fn escape_applescript(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    }

    /// Poll Mail.app for unread emails matching the subject prefix
    async fn poll_emails(&self, tx: &mpsc::Sender<IncomingMessage>) -> Result<()> {
        let prefix = Self::escape_applescript(&self.subject_prefix);

        let script = format!(r#"
tell application "Mail"
    try
        set output to ""
        set unreadMsgs to (every message of inbox whose read status is false and subject begins with "{prefix}")
        repeat with m in unreadMsgs
            set msgSubject to subject of m
            set msgSender to sender of m
            set msgDate to date received of m as string
            set msgId to id of m
            set msgBody to content of m
            -- Truncate body to 2000 chars
            if length of msgBody > 2000 then
                set msgBody to text 1 thru 2000 of msgBody
            end if
            set output to output & "<<MSG_START>>" & "\n"
            set output to output & "ID: " & msgId & "\n"
            set output to output & "From: " & msgSender & "\n"
            set output to output & "Subject: " & msgSubject & "\n"
            set output to output & "Date: " & msgDate & "\n"
            set output to output & "Body: " & msgBody & "\n"
            set output to output & "<<MSG_END>>" & "\n"
            -- Mark as read to avoid re-processing
            set read status of m to true
        end repeat
        return output
    on error errMsg
        return "ERROR: " & errMsg
    end try
end tell
"#);

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
        )
        .await
        .map_err(|_| anyhow!("Mail.app polling timed out"))?
        .map_err(|e| anyhow!("Failed to run osascript: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Mail.app poll failed: {}", stderr);
            return Ok(()); // Non-fatal, will retry next poll
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() || stdout.starts_with("ERROR:") {
            if stdout.starts_with("ERROR:") {
                warn!("Mail.app error: {}", stdout);
            }
            return Ok(());
        }

        // Parse message blocks
        for block in stdout.split("<<MSG_START>>") {
            let block = block.trim();
            if block.is_empty() || !block.contains("<<MSG_END>>") {
                continue;
            }

            let block = block.replace("<<MSG_END>>", "");
            let mut id = String::new();
            let mut sender = String::new();
            let mut subject = String::new();
            let mut body = String::new();

            for line in block.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("ID: ") {
                    id = val.to_string();
                } else if let Some(val) = line.strip_prefix("From: ") {
                    sender = val.to_string();
                } else if let Some(val) = line.strip_prefix("Subject: ") {
                    subject = val.to_string();
                } else if let Some(val) = line.strip_prefix("Body: ") {
                    body = val.to_string();
                }
            }

            if id.is_empty() || sender.is_empty() {
                continue;
            }

            // Strip subject prefix
            let stripped_subject = subject
                .strip_prefix(&self.subject_prefix)
                .unwrap_or(&subject)
                .trim()
                .to_string();

            // Combine subject and body as message content
            let content = if stripped_subject.is_empty() {
                body.clone()
            } else if body.is_empty() {
                stripped_subject.clone()
            } else {
                format!("{}\n\n{}", stripped_subject, body)
            };

            let msg_id = format!("email_{}", id);

            // Store metadata for reply routing
            {
                let mut lru = self.message_senders.lock().await;
                lru.put(msg_id.clone(), EmailMeta {
                    sender: sender.clone(),
                    subject: subject.clone(),
                });
            }

            let incoming = IncomingMessage {
                id: msg_id,
                sender: sender.clone(),
                content,
                channel: ChannelType::Email,
                timestamp: Utc::now(),
            };

            info!("New email from {}: {}", sender, stripped_subject);

            if let Err(e) = tx.send(incoming).await {
                error!("Failed to send email message to bus: {}", e);
            }
        }

        Ok(())
    }

    /// Reply to an email using Mail.app threading
    async fn reply_to_email(&self, original_subject: &str, sender: &str, reply_body: &str) -> Result<()> {
        let safe_subject = Self::escape_applescript(original_subject);
        let safe_body = Self::escape_applescript(reply_body);
        let safe_sender = Self::escape_applescript(sender);

        // Try to find and reply to the original message for threading
        let script = format!(r#"
tell application "Mail"
    try
        set targetMsgs to (every message of inbox whose subject is "{safe_subject}" and sender contains "{safe_sender}")
        if (count of targetMsgs) > 0 then
            set originalMsg to item 1 of targetMsgs
            set replyMsg to reply originalMsg with opening window
            set content of replyMsg to "{safe_body}"
            send replyMsg
            return "Reply sent (threaded)"
        else
            -- Fallback: send new email
            set newMsg to make new outgoing message with properties {{subject:"Re: {safe_subject}", content:"{safe_body}", visible:true}}
            tell newMsg
                make new to recipient at end of to recipients with properties {{address:"{safe_sender}"}}
                send
            end tell
            return "Reply sent (new message)"
        end if
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#);

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
        )
        .await
        .map_err(|_| anyhow!("Email reply timed out"))?
        .map_err(|e| anyhow!("Failed to run osascript: {}", e))?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout);
            info!("Email reply result: {}", result.trim());
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Failed to reply to email: {}", stderr))
        }
    }
}

#[async_trait]
impl MessageChannel for EmailChannel {
    async fn start(&self, tx: mpsc::Sender<IncomingMessage>) -> Result<()> {
        info!("Starting Email channel adapter");
        info!("Poll interval: {:?}", self.poll_interval);
        info!("Subject prefix: {}", self.subject_prefix);

        let poll_interval = self.poll_interval;
        let subject_prefix = self.subject_prefix.clone();
        let message_senders = self.message_senders.clone();
        let last_check_time = self.last_check_time.clone();

        let channel = EmailChannel {
            poll_interval,
            subject_prefix,
            message_senders,
            last_check_time,
        };

        tokio::spawn(async move {
            info!("Email polling task started");
            let mut interval = tokio::time::interval(channel.poll_interval);

            loop {
                interval.tick().await;
                debug!("Polling Mail.app for new emails");

                if let Err(e) = channel.poll_emails(&tx).await {
                    error!("Error polling emails: {}", e);
                }
            }
        });

        info!("Email channel adapter started");
        Ok(())
    }

    async fn send(&self, msg: OutgoingMessage) -> Result<()> {
        debug!("Sending email reply");

        // Look up original email metadata for threading
        if let Some(reply_to) = &msg.reply_to {
            let lru = self.message_senders.lock().await;
            if let Some(meta) = lru.peek(reply_to) {
                let subject = meta.subject.clone();
                let sender = meta.sender.clone();
                drop(lru);
                return self.reply_to_email(&subject, &sender, &msg.content).await;
            }
        }

        // Fallback: no reply_to context, can't send without a recipient
        warn!("Cannot send email without reply context (no reply_to or sender unknown)");
        Err(anyhow!("Cannot send email: no reply context available"))
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Email
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_channel_creation() {
        let channel = EmailChannel::new(
            Duration::from_secs(10),
            "[meepo]".to_string(),
        );
        assert_eq!(channel.channel_type(), ChannelType::Email);
    }

    #[test]
    fn test_escape_applescript() {
        assert_eq!(
            EmailChannel::escape_applescript("Hello \"world\""),
            "Hello \\\"world\\\""
        );
        assert_eq!(
            EmailChannel::escape_applescript("line1\nline2"),
            "line1\\nline2"
        );
    }

    #[tokio::test]
    async fn test_email_meta_tracking() {
        let channel = EmailChannel::new(
            Duration::from_secs(10),
            "[meepo]".to_string(),
        );

        // Store metadata
        {
            let mut lru = channel.message_senders.lock().await;
            lru.put("email_123".to_string(), EmailMeta {
                sender: "user@example.com".to_string(),
                subject: "[meepo] test subject".to_string(),
            });
        }

        // Verify lookup
        {
            let lru = channel.message_senders.lock().await;
            let meta = lru.peek("email_123").unwrap();
            assert_eq!(meta.sender, "user@example.com");
            assert_eq!(meta.subject, "[meepo] test subject");
        }
    }

    #[tokio::test]
    async fn test_send_without_context_fails() {
        let channel = EmailChannel::new(
            Duration::from_secs(10),
            "[meepo]".to_string(),
        );

        let msg = OutgoingMessage {
            content: "test reply".to_string(),
            channel: ChannelType::Email,
            reply_to: None,
        };

        let result = channel.send(msg).await;
        assert!(result.is_err());
    }
}
```

**Step 2: Register module in lib.rs**

Add to `crates/meepo-channels/src/lib.rs`:
```rust
pub mod email;
```
And add re-export:
```rust
pub use email::EmailChannel;
```

**Step 3: Run tests**

Run: `cargo test -p meepo-channels -- email`
Expected: All email channel tests pass

**Step 4: Commit**

```bash
git add crates/meepo-channels/src/email.rs crates/meepo-channels/src/lib.rs
git commit -m "feat: add EmailChannel with Mail.app polling and threaded replies"
```

---

### Task 6: Enhance `ReadEmailsTool` and `SendEmailTool`

**Files:**
- Modify: `crates/meepo-core/src/tools/macos.rs`

**Step 1: Enhance ReadEmailsTool with mailbox, search, and body preview**

Replace the `ReadEmailsTool` implementation in `crates/meepo-core/src/tools/macos.rs` with:

Update `input_schema()` to add `mailbox` and `search` params:
```rust
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
```

Update `execute()` to use the new parameters and include body preview:
```rust
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

    // Validate mailbox name to prevent injection
    let safe_mailbox = match mailbox.to_lowercase().as_str() {
        "inbox" => "inbox",
        "sent" => "sent mailbox",
        "drafts" => "drafts",
        "trash" => "trash",
        _ => "inbox",
    };

    let filter_clause = if let Some(term) = search {
        let safe_term = sanitize_applescript_string(term);
        format!(r#" whose (subject contains "{safe_term}" or sender contains "{safe_term}")"#)
    } else {
        String::new()
    };

    let script = format!(r#"
tell application "Mail"
    try
        set msgs to (messages 1 thru {limit} of {safe_mailbox}{filter_clause})
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
"#);

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
```

**Step 2: Enhance SendEmailTool with in_reply_to and cc**

Update `SendEmailTool` input_schema to add `in_reply_to` and `cc`:
```rust
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
```

Update `execute()` to handle reply threading and CC:
```rust
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

    let safe_to = sanitize_applescript_string(to);
    let safe_subject = sanitize_applescript_string(subject);
    let safe_body = sanitize_applescript_string(body);

    let script = if let Some(reply_subject) = in_reply_to {
        // Reply to existing email for threading
        let safe_reply_subject = sanitize_applescript_string(reply_subject);
        debug!("Replying to email with subject: {}", reply_subject);
        format!(r#"
tell application "Mail"
    try
        set targetMsgs to (every message of inbox whose subject contains "{safe_reply_subject}")
        if (count of targetMsgs) > 0 then
            set originalMsg to item 1 of targetMsgs
            set replyMsg to reply originalMsg with opening window
            set content of replyMsg to "{safe_body}"
            send replyMsg
            return "Reply sent (threaded)"
        else
            set newMessage to make new outgoing message with properties {{subject:"{safe_subject}", content:"{safe_body}", visible:true}}
            tell newMessage
                make new to recipient at end of to recipients with properties {{address:"{safe_to}"}}
                send
            end tell
            return "Email sent (no original found for threading)"
        end if
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#)
    } else {
        // New email
        debug!("Sending new email to: {}", to);
        let cc_block = if let Some(cc_addr) = cc {
            let safe_cc = sanitize_applescript_string(cc_addr);
            format!(r#"
                make new cc recipient at end of cc recipients with properties {{address:"{safe_cc}"}}"#)
        } else {
            String::new()
        };

        format!(r#"
tell application "Mail"
    try
        set newMessage to make new outgoing message with properties {{subject:"{safe_subject}", content:"{safe_body}", visible:true}}
        tell newMessage
            make new to recipient at end of to recipients with properties {{address:"{safe_to}"}}{cc_block}
            send
        end tell
        return "Email sent successfully"
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#)
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
```

**Step 3: Run tests**

Run: `cargo test -p meepo-core -- macos`
Expected: All macos tests pass (schema tests, param validation)

**Step 4: Commit**

```bash
git add crates/meepo-core/src/tools/macos.rs
git commit -m "feat: enhance ReadEmailsTool and SendEmailTool with search, mailbox, threading, and CC"
```

---

### Task 7: Wire config and registration

**Files:**
- Modify: `crates/meepo-cli/src/config.rs`
- Modify: `crates/meepo-cli/src/main.rs`
- Modify: `config/default.toml`

**Step 1: Add EmailConfig and FilesystemConfig to config.rs**

Add `EmailConfig` struct after `IMessageConfig`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_email_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_subject_prefix")]
    pub subject_prefix: String,
}

fn default_email_poll_interval() -> u64 {
    10
}

fn default_subject_prefix() -> String {
    "[meepo]".to_string()
}
```

Add `FilesystemConfig` after `MemoryConfig`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    #[serde(default = "default_allowed_directories")]
    pub allowed_directories: Vec<String>,
}

fn default_allowed_directories() -> Vec<String> {
    vec!["~/Coding".to_string()]
}
```

Add `email` to `ChannelsConfig`:
```rust
pub struct ChannelsConfig {
    pub discord: DiscordConfig,
    pub slack: SlackConfig,
    pub imessage: IMessageConfig,
    #[serde(default)]
    pub email: EmailConfig,
}
```

Add `filesystem` to `MeepoConfig`:
```rust
pub struct MeepoConfig {
    pub agent: AgentConfig,
    pub providers: ProvidersConfig,
    pub channels: ChannelsConfig,
    pub knowledge: KnowledgeConfig,
    pub watchers: WatchersConfig,
    pub code: CodeConfig,
    pub memory: MemoryConfig,
    #[serde(default = "default_orchestrator_config")]
    pub orchestrator: OrchestratorConfig,
    #[serde(default)]
    pub filesystem: FilesystemConfig,
}
```

Add `Default` impl for `EmailConfig` and `FilesystemConfig`:
```rust
impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            poll_interval_secs: default_email_poll_interval(),
            subject_prefix: default_subject_prefix(),
        }
    }
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            allowed_directories: default_allowed_directories(),
        }
    }
}
```

**Step 2: Register EmailChannel and filesystem tools in main.rs**

In `cmd_start()`, after the Slack channel registration block (~line 308), add:
```rust
// Register Email channel if enabled
if cfg.channels.email.enabled {
    let email = meepo_channels::email::EmailChannel::new(
        std::time::Duration::from_secs(cfg.channels.email.poll_interval_secs),
        cfg.channels.email.subject_prefix.clone(),
    );
    bus.register(Box::new(email));
    info!("Email channel registered");
}
```

In the tool registration section (~after line 196 `WriteFileTool`), add:
```rust
// Filesystem access tools
registry.register(Arc::new(meepo_core::tools::filesystem::ListDirectoryTool::new(
    cfg.filesystem.allowed_directories.clone(),
)));
registry.register(Arc::new(meepo_core::tools::filesystem::SearchFilesTool::new(
    cfg.filesystem.allowed_directories.clone(),
)));
```

**Step 3: Update default.toml**

Add email channel section after `[channels.imessage]` block:
```toml

# ── Email Channel ────────────────────────────────────────────────
# Talk to Meepo via email through Mail.app. Works with any email
# account configured in Mail.app (Gmail, iCloud, etc.).
#
# How it works:
#   - Meepo polls Mail.app for unread emails with the subject prefix
#   - Only emails whose subject starts with the prefix are processed
#   - Replies are sent as threaded replies to the original email
#
# Requirements:
#   - macOS Mail.app with at least one email account configured
#   - Automation permissions for your terminal app:
#     System Settings → Privacy & Security → Automation → Terminal → Mail
#
# Example: Send an email with subject "[meepo] What's on my calendar?"

[channels.email]
enabled = false                          # Set to true to enable
poll_interval_secs = 10                  # How often to check for new emails
subject_prefix = "[meepo]"              # Emails must have this subject prefix
```

Add filesystem section after `[memory]`:
```toml

# ── Filesystem Access ────────────────────────────────────────────
# Directories the agent can browse and search.
# The agent can list files, read contents, and search within these dirs.

[filesystem]
allowed_directories = ["~/Coding"]       # Directories the agent can access
```

**Step 4: Verify everything compiles**

Run: `cargo check`
Expected: PASS

**Step 5: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 6: Commit**

```bash
git add crates/meepo-cli/src/config.rs crates/meepo-cli/src/main.rs config/default.toml
git commit -m "feat: wire email channel and filesystem tools into config and CLI registration"
```

---

### Task 8: Final integration test

**Step 1: Build release binary**

Run: `cargo build -p meepo-cli --release`
Expected: Builds successfully

**Step 2: Reinitialize config**

Run: `rm ~/.meepo/config.toml && cargo run -p meepo-cli -- init`
Expected: New config created with email and filesystem sections

**Step 3: Verify tool count in startup**

Run: `cargo run -p meepo-cli -- config | grep -c enabled`
Expected: Shows email channel in config output

**Step 4: Commit if any fixes needed, then final commit**

```bash
git add -A
git commit -m "feat: email channel with Mail.app polling and folder access tools"
```
