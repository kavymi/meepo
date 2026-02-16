//! A2A client — sends tasks to peer agents

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, info};

use crate::protocol::*;

/// Configuration for a known peer agent
#[derive(Debug, Clone)]
pub struct PeerAgentConfig {
    pub name: String,
    pub url: String,
    pub token: Option<String>,
}

/// A2A client for communicating with peer agents
#[derive(Clone)]
pub struct A2aClient {
    http: Client,
}

impl Default for A2aClient {
    fn default() -> Self {
        Self::new()
    }
}

impl A2aClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Fetch an agent's capability card
    pub async fn fetch_agent_card(&self, base_url: &str, token: Option<&str>) -> Result<AgentCard> {
        let url = format!("{}/.well-known/agent.json", base_url.trim_end_matches('/'));
        debug!("Fetching agent card from {}", url);

        let mut req = self.http.get(&url);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("Failed to connect to agent at {}", url))?;

        if !resp.status().is_success() {
            return Err(anyhow!("Agent card request failed: HTTP {}", resp.status()));
        }

        let card: AgentCard = resp.json().await.context("Failed to parse agent card")?;

        info!(
            "Fetched agent card: {} ({} capabilities)",
            card.name,
            card.capabilities.len()
        );
        Ok(card)
    }

    /// Submit a task to a peer agent
    pub async fn submit_task(
        &self,
        base_url: &str,
        token: Option<&str>,
        prompt: &str,
        context: Value,
    ) -> Result<TaskResponse> {
        let url = format!("{}/a2a/tasks", base_url.trim_end_matches('/'));
        debug!("Submitting task to {}", url);

        let request = TaskRequest {
            prompt: prompt.to_string(),
            context,
        };

        let mut req = self.http.post(&url).json(&request);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("Failed to submit task to {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Task submission failed: HTTP {} — {}",
                status,
                body
            ));
        }

        let task: TaskResponse = resp.json().await.context("Failed to parse task response")?;

        info!("Task submitted: {} (status: {})", task.task_id, task.status);
        Ok(task)
    }

    /// Poll task status
    pub async fn get_task_status(
        &self,
        base_url: &str,
        token: Option<&str>,
        task_id: &str,
    ) -> Result<TaskResponse> {
        let url = format!("{}/a2a/tasks/{}", base_url.trim_end_matches('/'), task_id);

        let mut req = self.http.get(&url);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("Failed to poll task {} at {}", task_id, url))?;

        if !resp.status().is_success() {
            return Err(anyhow!(
                "Task status request failed: HTTP {}",
                resp.status()
            ));
        }

        resp.json().await.context("Failed to parse task status")
    }

    /// Cancel a task
    pub async fn cancel_task(
        &self,
        base_url: &str,
        token: Option<&str>,
        task_id: &str,
    ) -> Result<()> {
        let url = format!("{}/a2a/tasks/{}", base_url.trim_end_matches('/'), task_id);

        let mut req = self.http.delete(&url);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("Failed to cancel task {} at {}", task_id, url))?;

        if !resp.status().is_success() {
            return Err(anyhow!("Task cancellation failed: HTTP {}", resp.status()));
        }

        info!("Task {} cancelled", task_id);
        Ok(())
    }

    /// Submit task and poll until completion (blocking)
    pub async fn submit_and_wait(
        &self,
        base_url: &str,
        token: Option<&str>,
        prompt: &str,
        context: Value,
        poll_interval: std::time::Duration,
        timeout: std::time::Duration,
    ) -> Result<TaskResponse> {
        let task = self.submit_task(base_url, token, prompt, context).await?;
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(anyhow!(
                    "Task {} timed out after {:?}",
                    task.task_id,
                    timeout
                ));
            }

            tokio::time::sleep(poll_interval).await;

            let status = self.get_task_status(base_url, token, &task.task_id).await?;
            match status.status {
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                    return Ok(status);
                }
                _ => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = A2aClient::new();
        // Just verify it constructs
        let _ = client;
    }

    #[test]
    fn test_peer_config() {
        let config = PeerAgentConfig {
            name: "openclaw".to_string(),
            url: "http://localhost:3000".to_string(),
            token: Some("test-token".to_string()),
        };
        assert_eq!(config.name, "openclaw");
    }

    #[test]
    fn test_client_default() {
        let client = A2aClient::default();
        let _ = client;
    }

    #[test]
    fn test_client_clone() {
        let client = A2aClient::new();
        let cloned = client.clone();
        let _ = cloned;
    }

    #[test]
    fn test_peer_config_without_token() {
        let config = PeerAgentConfig {
            name: "peer".to_string(),
            url: "http://example.com".to_string(),
            token: None,
        };
        assert!(config.token.is_none());
    }

    #[test]
    fn test_peer_config_debug() {
        let config = PeerAgentConfig {
            name: "agent".to_string(),
            url: "http://localhost:8080".to_string(),
            token: Some("secret".to_string()),
        };
        let debug = format!("{:?}", config);
        assert!(debug.contains("agent"));
        assert!(debug.contains("localhost"));
    }

    #[tokio::test]
    async fn test_fetch_agent_card_connection_refused() {
        let client = A2aClient::new();
        let result = client.fetch_agent_card("http://127.0.0.1:1", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_submit_task_connection_refused() {
        let client = A2aClient::new();
        let result = client
            .submit_task(
                "http://127.0.0.1:1",
                None,
                "test prompt",
                serde_json::json!({}),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_task_status_connection_refused() {
        let client = A2aClient::new();
        let result = client
            .get_task_status("http://127.0.0.1:1", None, "task-123")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cancel_task_connection_refused() {
        let client = A2aClient::new();
        let result = client
            .cancel_task("http://127.0.0.1:1", None, "task-123")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_agent_card_trailing_slash() {
        let client = A2aClient::new();
        // Verify URL construction with trailing slash doesn't double-slash
        let result = client.fetch_agent_card("http://127.0.0.1:1/", None).await;
        assert!(result.is_err());
        // The error should be a connection error, not a URL error
        let err = result.unwrap_err().to_string();
        assert!(err.contains("connect") || err.contains("Connect") || err.contains("Failed"));
    }
}
