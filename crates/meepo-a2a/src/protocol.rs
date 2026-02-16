//! A2A (Agent-to-Agent) protocol types
//!
//! Implements Google's Agent-to-Agent protocol for multi-agent task delegation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Agent Card — advertises capabilities at /.well-known/agent.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub authentication: AuthConfig,
}

/// Authentication configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub schemes: Vec<String>,
}

/// Task submission request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub prompt: String,
    #[serde(default)]
    pub context: Value,
}

/// Task status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub task_id: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// Task lifecycle status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Submitted,
    Working,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Submitted => write!(f, "submitted"),
            Self::Working => write!(f, "working"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_serialization() {
        let card = AgentCard {
            name: "meepo".to_string(),
            description: "AI agent".to_string(),
            url: "http://localhost:8081".to_string(),
            capabilities: vec!["file_operations".to_string(), "web_research".to_string()],
            authentication: AuthConfig {
                schemes: vec!["bearer".to_string()],
            },
        };
        let json = serde_json::to_value(&card).unwrap();
        assert_eq!(json["name"], "meepo");
        assert_eq!(json["capabilities"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Working.to_string(), "working");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
    }

    #[test]
    fn test_task_request_deserialization() {
        let json = r#"{"prompt":"search web for Rust news","context":{}}"#;
        let req: TaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "search web for Rust news");
    }

    #[test]
    fn test_task_response_serialization() {
        let resp = TaskResponse {
            task_id: "abc-123".to_string(),
            status: TaskStatus::Completed,
            result: Some("Found 5 articles".to_string()),
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "completed");
        assert!(json["result"].is_string());
    }

    #[test]
    fn test_task_status_all_variants_display() {
        assert_eq!(TaskStatus::Submitted.to_string(), "submitted");
        assert_eq!(TaskStatus::Working.to_string(), "working");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
        assert_eq!(TaskStatus::Failed.to_string(), "failed");
        assert_eq!(TaskStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_task_status_serde_roundtrip() {
        let statuses = [
            TaskStatus::Submitted,
            TaskStatus::Working,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, parsed);
        }
    }

    #[test]
    fn test_task_status_equality() {
        assert_eq!(TaskStatus::Completed, TaskStatus::Completed);
        assert_ne!(TaskStatus::Completed, TaskStatus::Failed);
    }

    #[test]
    fn test_agent_card_roundtrip() {
        let card = AgentCard {
            name: "test-agent".to_string(),
            description: "A test agent".to_string(),
            url: "http://localhost:9000".to_string(),
            capabilities: vec!["search".to_string(), "code".to_string()],
            authentication: AuthConfig {
                schemes: vec!["bearer".to_string()],
            },
        };
        let json = serde_json::to_string(&card).unwrap();
        let parsed: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test-agent");
        assert_eq!(parsed.capabilities.len(), 2);
        assert_eq!(parsed.authentication.schemes[0], "bearer");
    }

    #[test]
    fn test_agent_card_default_auth() {
        let json = r#"{"name":"a","description":"b","url":"http://x","capabilities":[]}"#;
        let card: AgentCard = serde_json::from_str(json).unwrap();
        assert!(card.authentication.schemes.is_empty());
    }

    #[test]
    fn test_task_request_with_context() {
        let req = TaskRequest {
            prompt: "do something".to_string(),
            context: serde_json::json!({"key": "value", "nested": {"a": 1}}),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["prompt"], "do something");
        assert_eq!(json["context"]["nested"]["a"], 1);
    }

    #[test]
    fn test_task_request_default_context() {
        let json = r#"{"prompt":"hello"}"#;
        let req: TaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "hello");
        assert_eq!(req.context, Value::Null);
    }

    #[test]
    fn test_task_response_without_result() {
        let resp = TaskResponse {
            task_id: "t-1".to_string(),
            status: TaskStatus::Working,
            result: None,
            created_at: Utc::now(),
            completed_at: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("result").is_none());
        assert!(json.get("completed_at").is_none());
    }

    #[test]
    fn test_error_response_serde() {
        let err = ErrorResponse {
            error: "something went wrong".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        let parsed: ErrorResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.error, "something went wrong");
    }

    #[test]
    fn test_auth_config_default() {
        let auth = AuthConfig::default();
        assert!(auth.schemes.is_empty());
    }
}
