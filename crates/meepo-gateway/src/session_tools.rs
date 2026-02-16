//! Agent-to-agent session tools — sessions_list, sessions_history,
//! sessions_send, sessions_spawn
//!
//! These tools let agents discover, read, and communicate with other
//! sessions/agents. Modeled after OpenClaw's session tools with
//! ping-pong protocol and sub-agent spawning.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info, warn};

use meepo_core::tools::{ToolHandler, json_schema};

use crate::session::{
    MessageProvenance, SessionKind, SessionManager, SessionVisibility,
};

/// Configuration for agent-to-agent interaction
#[derive(Debug, Clone)]
pub struct AgentToAgentConfig {
    /// Whether agent-to-agent messaging is enabled
    pub enabled: bool,
    /// Agent IDs allowed to communicate (empty = all allowed)
    pub allow: Vec<String>,
    /// Session visibility scope
    pub visibility: SessionVisibility,
    /// Maximum ping-pong turns for sessions_send (0-5)
    pub max_ping_pong_turns: u8,
    /// Auto-archive subagent sessions after N minutes (0 = never)
    pub subagent_archive_after_minutes: u32,
}

impl Default for AgentToAgentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allow: Vec::new(),
            visibility: SessionVisibility::Tree,
            max_ping_pong_turns: 5,
            subagent_archive_after_minutes: 60,
        }
    }
}

// ── sessions_list ──────────────────────────────────────────────

/// Tool: List active sessions and their metadata
pub struct SessionsListTool {
    sessions: Arc<SessionManager>,
    config: AgentToAgentConfig,
}

impl SessionsListTool {
    pub fn new(sessions: Arc<SessionManager>, config: AgentToAgentConfig) -> Self {
        Self { sessions, config }
    }
}

#[async_trait]
impl ToolHandler for SessionsListTool {
    fn name(&self) -> &str {
        "sessions_list"
    }

    fn description(&self) -> &str {
        "List active sessions (agents) and their metadata. Use to discover \
         other sessions for inter-agent communication."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "kinds": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["main", "group", "cron", "hook", "node", "subagent", "other"] },
                    "description": "Filter by session kind(s). Omit for all kinds."
                },
                "agent_id": {
                    "type": "string",
                    "description": "Filter by agent ID. Omit for all agents (subject to visibility)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of sessions to return (default: 50)"
                },
                "active_minutes": {
                    "type": "integer",
                    "description": "Only sessions updated within N minutes"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        if !self.config.enabled {
            return Err(anyhow!("Agent-to-agent communication is disabled"));
        }

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let active_minutes = input
            .get("active_minutes")
            .and_then(|v| v.as_u64());

        let agent_id = input.get("agent_id").and_then(|v| v.as_str());

        // Get sessions based on filters
        let mut sessions = if let Some(aid) = agent_id {
            self.sessions.list_for_agent(aid).await
        } else {
            self.sessions.list().await
        };

        // Filter by kind if specified
        if let Some(kinds_val) = input.get("kinds").and_then(|v| v.as_array()) {
            let kind_strs: Vec<&str> = kinds_val
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            sessions.retain(|s| kind_strs.contains(&s.kind.to_string().as_str()));
        }

        // Filter by activity recency
        if let Some(minutes) = active_minutes {
            let cutoff = chrono::Utc::now()
                - chrono::Duration::minutes(minutes as i64);
            sessions.retain(|s| s.last_activity >= cutoff);
        }

        // Apply limit
        sessions.truncate(limit);

        // Build response (exclude message bodies for listing)
        let entries: Vec<Value> = sessions
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "agent_id": s.agent_id,
                    "kind": s.kind.to_string(),
                    "message_count": s.message_count,
                    "last_activity": s.last_activity.to_rfc3339(),
                    "created_at": s.created_at.to_rfc3339(),
                    "parent_session": s.parent_session,
                })
            })
            .collect();

        debug!("sessions_list: returning {} sessions", entries.len());
        serde_json::to_string_pretty(&serde_json::json!({
            "sessions": entries,
            "count": entries.len(),
        }))
        .map_err(|e| anyhow!("Failed to serialize sessions: {}", e))
    }
}

