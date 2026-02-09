# Email Channel & Folder Access Tools

**Date**: 2026-02-09
**Status**: Design approved

## Summary

Three features:
1. **Email Channel** — New `MessageChannel` that polls Mac Mail.app inbox, auto-processes emails with `[meepo]` subject prefix, replies with threading
2. **Enhanced Email Tools** — Improved `read_emails` and `send_email` with search, mailbox filtering, and reply threading
3. **Folder Access Tools** — New `list_directory` and `search_files` tools for browsing/searching ~/Coding

## 1. Email Channel

### Files
- `crates/meepo-channels/src/email.rs` — new `EmailChannel` struct
- `crates/meepo-channels/src/lib.rs` — add `pub mod email`
- `crates/meepo-core/src/types.rs` — add `Email` variant to `ChannelType`
- `crates/meepo-cli/src/config.rs` — add `EmailConfig` to channel configs
- `crates/meepo-cli/src/main.rs` — register `EmailChannel` in daemon startup
- `config/default.toml` — add `[channels.email]` section

### Config
```toml
[channels.email]
enabled = false
poll_interval_secs = 10
subject_prefix = "[meepo]"
```

### Behavior
- Polls Mail.app inbox via AppleScript every `poll_interval_secs`
- Only processes unread emails whose subject starts with `subject_prefix`
- Strips prefix from subject, combines with body as message content
- Marks processed emails as read to avoid re-processing
- Replies using Mail.app's `reply` command on the original message (preserves threading)
- Tracks last-seen date to skip old emails on startup

### AppleScript Strategy
- **Read**: Get unread messages from inbox, filter by subject prefix
- **Reply**: Use `reply` on the message object, set content, send — this preserves In-Reply-To/References headers automatically

## 2. Enhanced Email Tools

### Files
- `crates/meepo-core/src/tools/macos.rs` — modify `ReadEmailsTool` and `SendEmailTool`

### ReadEmailsTool Changes
- Add `mailbox` param (optional, default "inbox")
- Add `search` param (optional, filter by subject/sender keyword)
- Return body preview (first 500 chars) in addition to sender/subject/date

### SendEmailTool Changes
- Add `in_reply_to` param (optional, message subject to find and reply to)
- Add `cc` param (optional, CC recipients)
- When `in_reply_to` provided: use AppleScript `reply` on matching message

## 3. Folder Access Tools

### Files
- `crates/meepo-core/src/tools/filesystem.rs` — new module
- `crates/meepo-core/src/tools/mod.rs` — add `pub mod filesystem`
- `crates/meepo-cli/src/config.rs` — add `FilesystemConfig`
- `crates/meepo-cli/src/main.rs` — register filesystem tools
- `config/default.toml` — add `[filesystem]` section

### Config
```toml
[filesystem]
allowed_directories = ["~/Coding"]
```

### ListDirectoryTool
- **Name**: `list_directory`
- **Input**: `path` (required), `recursive` (optional, default false, max depth 3), `pattern` (optional glob)
- **Output**: File names, sizes, modification dates
- **Security**: Validates path is within `allowed_directories`

### SearchFilesTool
- **Name**: `search_files`
- **Input**: `path` (required), `query` (required), `file_pattern` (optional glob), `max_results` (default 20)
- **Output**: Matching file paths with line numbers and context
- **Security**: Same directory validation. Max 1000 files scanned, 30s timeout.

## Implementation Order

1. Add `Email` to `ChannelType` enum
2. Create `filesystem.rs` with `ListDirectoryTool` and `SearchFilesTool`
3. Create `email.rs` channel
4. Enhance `ReadEmailsTool` and `SendEmailTool`
5. Wire config + registration in CLI
6. Update `default.toml`
7. Tests
