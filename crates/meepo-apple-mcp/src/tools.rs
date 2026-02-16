//! MCP tool definitions for Apple apps via AppleScript
//!
//! Each tool has a name, description, JSON Schema for input, and an async execute function.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use crate::applescript::{ensure_app_running, run_applescript, sanitize};

/// MCP tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Build the list of all available Apple MCP tools
pub fn all_tools() -> Vec<ToolDef> {
    vec![
        // ── Mail ──
        ToolDef {
            name: "read_emails".into(),
            description:
                "Read recent emails from Mail.app. Returns sender, subject, date, and preview."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "number",
                        "description": "Number of emails to retrieve (default: 10, max: 50)"
                    },
                    "mailbox": {
                        "type": "string",
                        "description": "Mailbox to read from: inbox, sent, drafts, trash (default: inbox)"
                    },
                    "search": {
                        "type": "string",
                        "description": "Optional search term to filter by subject or sender"
                    }
                }
            }),
        },
        ToolDef {
            name: "send_email".into(),
            description: "Send an email via Mail.app.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": { "type": "string", "description": "Recipient email address" },
                    "subject": { "type": "string", "description": "Email subject" },
                    "body": { "type": "string", "description": "Email body text" },
                    "cc": { "type": "string", "description": "Optional CC address" }
                },
                "required": ["to", "subject", "body"]
            }),
        },
        // ── Calendar ──
        ToolDef {
            name: "read_calendar".into(),
            description: "Read upcoming calendar events from Calendar.app.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "days_ahead": {
                        "type": "number",
                        "description": "Number of days ahead to look (default: 7, max: 30)"
                    }
                }
            }),
        },
        ToolDef {
            name: "create_event".into(),
            description: "Create a calendar event in Calendar.app.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "Event title" },
                    "start_time": { "type": "string", "description": "Start time (e.g. 'January 15, 2026 at 2:00 PM')" },
                    "duration_minutes": { "type": "number", "description": "Duration in minutes (default: 60)" }
                },
                "required": ["summary", "start_time"]
            }),
        },
        // ── Contacts ──
        ToolDef {
            name: "search_contacts".into(),
            description: "Search contacts in Contacts.app by name, email, or phone.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" }
                },
                "required": ["query"]
            }),
        },
        // ── Reminders ──
        ToolDef {
            name: "list_reminders".into(),
            description: "List reminders from Reminders.app.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "list_name": {
                        "type": "string",
                        "description": "Optional reminder list name to filter by"
                    }
                }
            }),
        },
        ToolDef {
            name: "create_reminder".into(),
            description: "Create a reminder in Reminders.app.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Reminder text" },
                    "list_name": { "type": "string", "description": "Optional list name" },
                    "due_date": { "type": "string", "description": "Optional due date (e.g. 'January 15, 2026 at 2:00 PM')" },
                    "notes": { "type": "string", "description": "Optional notes" }
                },
                "required": ["name"]
            }),
        },
        // ── Notes ──
        ToolDef {
            name: "list_notes".into(),
            description: "List notes from Notes.app.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "folder": { "type": "string", "description": "Optional folder name" },
                    "limit": { "type": "number", "description": "Max notes to return (default: 20)" }
                }
            }),
        },
        ToolDef {
            name: "create_note".into(),
            description: "Create a note in Notes.app.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Note title" },
                    "body": { "type": "string", "description": "Note body text" },
                    "folder": { "type": "string", "description": "Optional folder name" }
                },
                "required": ["title", "body"]
            }),
        },
        // ── Music ──
        ToolDef {
            name: "get_current_track".into(),
            description: "Get the currently playing track from Apple Music.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "control_playback".into(),
            description: "Control Apple Music playback (play, pause, next, previous).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Playback action: play, pause, next, previous, toggle"
                    }
                },
                "required": ["action"]
            }),
        },
    ]
}