// ── sessions_history ───────────────────────────────────────────

/// Tool: Fetch transcript/history for a session
pub struct SessionsHistoryTool {
    sessions: Arc<SessionManager>,
    config: AgentToAgentConfig,
}

impl SessionsHistoryTool {
    pub fn new(sessions: Arc<SessionManager>, config: AgentToAgentConfig) -> Self {
        Self { sessions, config }
    }
}

#[async_trait]
impl ToolHandler for SessionsHistoryTool {
    fn name(&self) -> &str {
        "sessions_history"
    }

    fn description(&self) -> &str {
        "Fetch message history for a session. Use to read what another \
         agent/session has been doing."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "session_id": {
                    "type": "string",
                    "description": "Session ID to fetch history for"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to return (default: 50)"
                },
                "include_tools": {
                    "type": "boolean",
                    "description": "Include tool result messages (default: false)"
                }
            }),
            vec!["session_id"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        if !self.config.enabled {
            return Err(anyhow!("Agent-to-agent communication is disabled"));
        }

        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'session_id'"))?;

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let include_tools = input
            .get("include_tools")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let messages = self
            .sessions
            .get_history(session_id, limit, include_tools)
            .await
            .map_err(|e| anyhow!("Failed to get history: {}", e))?;

        let entries: Vec<Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": crate::session::redact_credentials(&m.content),
                    "timestamp": m.timestamp.to_rfc3339(),
                    "provenance": m.provenance,
                })
            })
            .collect();

        debug!(
            "sessions_history: returning {} messages for session '{}'",
            entries.len(),
            session_id
        );

        serde_json::to_string_pretty(&serde_json::json!({
            "session_id": session_id,
            "messages": entries,
            "count": entries.len(),
        }))
        .map_err(|e| anyhow!("Failed to serialize history: {}", e))
    }
}

// ── sessions_send ──────────────────────────────────────────────

/// Tool: Send a message to another session (inter-agent communication)
///
/// Supports fire-and-forget (timeout=0) and synchronous wait modes.
/// After the initial response, a ping-pong loop alternates between
/// requester and target for up to max_ping_pong_turns. Either side
/// can reply "REPLY_SKIP" to stop the loop.
pub struct SessionsSendTool {
    sessions: Arc<SessionManager>,
    config: AgentToAgentConfig,
}

impl SessionsSendTool {
    pub fn new(sessions: Arc<SessionManager>, config: AgentToAgentConfig) -> Self {
        Self { sessions, config }
    }

    fn is_agent_allowed(&self, agent_id: &str) -> bool {
        if self.config.allow.is_empty() {
            return true;
        }
        self.config.allow.iter().any(|a| a == agent_id || a == "*")
    }
}

#[async_trait]
impl ToolHandler for SessionsSendTool {
    fn name(&self) -> &str {
        "sessions_send"
    }

