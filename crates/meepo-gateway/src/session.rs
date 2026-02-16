//! Session management — each conversation gets its own session
//!
//! Includes session path hardening (OpenClaw #15565/#15410/#15140),
//! key normalization (OpenClaw #12846), and credential redaction (OpenClaw #13073).

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

/// A single chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub message_count: u64,
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
        let mut sessions = HashMap::new();
        let now = Utc::now();
        sessions.insert(
            "main".to_string(),
            Session {
                id: "main".to_string(),
                name: "Main".to_string(),
                created_at: now,
                last_activity: now,
                message_count: 0,
            },
        );
        Self {
            sessions: Arc::new(RwLock::new(sessions)),
        }
    }

    /// List all sessions
    pub async fn list(&self) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<Session> = sessions.values().cloned().collect();
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
            created_at: now,
            last_activity: now,
            message_count: 0,
        };
        sessions.insert(id.clone(), session.clone());
        info!("Created session '{}' ({})", trimmed_name, id);
        Ok(session)
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
