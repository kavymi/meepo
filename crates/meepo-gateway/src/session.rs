//! Session management — each conversation gets its own session
//!
//! Includes session path hardening (OpenClaw #15565/#15410/#15140),
//! key normalization (OpenClaw #12846), credential redaction (OpenClaw #13073),
//! and agent-to-agent session tools (sessions_list, sessions_history,
//! sessions_send, sessions_spawn).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Maximum session ID length
const MAX_SESSION_ID_LEN: usize = 128;

/// Maximum session name length
const MAX_SESSION_NAME_LEN: usize = 256;

/// Maximum number of concurrent sessions
const MAX_SESSIONS: usize = 1000;

/// Maximum messages stored in-memory per session
const MAX_HISTORY_PER_SESSION: usize = 500;

/// Session kind — categorizes how the session was created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Main,
    Group,
    Cron,
    Hook,
    Node,
    Subagent,
    Other,
}

impl std::fmt::Display for SessionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Main => write!(f, "main"),
            Self::Group => write!(f, "group"),
            Self::Cron => write!(f, "cron"),
            Self::Hook => write!(f, "hook"),
            Self::Node => write!(f, "node"),
            Self::Subagent => write!(f, "subagent"),
            Self::Other => write!(f, "other"),
        }
    }
}

/// How a message entered the session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageProvenance {
    /// Normal user input from a channel
    User,
    /// Agent (assistant) response
    Assistant,
    /// Tool result
    ToolResult,
    /// Injected by another agent via sessions_send
    InterSession { from_session: String },
    /// Injected by sessions_spawn parent
    SubagentTask { parent_session: String },
}

/// A message stored in session history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub provenance: MessageProvenance,
}

/// Visibility scope for session tools
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SessionVisibility {
    /// Only the current session
    Own,
    /// Current session + sessions spawned by it
    #[default]
    Tree,
    /// Any session belonging to the same agent id
    Agent,
    /// All sessions (cross-agent; requires agent_to_agent enabled)
    All,
}

/// A single chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub kind: SessionKind,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub message_count: u64,
    #[serde(default)]
    pub parent_session: Option<String>,
    #[serde(skip_serializing)]
    pub messages: Vec<SessionMessage>,
}

/// Manages all active sessions
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

/// Normalize a session key: lowercase, trim whitespace, reject path traversal
fn normalize_session_key(key: &str) -> Result<String, &'static str> {
    let normalized = key.trim().to_lowercase();

    if normalized.is_empty() {
        return Err("Session ID cannot be empty");
    }
    if normalized.len() > MAX_SESSION_ID_LEN {
        return Err("Session ID too long");
    }
    // Path hardening: reject path separators and traversal patterns
    if normalized.contains('/')
        || normalized.contains('\\')
        || normalized.contains("..")
        || normalized.contains('\0')
    {
        return Err("Session ID contains invalid characters");
    }
    // Reject control characters
    if normalized.chars().any(|c| c.is_control()) {
        return Err("Session ID contains control characters");
    }

    Ok(normalized)
}

/// Redact credentials/secrets from text content.
/// Matches common patterns: API keys, tokens, passwords, bearer tokens.
pub fn redact_credentials(text: &str) -> String {
    use std::borrow::Cow;

    let patterns: &[(&str, &str)] = &[
        // API key patterns (sk-..., key-..., etc.)
        ("sk-[a-zA-Z0-9]{20,}", "[REDACTED_API_KEY]"),
        ("key-[a-zA-Z0-9]{20,}", "[REDACTED_API_KEY]"),
        // Bearer tokens in text
        ("Bearer [a-zA-Z0-9._\\-]{20,}", "Bearer [REDACTED]"),
        // Generic long hex/base64 tokens (40+ chars)
        ("[a-fA-F0-9]{40,}", "[REDACTED_TOKEN]"),
    ];

    let mut result: Cow<'_, str> = Cow::Borrowed(text);

    for (pattern, replacement) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            let replaced = re.replace_all(&result, *replacement);
            if let Cow::Owned(s) = replaced {
                result = Cow::Owned(s);
            }
        }
    }

    result.into_owned()
}

impl SessionManager {
    /// Create a new session manager with a default "main" session
    pub fn new() -> Self {
        Self::with_agent("main")
    }

    /// Create a new session manager with a default "main" session for a specific agent
    pub fn with_agent(agent_id: &str) -> Self {
        let mut sessions = HashMap::new();
        let now = Utc::now();
        sessions.insert(
            "main".to_string(),
            Session {
                id: "main".to_string(),
                name: "Main".to_string(),
                agent_id: agent_id.to_string(),
                kind: SessionKind::Main,
                created_at: now,
                last_activity: now,
                message_count: 0,
                parent_session: None,
                messages: Vec::new(),
            },
        );
        Self {
            sessions: Arc::new(RwLock::new(sessions)),
        }
    }