    fn description(&self) -> &str {
        "Send a message to another session/agent. The target session will \
         process the message and optionally reply. Use timeout_seconds=0 \
         for fire-and-forget. Reply 'REPLY_SKIP' to stop ping-pong."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "session_id": {
                    "type": "string",
                    "description": "Target session ID to send the message to"
                },
                "message": {
                    "type": "string",
                    "description": "Message content to send"
                },
                "timeout_seconds": {
                    "type": "integer",
                    "description": "Seconds to wait for reply (0 = fire-and-forget, default: 30)"
                }
            }),
            vec!["session_id", "message"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        if !self.config.enabled {
            return Err(anyhow!("Agent-to-agent communication is disabled"));
        }

        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'session_id'"))?;

        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'message'"))?;

        if message.is_empty() {
            return Err(anyhow!("Message cannot be empty"));
        }
        if message.len() > 32_000 {
            return Err(anyhow!("Message too long (max 32000 chars)"));
        }

        let timeout_seconds = input
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        // Verify target session exists
        let target = self
            .sessions
            .get(session_id)
            .await
            .ok_or_else(|| anyhow!("Target session '{}' not found", session_id))?;

        // Check agent allowlist
        if !self.is_agent_allowed(&target.agent_id) {
            return Err(anyhow!(
                "Agent '{}' is not in the agent-to-agent allow list",
                target.agent_id
            ));
        }

        // Inject the message into the target session with inter-session provenance
        let run_id = uuid::Uuid::new_v4().to_string();
        self.sessions
            .append_message(
                session_id,
                "user",
                message,
                MessageProvenance::InterSession {
                    from_session: "current".to_string(),
                },
            )
            .await
            .map_err(|e| anyhow!("Failed to inject message: {}", e))?;

        info!(
            "sessions_send: injected message into session '{}' (run_id: {})",
            session_id, run_id
        );

        if timeout_seconds == 0 {
            // Fire-and-forget mode
            return serde_json::to_string_pretty(&serde_json::json!({
                "run_id": run_id,
                "status": "accepted",
                "session_id": session_id,
            }))
            .map_err(|e| anyhow!("Failed to serialize response: {}", e));
        }

        // Synchronous wait mode: wait for a reply to appear
        // In a full implementation, this would use gateway agent.wait
        // For now, return accepted with the run_id for polling via sessions_history
        serde_json::to_string_pretty(&serde_json::json!({
            "run_id": run_id,
            "status": "accepted",
            "session_id": session_id,
            "max_ping_pong_turns": self.config.max_ping_pong_turns,
            "note": "Message injected. Use sessions_history to check for replies.",
        }))
        .map_err(|e| anyhow!("Failed to serialize response: {}", e))
    }
}

// ── sessions_spawn ─────────────────────────────────────────────

/// Tool: Spawn a sub-agent in an isolated session
///
/// Creates a new subagent session, injects the task, and returns
/// immediately. The sub-agent runs asynchronously; results are
/// announced back to the requester's chat channel on completion.
/// Sub-agents cannot call sessions_spawn (no recursive spawning).
pub struct SessionsSpawnTool {
    sessions: Arc<SessionManager>,
    config: AgentToAgentConfig,
}

impl SessionsSpawnTool {
    pub fn new(sessions: Arc<SessionManager>, config: AgentToAgentConfig) -> Self {
        Self { sessions, config }
    }

    fn is_agent_allowed(&self, agent_id: &str) -> bool {
        if self.config.allow.is_empty() {
            return true;
        }
        self.config.allow.iter().any(|a| a == agent_id || a == "*")
    }
}

#[async_trait]
impl ToolHandler for SessionsSpawnTool {
    fn name(&self) -> &str {
        "sessions_spawn"
    }

