//! `delegate_to_agent` tool — delegates tasks to peer A2A agents

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info};

use meepo_core::tools::ToolHandler;

use crate::client::{A2aClient, PeerAgentConfig};

/// Tool that delegates a task to a peer A2A agent
pub struct DelegateToAgentTool {
    client: A2aClient,
    peers: Vec<PeerAgentConfig>,
}

impl DelegateToAgentTool {
    pub fn new(peers: Vec<PeerAgentConfig>) -> Self {
        Self {
            client: A2aClient::new(),
            peers,
        }
    }
}

#[async_trait]
impl ToolHandler for DelegateToAgentTool {
    fn name(&self) -> &str {
        "delegate_to_agent"
    }

    fn description(&self) -> &str {
        "Delegate a task to a peer AI agent via A2A protocol. \
         The agent will execute the task and return the result."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Name of a known agent or full URL of the agent"
                },
                "task": {
                    "type": "string",
                    "description": "Task description for the agent to execute"
                },
                "context": {
                    "type": "object",
                    "description": "Optional context (files, URLs, data) to pass to the agent"
                },
                "wait": {
                    "type": "boolean",
                    "description": "If true, wait for completion. If false, return task ID immediately.",
                    "default": true
                }
            },
            "required": ["agent", "task"]
        })
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let agent_name = input.get("agent")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'agent' parameter"))?;

        let task = input.get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'task' parameter"))?;

        let context = input.get("context").cloned().unwrap_or(serde_json::json!({}));
        let wait = input.get("wait").and_then(|v| v.as_bool()).unwrap_or(true);

        // Resolve agent: look up by name, or treat as URL
        let (base_url, token) = if agent_name.starts_with("http://") || agent_name.starts_with("https://") {
            (agent_name.to_string(), None)
        } else {
            let peer = self.peers.iter()
                .find(|p| p.name == agent_name)
                .ok_or_else(|| anyhow::anyhow!(
                    "Unknown agent '{}'. Known agents: {}",
                    agent_name,
                    self.peers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
                ))?;
            (peer.url.clone(), peer.token.clone())
        };

        debug!("Delegating task to agent at {}: {}", base_url, &task[..task.len().min(100)]);

        // Fetch agent card first (optional, for logging)
        match self.client.fetch_agent_card(&base_url, token.as_deref()).await {
            Ok(card) => info!("Agent '{}' capabilities: {:?}", card.name, card.capabilities),
            Err(e) => debug!("Could not fetch agent card: {} (proceeding anyway)", e),
        }

        if wait {
            let result = self.client.submit_and_wait(
                &base_url,
                token.as_deref(),
                task,
                context,
                std::time::Duration::from_secs(2),
                std::time::Duration::from_secs(300),
            ).await?;

            match result.result {
                Some(text) => Ok(format!("Agent completed task (status: {}):\n{}", result.status, text)),
                None => Ok(format!("Agent finished with status: {}", result.status)),
            }
        } else {
            let response = self.client.submit_task(&base_url, token.as_deref(), task, context).await?;
            Ok(format!("Task submitted to agent. Task ID: {} (status: {})", response.task_id, response.status))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_schema() {
        let tool = DelegateToAgentTool::new(vec![]);
        assert_eq!(tool.name(), "delegate_to_agent");
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("agent")));
        assert!(required.contains(&serde_json::json!("task")));
    }

    #[test]
    fn test_known_peers() {
        let peers = vec![
            PeerAgentConfig {
                name: "openclaw".to_string(),
                url: "http://localhost:3000".to_string(),
                token: Some("test".to_string()),
            },
        ];
        let tool = DelegateToAgentTool::new(peers);
        assert_eq!(tool.peers.len(), 1);
    }
}