/// Execute a tool by name with the given arguments
pub async fn execute(name: &str, args: Value) -> Result<String> {
    match name {
        "read_emails" => exec_read_emails(args).await,
        "send_email" => exec_send_email(args).await,
        "read_calendar" => exec_read_calendar(args).await,
        "create_event" => exec_create_event(args).await,
        "search_contacts" => exec_search_contacts(args).await,
        "list_reminders" => exec_list_reminders(args).await,
        "create_reminder" => exec_create_reminder(args).await,
        "list_notes" => exec_list_notes(args).await,
        "create_note" => exec_create_note(args).await,
        "get_current_track" => exec_get_current_track().await,
        "control_playback" => exec_control_playback(args).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}

// ── Mail ────────────────────────────────────────────────────────

async fn exec_read_emails(args: Value) -> Result<String> {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .min(50);
    let mailbox = args
        .get("mailbox")
        .and_then(|v| v.as_str())
        .unwrap_or("inbox");
    let search = args.get("search").and_then(|v| v.as_str());

    let safe_mailbox = match mailbox.to_lowercase().as_str() {
        "inbox" => "inbox",
        "sent" => "sent mailbox",
        "drafts" => "drafts",
        "trash" => "trash",
        _ => "inbox",
    };

    let filter_clause = if let Some(term) = search {
        let safe_term = sanitize(term);
        format!(
            r#" whose (subject contains "{}" or sender contains "{}")"#,
            safe_term, safe_term
        )
    } else {
        String::new()
    };

    debug!("Reading {} emails from Mail.app ({})", limit, mailbox);
    ensure_app_running("Mail").await?;

    let script = format!(
        r#"
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
"#,
        limit, safe_mailbox, filter_clause
    );
    run_applescript(&script, 60, 2).await
}

async fn exec_send_email(args: Value) -> Result<String> {
    let to = args
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'to'"))?;
    let subject = args
        .get("subject")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'subject'"))?;
    let body = args
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'body'"))?;
    let cc = args.get("cc").and_then(|v| v.as_str());

    let safe_subject = sanitize(subject);
    let safe_body = sanitize(body);
    let safe_to = sanitize(to);

    let cc_block = if let Some(cc_addr) = cc {
        let safe_cc = sanitize(cc_addr);
        format!(
            r#"
                make new to recipient at end of cc recipients with properties {{address:"{safe_cc}"}}"#
        )
    } else {
        String::new()
    };

    let script = format!(
        r#"
tell application "Mail"
    try
        set newMsg to make new outgoing message with properties {{subject:"{safe_subject}", content:"{safe_body}", visible:true}}
        tell newMsg
            make new to recipient at end of to recipients with properties {{address:"{safe_to}"}}{cc_block}
            send
        end tell
        return "Email sent to {safe_to}"
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#
    );
    run_applescript(&script, 30, 1).await
}

// ── Calendar ────────────────────────────────────────────────────

async fn exec_read_calendar(args: Value) -> Result<String> {
    let days_ahead = args
        .get("days_ahead")
        .and_then(|v| v.as_u64())
        .unwrap_or(7)
        .min(30);

    debug!("Reading calendar events for next {} days", days_ahead);
    let script = format!(
        r#"
tell application "Calendar"
    try
        set startDate to current date
        set endDate to (current date) + ({} * days)
        set output to ""
        repeat with cal in calendars
            set calName to name of cal
            set theEvents to (every event of cal whose start date is greater than or equal to startDate and start date is less than or equal to endDate)
            repeat with evt in theEvents
                set output to output & "Calendar: " & calName & "\n"
                set output to output & "Event: " & (summary of evt) & "\n"
                set output to output & "Start: " & (start date of evt as string) & "\n"
                set output to output & "End: " & (end date of evt as string) & "\n"
                set output to output & "---\n"
            end repeat
        end repeat
        return output
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#,
        days_ahead
    );
    run_applescript(&script, 60, 2).await
}

async fn exec_create_event(args: Value) -> Result<String> {
    let summary = args
        .get("summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'summary'"))?;
    let start_time = args
        .get("start_time")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'start_time'"))?;
    let duration = args
        .get("duration_minutes")
        .and_then(|v| v.as_u64())
        .unwrap_or(60);

    let safe_summary = sanitize(summary);
    let safe_start = sanitize(start_time);

    let script = format!(
        r#"
tell application "Calendar"
    try
        set startDate to date "{safe_start}"
        set endDate to startDate + ({duration} * minutes)
        set targetCal to first calendar
        tell targetCal
            make new event with properties {{summary:"{safe_summary}", start date:startDate, end date:endDate}}
        end tell
        return "Event created successfully in calendar: " & (name of targetCal)
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#
    );
    run_applescript(&script, 30, 1).await
}