    /// List all sessions (sorted by last activity, newest first)
    pub async fn list(&self) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<Session> = sessions.values().cloned().collect();
        list.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        list
    }

    /// List sessions filtered by agent ID
    pub async fn list_for_agent(&self, agent_id: &str) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<Session> = sessions
            .values()
            .filter(|s| s.agent_id == agent_id)
            .cloned()
            .collect();
        list.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        list
    }

    /// List sessions filtered by kind
    pub async fn list_by_kind(&self, kinds: &[SessionKind]) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<Session> = sessions
            .values()
            .filter(|s| kinds.contains(&s.kind))
            .cloned()
            .collect();
        list.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        list
    }

    /// List child sessions (spawned by a parent session)
    pub async fn list_children(&self, parent_id: &str) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<Session> = sessions
            .values()
            .filter(|s| s.parent_session.as_deref() == Some(parent_id))
            .cloned()
            .collect();
        list.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        list
    }

    /// Get a session by ID (with key normalization)
    pub async fn get(&self, id: &str) -> Option<Session> {
        let normalized = match normalize_session_key(id) {
            Ok(k) => k,
            Err(e) => {
                warn!("Invalid session ID '{}': {}", id, e);
                return None;
            }
        };
        let sessions = self.sessions.read().await;
        sessions.get(&normalized).cloned()
    }

    /// Create a new session with validation, returns the session
    pub async fn create(&self, name: &str) -> Result<Session, &'static str> {
        self.create_with_kind(name, "main", SessionKind::Other, None)
            .await
    }

    /// Create a new session with a specific kind and agent
    pub async fn create_with_kind(
        &self,
        name: &str,
        agent_id: &str,
        kind: SessionKind,
        parent_session: Option<String>,
    ) -> Result<Session, &'static str> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err("Session name cannot be empty");
        }
        if trimmed_name.len() > MAX_SESSION_NAME_LEN {
            return Err("Session name too long");
        }

        let mut sessions = self.sessions.write().await;

        if sessions.len() >= MAX_SESSIONS {
            return Err("Maximum number of sessions reached");
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let session = Session {
            id: id.clone(),
            name: trimmed_name.to_string(),
            agent_id: agent_id.to_string(),
            kind,
            created_at: now,
            last_activity: now,
            message_count: 0,
            parent_session,
            messages: Vec::new(),
        };
        sessions.insert(id.clone(), session.clone());
        info!(
            "Created session '{}' ({}) for agent '{}'",
            trimmed_name, id, agent_id
        );
        Ok(session)
    }

    /// Create a sub-agent session (for sessions_spawn)
    pub async fn create_subagent(
        &self,
        agent_id: &str,
        parent_session_id: &str,
        label: Option<&str>,
    ) -> Result<Session, &'static str> {
        let name = label.unwrap_or("subagent");
        self.create_with_kind(
            name,
            agent_id,
            SessionKind::Subagent,
            Some(parent_session_id.to_string()),
        )
        .await
    }

    /// Delete a session by ID (cannot delete "main")
    pub async fn delete(&self, id: &str) -> Result<(), &'static str> {
        let normalized = normalize_session_key(id).map_err(|_| "Invalid session ID")?;
        if normalized == "main" {
            return Err("Cannot delete the main session");
        }
        let mut sessions = self.sessions.write().await;
        if sessions.remove(&normalized).is_some() {
            info!("Deleted session '{}'", normalized);
            Ok(())
        } else {
            Err("Session not found")
        }
    }

    /// Record activity on a session (updates last_activity and message_count)
    pub async fn record_activity(&self, session_id: &str) {
        let normalized = match normalize_session_key(session_id) {
            Ok(k) => k,
            Err(_) => return,
        };
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&normalized) {
            session.last_activity = Utc::now();
            session.message_count += 1;
            debug!(
                "Session '{}' activity (messages: {})",
                normalized, session.message_count
            );
        }
    }

    /// Append a message to a session's history
    pub async fn append_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        provenance: MessageProvenance,
    ) -> Result<(), &'static str> {
        let normalized = normalize_session_key(session_id).map_err(|_| "Invalid session ID")?;
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(&normalized).ok_or("Session not found")?;

        let msg = SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            provenance,
        };
        session.messages.push(msg);
        session.message_count += 1;
        session.last_activity = Utc::now();

        // Trim old messages if over limit
        if session.messages.len() > MAX_HISTORY_PER_SESSION {
            let drain_count = session.messages.len() - MAX_HISTORY_PER_SESSION;
            session.messages.drain(..drain_count);
        }

        Ok(())
    }

    /// Get message history for a session
    pub async fn get_history(
        &self,
        session_id: &str,
        limit: usize,
        include_tool_results: bool,
    ) -> Result<Vec<SessionMessage>, &'static str> {
        let normalized = normalize_session_key(session_id).map_err(|_| "Invalid session ID")?;
        let sessions = self.sessions.read().await;
        let session = sessions.get(&normalized).ok_or("Session not found")?;

        let messages: Vec<SessionMessage> = session
            .messages
            .iter()
            .filter(|m| include_tool_results || m.provenance != MessageProvenance::ToolResult)
            .cloned()
            .collect();

        let start = messages.len().saturating_sub(limit);
        Ok(messages[start..].to_vec())
    }

    /// Number of active sessions
    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_manager_default() {
        let mgr = SessionManager::new();
        let sessions = mgr.list().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "main");
    }

    #[tokio::test]
    async fn test_create_session() {
        let mgr = SessionManager::new();
        let session = mgr.create("Research").await.unwrap();
        assert_eq!(session.name, "Research");
        assert_eq!(mgr.count().await, 2);
    }

    #[tokio::test]
    async fn test_create_session_empty_name() {
        let mgr = SessionManager::new();
        let result = mgr.create("").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_session() {
        let mgr = SessionManager::new();
        let session = mgr.get("main").await;
        assert!(session.is_some());
        assert_eq!(session.unwrap().name, "Main");

        let missing = mgr.get("nonexistent").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_get_session_normalized() {
        let mgr = SessionManager::new();
        // Key normalization: "Main" → "main"
        let session = mgr.get("MAIN").await;
        assert!(session.is_some());
        assert_eq!(session.unwrap().id, "main");
    }

    #[tokio::test]
    async fn test_record_activity() {
        let mgr = SessionManager::new();
        mgr.record_activity("main").await;
        mgr.record_activity("main").await;
        let session = mgr.get("main").await.unwrap();
        assert_eq!(session.message_count, 2);
    }

    #[tokio::test]
    async fn test_list_sorted_by_activity() {
        let mgr = SessionManager::new();
        let _s1 = mgr.create("Older").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let s2 = mgr.create("Newer").await.unwrap();

        let list = mgr.list().await;
        // Newest first
        assert_eq!(list[0].id, s2.id);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let mgr = SessionManager::new();
        let session = mgr.create("Temp").await.unwrap();
        assert_eq!(mgr.count().await, 2);

        mgr.delete(&session.id).await.unwrap();
        assert_eq!(mgr.count().await, 1);
    }

    #[tokio::test]
    async fn test_delete_main_fails() {
        let mgr = SessionManager::new();
        let result = mgr.delete("main").await;
        assert!(result.is_err());
    }

    // ── Path hardening tests (OpenClaw #15565) ──

    #[test]
    fn test_normalize_session_key_basic() {
        assert_eq!(normalize_session_key("main").unwrap(), "main");
        assert_eq!(normalize_session_key("MAIN").unwrap(), "main");
        assert_eq!(normalize_session_key("  Main  ").unwrap(), "main");
    }

    #[test]
    fn test_normalize_session_key_rejects_path_traversal() {
        assert!(normalize_session_key("../etc/passwd").is_err());
        assert!(normalize_session_key("foo/bar").is_err());
        assert!(normalize_session_key("foo\\bar").is_err());
        assert!(normalize_session_key("..").is_err());
    }

    #[test]
    fn test_normalize_session_key_rejects_empty() {
        assert!(normalize_session_key("").is_err());
        assert!(normalize_session_key("   ").is_err());
    }

    #[test]
    fn test_normalize_session_key_rejects_null_bytes() {
        assert!(normalize_session_key("foo\0bar").is_err());
    }

    #[test]
    fn test_normalize_session_key_rejects_too_long() {
        let long_key = "a".repeat(MAX_SESSION_ID_LEN + 1);
        assert!(normalize_session_key(&long_key).is_err());
    }

    // ── Agent-scoped session tests ──

    #[tokio::test]
    async fn test_with_agent() {
        let mgr = SessionManager::with_agent("work");
        let session = mgr.get("main").await.unwrap();
        assert_eq!(session.agent_id, "work");
        assert_eq!(session.kind, SessionKind::Main);
    }

    #[tokio::test]
    async fn test_create_with_kind() {
        let mgr = SessionManager::new();
        let session = mgr
            .create_with_kind("Research", "work", SessionKind::Other, None)
            .await
            .unwrap();
        assert_eq!(session.agent_id, "work");
        assert_eq!(session.kind, SessionKind::Other);
        assert!(session.parent_session.is_none());
    }

    #[tokio::test]
    async fn test_create_subagent() {
        let mgr = SessionManager::new();
        let child = mgr
            .create_subagent("main", "main", Some("research"))
            .await
            .unwrap();
        assert_eq!(child.kind, SessionKind::Subagent);
        assert_eq!(child.parent_session.as_deref(), Some("main"));
        assert_eq!(child.name, "research");
    }

    #[tokio::test]
    async fn test_list_for_agent() {
        let mgr = SessionManager::new();
        mgr.create_with_kind("Work Session", "work", SessionKind::Other, None)
            .await
            .unwrap();
        mgr.create_with_kind("Personal", "personal", SessionKind::Other, None)
            .await
            .unwrap();

        let work_sessions = mgr.list_for_agent("work").await;
        assert_eq!(work_sessions.len(), 1);
        assert_eq!(work_sessions[0].agent_id, "work");

        // "main" agent has the default session
        let main_sessions = mgr.list_for_agent("main").await;
        assert_eq!(main_sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_list_children() {
        let mgr = SessionManager::new();
        let child1 = mgr
            .create_subagent("main", "main", Some("child1"))
            .await
            .unwrap();
        let _child2 = mgr
            .create_subagent("main", "main", Some("child2"))
            .await
            .unwrap();
        // Unrelated session
        mgr.create_with_kind("Other", "main", SessionKind::Other, None)
            .await
            .unwrap();

        let children = mgr.list_children("main").await;
        assert_eq!(children.len(), 2);

        // No children for the child
        let grandchildren = mgr.list_children(&child1.id).await;
        assert!(grandchildren.is_empty());
    }

    #[tokio::test]
    async fn test_append_and_get_history() {
        let mgr = SessionManager::new();
        mgr.append_message("main", "user", "Hello", MessageProvenance::User)
            .await
            .unwrap();
        mgr.append_message(
            "main",
            "assistant",
            "Hi there!",
            MessageProvenance::Assistant,
        )
        .await
        .unwrap();
        mgr.append_message("main", "tool", "result", MessageProvenance::ToolResult)
            .await
            .unwrap();

        // Without tool results
        let history = mgr.get_history("main", 10, false).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].role, "assistant");

        // With tool results
        let history_all = mgr.get_history("main", 10, true).await.unwrap();
        assert_eq!(history_all.len(), 3);
    }

    #[tokio::test]
    async fn test_get_history_with_limit() {
        let mgr = SessionManager::new();
        for i in 0..5 {
            mgr.append_message(
                "main",
                "user",
                &format!("msg {}", i),
                MessageProvenance::User,
            )
            .await
            .unwrap();
        }
        let history = mgr.get_history("main", 2, true).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "msg 3");
        assert_eq!(history[1].content, "msg 4");
    }

    #[tokio::test]
    async fn test_append_message_nonexistent_session() {
        let mgr = SessionManager::new();
        let result = mgr
            .append_message("nonexistent", "user", "Hello", MessageProvenance::User)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_by_kind() {
        let mgr = SessionManager::new();
        mgr.create_subagent("main", "main", Some("sub1"))
            .await
            .unwrap();
        mgr.create_with_kind("Cron Job", "main", SessionKind::Cron, None)
            .await
            .unwrap();

        let subagents = mgr.list_by_kind(&[SessionKind::Subagent]).await;
        assert_eq!(subagents.len(), 1);

        let main_and_cron = mgr
            .list_by_kind(&[SessionKind::Main, SessionKind::Cron])
            .await;
        assert_eq!(main_and_cron.len(), 2);
    }

    // ── Credential redaction tests (OpenClaw #13073) ──

    #[test]
    fn test_redact_api_key() {
        let text = "My key is sk-abcdefghijklmnopqrstuvwxyz1234567890";
        let redacted = redact_credentials(text);
        assert!(redacted.contains("[REDACTED_API_KEY]"));
        assert!(!redacted.contains("sk-abcdef"));
    }

    #[test]
    fn test_redact_bearer_token() {
        let text = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
        let redacted = redact_credentials(text);
        assert!(redacted.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn test_redact_preserves_normal_text() {
        let text = "Hello, this is a normal message with no secrets.";
        let redacted = redact_credentials(text);
        assert_eq!(redacted, text);
    }

    #[test]
    fn test_redact_hex_token() {
        let text = "token=aabbccddee00112233445566778899aabbccddee001122";
        let redacted = redact_credentials(text);
        assert!(redacted.contains("[REDACTED_TOKEN]"));
    }
}
