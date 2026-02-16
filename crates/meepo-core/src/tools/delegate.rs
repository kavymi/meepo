//! delegate_tasks tool — Divided We Stand
//!
//! Allows the prime Meepo to spawn focused clones for parallel or background work.
//! Each clone gets a scoped toolset and cannot recursively spawn more clones.

use std::sync::{Arc, OnceLock};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tracing::info;

use crate::orchestrator::{ExecutionMode, SubTask, TaskGroup, TaskOrchestrator};
use crate::tools::{ToolHandler, ToolRegistry};
use crate::types::ChannelType;

/// Tool that spawns Meepo clones for delegated work — Divided We Stand.
///
/// Uses `OnceLock` to resolve the circular dependency: the tool needs the
/// registry, but the registry contains the tool. The slot is filled after
/// the registry is wrapped in Arc.
pub struct DelegateTasksTool {
    orchestrator: Arc<TaskOrchestrator>,
    registry_slot: Arc<OnceLock<Arc<ToolRegistry>>>,
}

impl DelegateTasksTool {
    pub fn new(
        orchestrator: Arc<TaskOrchestrator>,
        registry_slot: Arc<OnceLock<Arc<ToolRegistry>>>,
    ) -> Self {
        Self {
            orchestrator,
            registry_slot,
        }
    }

    fn registry(&self) -> Result<Arc<ToolRegistry>> {
        self.registry_slot
            .get()
            .cloned()
            .ok_or_else(|| anyhow!("Orchestrator registry not initialized"))
    }
}

#[async_trait]
impl ToolHandler for DelegateTasksTool {
    fn name(&self) -> &str {
        "delegate_tasks"
    }