// ── Contacts ────────────────────────────────────────────────────

async fn exec_search_contacts(args: Value) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'query'"))?;
    let safe_query = sanitize(query);

    debug!("Searching contacts for: {}", query);
    let script = format!(
        r#"
tell application "Contacts"
    try
        set matchingPeople to (every person whose name contains "{safe_query}" or value of emails contains "{safe_query}")
        set output to ""
        repeat with p in matchingPeople
            set output to output & "Name: " & (name of p) & "\n"
            try
                set output to output & "Email: " & (value of first email of p) & "\n"
            end try
            try
                set output to output & "Phone: " & (value of first phone of p) & "\n"
            end try
            set output to output & "---\n"
        end repeat
        return output
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#
    );
    run_applescript(&script, 30, 1).await
}

// ── Reminders ───────────────────────────────────────────────────

async fn exec_list_reminders(args: Value) -> Result<String> {
    let list_name = args.get("list_name").and_then(|v| v.as_str());

    let list_clause = if let Some(name) = list_name {
        let safe = sanitize(name);
        format!(r#"of list "{safe}""#)
    } else {
        String::new()
    };

    let script = format!(
        r#"
tell application "Reminders"
    try
        set rems to (every reminder {list_clause} whose completed is false)
        set output to ""
        repeat with r in rems
            set output to output & "Name: " & (name of r) & "\n"
            try
                set output to output & "Due: " & (due date of r as string) & "\n"
            end try
            try
                set output to output & "Notes: " & (body of r) & "\n"
            end try
            set output to output & "---\n"
        end repeat
        return output
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#
    );
    run_applescript(&script, 30, 1).await
}

async fn exec_create_reminder(args: Value) -> Result<String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'name'"))?;
    let list_name = args.get("list_name").and_then(|v| v.as_str());
    let due_date = args.get("due_date").and_then(|v| v.as_str());
    let notes = args.get("notes").and_then(|v| v.as_str());

    let safe_name = sanitize(name);

    let list_clause = if let Some(ln) = list_name {
        let safe = sanitize(ln);
        format!(r#" of list "{safe}""#)
    } else {
        String::new()
    };

    let mut props = format!(r#"name:"{safe_name}""#);
    if let Some(n) = notes {
        let safe = sanitize(n);
        props.push_str(&format!(r#", body:"{safe}""#));
    }

    let due_clause = if let Some(d) = due_date {
        let safe = sanitize(d);
        format!(
            r#"
            set due date of newReminder to date "{safe}""#
        )
    } else {
        String::new()
    };

    let script = format!(
        r#"
tell application "Reminders"
    try
        set newReminder to make new reminder{list_clause} with properties {{{props}}}{due_clause}
        return "Reminder created: {safe_name}"
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#
    );
    run_applescript(&script, 30, 1).await
}

// ── Notes ───────────────────────────────────────────────────────

async fn exec_list_notes(args: Value) -> Result<String> {
    let folder = args.get("folder").and_then(|v| v.as_str());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .min(100);

    let folder_clause = if let Some(f) = folder {
        let safe = sanitize(f);
        format!(r#" of folder "{safe}""#)
    } else {
        String::new()
    };

    let script = format!(
        r#"
tell application "Notes"
    try
        set allNotes to notes{folder_clause}
        set output to ""
        set noteCount to 0
        repeat with n in allNotes
            if noteCount ≥ {limit} then exit repeat
            set output to output & "Title: " & (name of n) & "\n"
            set output to output & "Date: " & (modification date of n as string) & "\n"
            set noteBody to plaintext of n
            if length of noteBody > 200 then
                set noteBody to text 1 thru 200 of noteBody
            end if
            set output to output & "Preview: " & noteBody & "\n"
            set output to output & "---\n"
            set noteCount to noteCount + 1
        end repeat
        return output
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#
    );
    run_applescript(&script, 30, 1).await
}

async fn exec_create_note(args: Value) -> Result<String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'title'"))?;
    let body = args
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'body'"))?;
    let folder = args.get("folder").and_then(|v| v.as_str());

    let safe_title = sanitize(title);
    let html_body = sanitize(body).replace('\n', "<br>");

    let folder_clause = if let Some(f) = folder {
        let safe = sanitize(f);
        format!(r#" in folder "{safe}""#)
    } else {
        String::new()
    };

    let script = format!(
        r#"
tell application "Notes"
    try
        set newNote to make new note{folder_clause} with properties {{name:"{safe_title}", body:"<h1>{safe_title}</h1><p>{html_body}</p>"}}
        return "Note created: {safe_title}"
    on error errMsg
        return "Error: " & errMsg
    end try
end tell
"#
    );
    run_applescript(&script, 30, 1).await
}

// ── Music ───────────────────────────────────────────────────────

async fn exec_get_current_track() -> Result<String> {
    debug!("Getting current track");
    let script = r#"
tell application "Music"
    if player state is playing then
        set trackName to name of current track
        set trackArtist to artist of current track
        set trackAlbum to album of current track
        set trackDuration to duration of current track
        set trackPosition to player position
        return "Track: " & trackName & "\nArtist: " & trackArtist & "\nAlbum: " & trackAlbum & "\nDuration: " & trackDuration & "s\nPosition: " & trackPosition & "s\nState: playing"
    else if player state is paused then
        set trackName to name of current track
        set trackArtist to artist of current track
        return "Track: " & trackName & "\nArtist: " & trackArtist & "\nState: paused"
    else
        return "State: stopped"
    end if
end tell
"#;
    run_applescript(script, 15, 0).await
}

async fn exec_control_playback(args: Value) -> Result<String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'action'"))?;

    let script = match action.to_lowercase().as_str() {
        "play" => r#"tell application "Music" to play"#.to_string(),
        "pause" => r#"tell application "Music" to pause"#.to_string(),
        "next" => r#"tell application "Music" to next track"#.to_string(),
        "previous" => r#"tell application "Music" to previous track"#.to_string(),
        "toggle" => r#"tell application "Music" to playpause"#.to_string(),
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown action: {}. Use: play, pause, next, previous, toggle",
                action
            ));
        }
    };

    run_applescript(&script, 10, 0).await?;
    Ok(format!("Playback: {}", action))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tools_not_empty() {
        let tools = all_tools();
        assert!(!tools.is_empty());
    }

    #[test]
    fn test_all_tools_have_names() {
        for tool in all_tools() {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert_eq!(tool.input_schema["type"], "object");
        }
    }

    #[test]
    fn test_tool_names_unique() {
        let tools = all_tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), original_len, "Duplicate tool names found");
    }

    #[test]
    fn test_all_tools_serializable() {
        let tools = all_tools();
        let json = serde_json::to_value(&tools).unwrap();
        assert!(json.is_array());
    }

    #[test]
    fn test_required_fields_present() {
        let tools = all_tools();
        let send_email = tools.iter().find(|t| t.name == "send_email").unwrap();
        let required = send_email.input_schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "to"));
        assert!(required.iter().any(|v| v == "subject"));
        assert!(required.iter().any(|v| v == "body"));
    }
}