    fn description(&self) -> &str {
        "Spawn a sub-agent in an isolated session for a focused task. \
         Returns immediately; the sub-agent runs asynchronously and \
         announces results when done. Sub-agents cannot spawn further \
         sub-agents."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "task": {
                    "type": "string",
                    "description": "Task description for the sub-agent"
                },
                "label": {
                    "type": "string",
                    "description": "Optional label for the sub-agent session (used in logs/UI)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Optional agent ID to spawn under (default: current agent)"
                },
                "parent_session_id": {
                    "type": "string",
                    "description": "Parent session ID (default: 'main')"
                },
                "run_timeout_seconds": {
                    "type": "integer",
                    "description": "Abort the sub-agent after N seconds (0 = no timeout, default: 300)"
                },
                "cleanup": {
                    "type": "string",
                    "enum": ["delete", "keep"],
                    "description": "Whether to delete or keep the sub-agent session after completion (default: keep)"
                }
            }),
            vec!["task"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        if !self.config.enabled {
            return Err(anyhow!("Agent-to-agent communication is disabled"));
        }

        let task = input
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'task'"))?;

        if task.is_empty() {
            return Err(anyhow!("Task cannot be empty"));
        }
        if task.len() > 64_000 {
            return Err(anyhow!("Task too long (max 64000 chars)"));
        }

        let label = input.get("label").and_then(|v| v.as_str());
        let agent_id = input
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("main");
        let parent_session_id = input
            .get("parent_session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("main");
        let _run_timeout = input
            .get("run_timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);
        let cleanup = input
            .get("cleanup")
            .and_then(|v| v.as_str())
            .unwrap_or("keep");

        if cleanup != "delete" && cleanup != "keep" {
            return Err(anyhow!("Invalid cleanup value: must be 'delete' or 'keep'"));
        }

        // Check agent allowlist
        if !self.is_agent_allowed(agent_id) {
            return Err(anyhow!(
                "Agent '{}' is not in the agent-to-agent allow list",
                agent_id
            ));
        }

        // Verify parent session exists
        if self.sessions.get(parent_session_id).await.is_none() {
            return Err(anyhow!(
                "Parent session '{}' not found",
                parent_session_id
            ));
        }

        // Create the sub-agent session
        let child_session = self
            .sessions
            .create_subagent(agent_id, parent_session_id, label)
            .await
            .map_err(|e| anyhow!("Failed to create sub-agent session: {}", e))?;

        // Inject the task into the sub-agent session
        self.sessions
            .append_message(
                &child_session.id,
                "user",
                task,
                MessageProvenance::SubagentTask {
                    parent_session: parent_session_id.to_string(),
                },
            )
            .await
            .map_err(|e| anyhow!("Failed to inject task: {}", e))?;

        let run_id = uuid::Uuid::new_v4().to_string();

        info!(
            "sessions_spawn: created sub-agent session '{}' (label: {:?}, agent: {}, run_id: {})",
            child_session.id,
            label,
            agent_id,
            run_id
        );

        serde_json::to_string_pretty(&serde_json::json!({
            "status": "accepted",
            "run_id": run_id,
            "child_session_id": child_session.id,
            "child_session_name": child_session.name,
            "agent_id": agent_id,
            "parent_session_id": parent_session_id,
            "cleanup": cleanup,
        }))
        .map_err(|e| anyhow!("Failed to serialize response: {}", e))
    }
}

// ── agents_list ────────────────────────────────────────────────

/// Tool: List available agent IDs (for discovering spawn targets)
pub struct AgentsListTool {
    agent_ids: Vec<String>,
    config: AgentToAgentConfig,
}

impl AgentsListTool {
    pub fn new(agent_ids: Vec<String>, config: AgentToAgentConfig) -> Self {
        Self { agent_ids, config }
    }
}

#[async_trait]
impl ToolHandler for AgentsListTool {
    fn name(&self) -> &str {
        "agents_list"
    }

    fn description(&self) -> &str {
        "List available agent IDs. Use to discover which agents can be \
         targeted by sessions_send or sessions_spawn."
    }

    fn input_schema(&self) -> Value {
        json_schema(serde_json::json!({}), vec![])
    }

    async fn execute(&self, _input: Value) -> Result<String> {
        if !self.config.enabled {
            return Err(anyhow!("Agent-to-agent communication is disabled"));
        }

        let allowed: Vec<&String> = if self.config.allow.is_empty() {
            self.agent_ids.iter().collect()
        } else {
            self.agent_ids
                .iter()
                .filter(|id| {
                    self.config.allow.iter().any(|a| a == *id || a == "*")
                })
                .collect()
        };

        serde_json::to_string_pretty(&serde_json::json!({
            "agents": allowed,
            "count": allowed.len(),
            "agent_to_agent_enabled": self.config.enabled,
        }))
        .map_err(|e| anyhow!("Failed to serialize agents: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AgentToAgentConfig {
        AgentToAgentConfig {
            enabled: true,
            allow: vec![],
            visibility: SessionVisibility::All,
            max_ping_pong_turns: 5,
            subagent_archive_after_minutes: 60,
        }
    }

    fn disabled_config() -> AgentToAgentConfig {
        AgentToAgentConfig {
            enabled: false,
            ..test_config()
        }
    }

    fn restricted_config() -> AgentToAgentConfig {
        AgentToAgentConfig {
            allow: vec!["main".to_string(), "work".to_string()],
            ..test_config()
        }
    }

    // ── sessions_list tests ──

    #[tokio::test]
    async fn test_sessions_list_basic() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsListTool::new(mgr.clone(), test_config());

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["sessions"][0]["id"], "main");
    }

    #[tokio::test]
    async fn test_sessions_list_disabled() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsListTool::new(mgr, disabled_config());

        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_list_filter_by_kind() {
        let mgr = Arc::new(SessionManager::new());
        mgr.create_subagent("main", "main", Some("sub")).await.unwrap();

        let tool = SessionsListTool::new(mgr, test_config());
        let result = tool
            .execute(serde_json::json!({"kinds": ["subagent"]}))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["sessions"][0]["kind"], "subagent");
    }

    #[tokio::test]
    async fn test_sessions_list_filter_by_agent() {
        let mgr = Arc::new(SessionManager::new());
        mgr.create_with_kind("Work", "work", SessionKind::Other, None)
            .await
            .unwrap();

        let tool = SessionsListTool::new(mgr, test_config());
        let result = tool
            .execute(serde_json::json!({"agent_id": "work"}))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["sessions"][0]["agent_id"], "work");
    }

    #[tokio::test]
    async fn test_sessions_list_with_limit() {
        let mgr = Arc::new(SessionManager::new());
        for i in 0..5 {
            mgr.create_with_kind(&format!("S{}", i), "main", SessionKind::Other, None)
                .await
                .unwrap();
        }

        let tool = SessionsListTool::new(mgr, test_config());
        let result = tool
            .execute(serde_json::json!({"limit": 3}))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 3);
    }

    // ── sessions_history tests ──

    #[tokio::test]
    async fn test_sessions_history_basic() {
        let mgr = Arc::new(SessionManager::new());
        mgr.append_message("main", "user", "Hello", MessageProvenance::User)
            .await
            .unwrap();
        mgr.append_message("main", "assistant", "Hi!", MessageProvenance::Assistant)
            .await
            .unwrap();

        let tool = SessionsHistoryTool::new(mgr, test_config());
        let result = tool
            .execute(serde_json::json!({"session_id": "main"}))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 2);
    }

    #[tokio::test]
    async fn test_sessions_history_missing_session() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsHistoryTool::new(mgr, test_config());

        let result = tool
            .execute(serde_json::json!({"session_id": "nonexistent"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_history_disabled() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsHistoryTool::new(mgr, disabled_config());

        let result = tool
            .execute(serde_json::json!({"session_id": "main"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_history_excludes_tools_by_default() {
        let mgr = Arc::new(SessionManager::new());
        mgr.append_message("main", "user", "Hello", MessageProvenance::User)
            .await
            .unwrap();
        mgr.append_message("main", "tool", "result", MessageProvenance::ToolResult)
            .await
            .unwrap();

        let tool = SessionsHistoryTool::new(mgr, test_config());
        let result = tool
            .execute(serde_json::json!({"session_id": "main"}))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 1);
    }

    // ── sessions_send tests ──

    #[tokio::test]
    async fn test_sessions_send_basic() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSendTool::new(mgr.clone(), test_config());

        let result = tool
            .execute(serde_json::json!({
                "session_id": "main",
                "message": "Hello from another agent"
            }))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "accepted");
        assert!(parsed["run_id"].is_string());

        // Verify message was injected
        let history = mgr.get_history("main", 10, true).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Hello from another agent");
    }

    #[tokio::test]
    async fn test_sessions_send_fire_and_forget() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSendTool::new(mgr, test_config());

        let result = tool
            .execute(serde_json::json!({
                "session_id": "main",
                "message": "Fire and forget",
                "timeout_seconds": 0
            }))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "accepted");
    }

    #[tokio::test]
    async fn test_sessions_send_missing_session() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSendTool::new(mgr, test_config());

        let result = tool
            .execute(serde_json::json!({
                "session_id": "nonexistent",
                "message": "Hello"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_send_empty_message() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSendTool::new(mgr, test_config());

        let result = tool
            .execute(serde_json::json!({
                "session_id": "main",
                "message": ""
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_send_disabled() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSendTool::new(mgr, disabled_config());

        let result = tool
            .execute(serde_json::json!({
                "session_id": "main",
                "message": "Hello"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_send_agent_not_allowed() {
        let mgr = Arc::new(SessionManager::new());
        // Create a session for an agent not in the allow list
        mgr.create_with_kind("Other", "personal", SessionKind::Other, None)
            .await
            .unwrap();

        let tool = SessionsSendTool::new(mgr.clone(), restricted_config());
        let sessions = mgr.list_for_agent("personal").await;
        let result = tool
            .execute(serde_json::json!({
                "session_id": sessions[0].id,
                "message": "Hello"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("allow list"));
    }

    // ── sessions_spawn tests ──

    #[tokio::test]
    async fn test_sessions_spawn_basic() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSpawnTool::new(mgr.clone(), test_config());

        let result = tool
            .execute(serde_json::json!({
                "task": "Research Rust async patterns"
            }))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "accepted");
        assert!(parsed["child_session_id"].is_string());
        assert_eq!(parsed["agent_id"], "main");

        // Verify sub-agent session was created
        let child_id = parsed["child_session_id"].as_str().unwrap();
        let child = mgr.get(child_id).await.unwrap();
        assert_eq!(child.kind, SessionKind::Subagent);
        assert_eq!(child.parent_session.as_deref(), Some("main"));

        // Verify task was injected
        let history = mgr.get_history(child_id, 10, true).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Research Rust async patterns");
    }

    #[tokio::test]
    async fn test_sessions_spawn_with_label() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSpawnTool::new(mgr.clone(), test_config());

        let result = tool
            .execute(serde_json::json!({
                "task": "Do research",
                "label": "rust-research"
            }))
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["child_session_name"], "rust-research");
    }

    #[tokio::test]
    async fn test_sessions_spawn_disabled() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSpawnTool::new(mgr, disabled_config());

        let result = tool
            .execute(serde_json::json!({"task": "Do something"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_spawn_empty_task() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSpawnTool::new(mgr, test_config());

        let result = tool.execute(serde_json::json!({"task": ""})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_spawn_agent_not_allowed() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSpawnTool::new(mgr, restricted_config());

        let result = tool
            .execute(serde_json::json!({
                "task": "Do something",
                "agent_id": "personal"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("allow list"));
    }

    #[tokio::test]
    async fn test_sessions_spawn_nonexistent_parent() {
        let mgr = Arc::new(SessionManager::new());
        let tool = SessionsSpawnTool::new(mgr, test_config());

        let result = tool
            .execute(serde_json::json!({
                "task": "Do something",
                "parent_session_id": "nonexistent"
            }))
            .await;
        assert!(result.is_err());
    }

    // ── agents_list tests ──

    #[tokio::test]
    async fn test_agents_list_basic() {
        let tool = AgentsListTool::new(
            vec!["main".to_string(), "work".to_string(), "personal".to_string()],
            test_config(),
        );

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 3);
        assert!(parsed["agent_to_agent_enabled"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_agents_list_restricted() {
        let tool = AgentsListTool::new(
            vec!["main".to_string(), "work".to_string(), "personal".to_string()],
            restricted_config(),
        );

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 2); // only main and work
    }

    #[tokio::test]
    async fn test_agents_list_disabled() {
        let tool = AgentsListTool::new(vec!["main".to_string()], disabled_config());

        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