    fn description(&self) -> &str {
        "Spawn Meepo clones to divide and conquer. Divided We Stand. \
         Use 'parallel' mode to send clones digging simultaneously and wait for all results. \
         Use 'background' mode to send clones off to work independently — they'll report back when done."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["parallel", "background"],
                    "description": "parallel: blocks until all complete, returns combined results. background: returns immediately, notifies user on completion."
                },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "task_id": {
                                "type": "string",
                                "description": "Short identifier like 'search_events' or 'check_weather'"
                            },
                            "prompt": {
                                "type": "string",
                                "description": "Focused instruction for this sub-agent"
                            },
                            "context_summary": {
                                "type": "string",
                                "description": "Relevant context from the conversation this sub-agent needs"
                            },
                            "tools": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Tool names this sub-agent can use (e.g. ['browse_url', 'read_calendar'])"
                            }
                        },
                        "required": ["task_id", "prompt", "tools"]
                    },
                    "description": "Array of sub-tasks to delegate"
                }
            },
            "required": ["mode", "tasks"]
        })
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let registry = self.registry()?;

        let mode_str = input
            .get("mode")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'mode' parameter"))?;

        let mode: ExecutionMode = serde_json::from_value(Value::String(mode_str.to_string()))
            .map_err(|_| {
                anyhow!(
                    "Invalid mode '{}'. Must be 'parallel' or 'background'.",
                    mode_str
                )
            })?;

        let tasks_value = input
            .get("tasks")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("Missing or invalid 'tasks' parameter"))?;

        if tasks_value.is_empty() {
            return Err(anyhow!("'tasks' array cannot be empty"));
        }

        let mut tasks = Vec::new();
        for (i, task_value) in tasks_value.iter().enumerate() {
            let task_id = task_value
                .get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Task {} missing 'task_id'", i))?
                .to_string();

            let prompt = task_value
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Task {} missing 'prompt'", i))?
                .to_string();

            let context_summary = task_value
                .get("context_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let tools: Vec<String> = task_value
                .get("tools")
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow!("Task {} missing 'tools'", i))?
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .filter(|t| t != "delegate_tasks") // Prevent recursive sub-agent spawning
                .collect();

            tasks.push(SubTask {
                task_id,
                prompt,
                context_summary,
                allowed_tools: tools,
            });
        }

        let group_id = format!("{}-{}", mode_str, &uuid::Uuid::new_v4().to_string()[..8]);
        info!(
            "Delegating {} tasks in {} mode (group: {})",
            tasks.len(),
            mode_str,
            group_id
        );

        let group = TaskGroup {
            group_id,
            mode: mode.clone(),
            channel: ChannelType::Internal,
            reply_to: None,
            tasks,
            created_at: Utc::now(),
        };

        match mode {
            ExecutionMode::Parallel => self.orchestrator.run_parallel(group, registry).await,
            ExecutionMode::Background => self.orchestrator.run_background(group, registry).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_execution_mode() {
        let parallel: ExecutionMode =
            serde_json::from_value(Value::String("parallel".to_string())).unwrap();
        assert_eq!(parallel, ExecutionMode::Parallel);

        let background: ExecutionMode =
            serde_json::from_value(Value::String("background".to_string())).unwrap();
        assert_eq!(background, ExecutionMode::Background);

        let invalid: std::result::Result<ExecutionMode, _> =
            serde_json::from_value(Value::String("invalid".to_string()));
        assert!(invalid.is_err());
    }

    #[test]
    fn test_delegate_tool_schema_has_required_fields() {
        let slot = Arc::new(OnceLock::new());
        let api = crate::api::ApiClient::new("key".to_string(), None);
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let orch = Arc::new(TaskOrchestrator::new(
            api,
            tx,
            crate::orchestrator::OrchestratorConfig::default(),
        ));
        let tool = DelegateTasksTool::new(orch, slot);

        assert_eq!(tool.name(), "delegate_tasks");

        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("mode".to_string())));
        assert!(required.contains(&Value::String("tasks".to_string())));
    }

    #[tokio::test]
    async fn test_delegate_tool_errors_without_registry() {
        let slot = Arc::new(OnceLock::new());
        let api = crate::api::ApiClient::new("key".to_string(), None);
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let orch = Arc::new(TaskOrchestrator::new(
            api,
            tx,
            crate::orchestrator::OrchestratorConfig::default(),
        ));
        let tool = DelegateTasksTool::new(orch, slot);

        let input = serde_json::json!({
            "mode": "parallel",
            "tasks": [{"task_id": "t1", "prompt": "test", "tools": []}]
        });
        let result = tool.execute(input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_delegate_tool_rejects_empty_tasks() {
        let slot = Arc::new(OnceLock::new());
        let api = crate::api::ApiClient::new("key".to_string(), None);
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let orch = Arc::new(TaskOrchestrator::new(
            api,
            tx,
            crate::orchestrator::OrchestratorConfig::default(),
        ));
        let tool = DelegateTasksTool::new(orch, slot.clone());

        let registry = Arc::new(crate::tools::ToolRegistry::new());
        assert!(slot.set(registry).is_ok());

        let input = serde_json::json!({
            "mode": "parallel",
            "tasks": []
        });
        let result = tool.execute(input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_delegate_tool_rejects_invalid_mode() {
        let slot = Arc::new(OnceLock::new());
        let api = crate::api::ApiClient::new("key".to_string(), None);
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let orch = Arc::new(TaskOrchestrator::new(
            api,
            tx,
            crate::orchestrator::OrchestratorConfig::default(),
        ));
        let tool = DelegateTasksTool::new(orch, slot.clone());

        let registry = Arc::new(crate::tools::ToolRegistry::new());
        assert!(slot.set(registry).is_ok());

        let input = serde_json::json!({
            "mode": "invalid_mode",
            "tasks": [{"task_id": "t1", "prompt": "test", "tools": []}]
        });
        let result = tool.execute(input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid mode"));
    }

    #[test]
    fn test_delegate_tasks_stripped_from_allowed_tools() {
        // Verify at the parsing level that delegate_tasks is filtered out.
        // We test this by directly checking the filter logic rather than
        // going through the full execute path (which requires a real API).
        let tools_json = serde_json::json!(["read_file", "delegate_tasks", "browse_url"]);
        let tools: Vec<String> = tools_json
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .filter(|t| t != "delegate_tasks")
            .collect();

        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"read_file".to_string()));
        assert!(tools.contains(&"browse_url".to_string()));
        assert!(!tools.contains(&"delegate_tasks".to_string()));
    }
}
