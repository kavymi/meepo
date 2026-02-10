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
}
