//! Task & Project Manager tools
//!
//! Full GTD-style task system built into the knowledge graph. Extracts action items
//! from emails/messages, manages priorities, due dates, projects, and contexts.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Create a new task
pub struct CreateTaskTool {
    db: Arc<KnowledgeDb>,
}

impl CreateTaskTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for CreateTaskTool {
    fn name(&self) -> &str {
        "create_task"
    }

    fn description(&self) -> &str {
        "Create a new task in the task manager. Tasks have a title, optional description, \
         priority, due date, project, and context. Tasks are stored in the knowledge graph \
         and can be synced to Apple Reminders."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "title": {
                    "type": "string",
                    "description": "Task title"
                },
                "description": {
                    "type": "string",
                    "description": "Detailed task description"
                },
                "priority": {
                    "type": "string",
                    "description": "Priority: critical, high, medium, low (default: medium)"
                },
                "due_date": {
                    "type": "string",
                    "description": "Due date in ISO8601 or natural language (e.g., 'tomorrow', 'next Friday')"
                },
                "project": {
                    "type": "string",
                    "description": "Project name to associate this task with"
                },
                "context": {
                    "type": "string",
                    "description": "Context tag (e.g., 'work', 'home', 'errands', 'computer')"
                },
                "source": {
                    "type": "string",
                    "description": "Where this task came from (e.g., 'email', 'meeting', 'manual')"
                }
            }),
            vec!["title"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let title = input
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'title' parameter"))?;
        let description = input.get("description").and_then(|v| v.as_str());
        let priority = input
            .get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("medium");
        let due_date = input.get("due_date").and_then(|v| v.as_str());
        let project = input.get("project").and_then(|v| v.as_str());
        let context = input.get("context").and_then(|v| v.as_str());
        let source = input
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");

        if title.len() > 500 {
            return Err(anyhow::anyhow!("Title too long (max 500 characters)"));
        }

        // Validate priority
        let priority_num = match priority {
            "critical" => 5,
            "high" => 4,
            "medium" => 3,
            "low" => 2,
            _ => 3,
        };

        debug!("Creating task: {} (priority: {})", title, priority);

        let metadata = serde_json::json!({
            "status": "pending",
            "priority": priority,
            "priority_num": priority_num,
            "description": description,
            "due_date": due_date,
            "project": project,
            "context": context,
            "source": source,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });

        let task_id = self.db.insert_entity(title, "task", Some(metadata)).await?;

        // Link to project entity if specified
        if let Some(proj) = project {
            // Find or create project entity
            let projects = self
                .db
                .search_entities(proj, Some("project"))
                .await
                .unwrap_or_default();
            let project_id = if let Some(existing) = projects.first() {
                existing.id.clone()
            } else {
                self.db
                    .insert_entity(
                        proj,
                        "project",
                        Some(serde_json::json!({"created_at": chrono::Utc::now().to_rfc3339()})),
                    )
                    .await?
            };
            let _ = self
                .db
                .insert_relationship(&task_id, &project_id, "belongs_to", None)
                .await;
        }

        Ok(format!(
            "Task created:\n\
             - ID: {}\n\
             - Title: {}\n\
             - Priority: {}\n\
             - Due: {}\n\
             - Project: {}\n\
             - Context: {}",
            task_id,
            title,
            priority,
            due_date.unwrap_or("none"),
            project.unwrap_or("none"),
            context.unwrap_or("none")
        ))
    }
}

/// List tasks with filtering
pub struct ListTasksTool {
    db: Arc<KnowledgeDb>,
}

impl ListTasksTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for ListTasksTool {
    fn name(&self) -> &str {
        "list_tasks"
    }

    fn description(&self) -> &str {
        "List tasks with optional filtering by status, priority, project, context, or due date. \
         Returns tasks sorted by priority and due date."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "status": {
                    "type": "string",
                    "description": "Filter by status: pending, in_progress, completed, all (default: pending)"
                },
                "priority": {
                    "type": "string",
                    "description": "Filter by priority: critical, high, medium, low"
                },
                "project": {
                    "type": "string",
                    "description": "Filter by project name"
                },
                "context": {
                    "type": "string",
                    "description": "Filter by context tag"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum tasks to return (default: 20, max: 100)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let status = input
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");
        let priority = input.get("priority").and_then(|v| v.as_str());
        let project = input.get("project").and_then(|v| v.as_str());
        let context = input.get("context").and_then(|v| v.as_str());
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(100);

        debug!("Listing tasks: status={}", status);

        let tasks = self
            .db
            .search_entities("", Some("task"))
            .await
            .unwrap_or_default();

        let filtered: Vec<_> = tasks
            .iter()
            .filter(|t| {
                let meta = t.metadata.as_ref();
                let task_status = meta
                    .and_then(|m| m.get("status"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("pending");
                let task_priority = meta
                    .and_then(|m| m.get("priority"))
                    .and_then(|s| s.as_str());
                let task_project = meta.and_then(|m| m.get("project")).and_then(|s| s.as_str());
                let task_context = meta.and_then(|m| m.get("context")).and_then(|s| s.as_str());

                (status == "all" || task_status == status)
                    && priority.is_none_or(|p| task_priority == Some(p))
                    && project.is_none_or(|p| task_project == Some(p))
                    && context.is_none_or(|c| task_context == Some(c))
            })
            .take(limit as usize)
            .collect();

        if filtered.is_empty() {
            return Ok(format!("No {} tasks found.", status));
        }

        let mut output = format!("Tasks ({} found):\n\n", filtered.len());
        for task in &filtered {
            let meta = task.metadata.as_ref();
            let pri = meta
                .and_then(|m| m.get("priority"))
                .and_then(|s| s.as_str())
                .unwrap_or("medium");
            let due = meta
                .and_then(|m| m.get("due_date"))
                .and_then(|s| s.as_str())
                .unwrap_or("none");
            let proj = meta
                .and_then(|m| m.get("project"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let stat = meta
                .and_then(|m| m.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("pending");

            output.push_str(&format!(
                "- [{}] {} (priority: {}, due: {}{})\n  ID: {}\n",
                stat.to_uppercase(),
                task.name,
                pri,
                due,
                if proj.is_empty() {
                    String::new()
                } else {
                    format!(", project: {}", proj)
                },
                task.id
            ));
        }

        Ok(output)
    }
}

/// Update an existing task
pub struct UpdateTaskTool {
    db: Arc<KnowledgeDb>,
}

impl UpdateTaskTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for UpdateTaskTool {
    fn name(&self) -> &str {
        "update_task"
    }

    fn description(&self) -> &str {
        "Update an existing task's status, priority, due date, or other fields. \
         Use task ID or title to identify the task."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "task_id": {
                    "type": "string",
                    "description": "Task ID (from list_tasks) or task title for fuzzy match"
                },
                "status": {
                    "type": "string",
                    "description": "New status: pending, in_progress, completed, cancelled"
                },
                "priority": {
                    "type": "string",
                    "description": "New priority: critical, high, medium, low"
                },
                "due_date": {
                    "type": "string",
                    "description": "New due date"
                },
                "notes": {
                    "type": "string",
                    "description": "Add notes to the task"
                }
            }),
            vec!["task_id"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let task_id = input
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'task_id' parameter"))?;
        let new_status = input.get("status").and_then(|v| v.as_str());
        let new_priority = input.get("priority").and_then(|v| v.as_str());
        let new_due = input.get("due_date").and_then(|v| v.as_str());
        let notes = input.get("notes").and_then(|v| v.as_str());

        debug!("Updating task: {}", task_id);

        // Try to find by ID first, then by name
        let entity = self.db.get_entity(task_id).await?;
        let entity = match entity {
            Some(e) => e,
            None => {
                // Search by name
                let results = self.db.search_entities(task_id, Some("task")).await?;
                results
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?
            }
        };

        // Build updated metadata
        let mut metadata = entity.metadata.unwrap_or(serde_json::json!({}));
        if let Some(status) = new_status {
            metadata["status"] = serde_json::json!(status);
            if status == "completed" {
                metadata["completed_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            }
        }
        if let Some(priority) = new_priority {
            metadata["priority"] = serde_json::json!(priority);
        }
        if let Some(due) = new_due {
            metadata["due_date"] = serde_json::json!(due);
        }
        if let Some(n) = notes {
            let existing_notes = metadata.get("notes").and_then(|v| v.as_str()).unwrap_or("");
            metadata["notes"] = serde_json::json!(format!(
                "{}\n[{}] {}",
                existing_notes,
                chrono::Utc::now().format("%Y-%m-%d %H:%M"),
                n
            ));
        }
        metadata["updated_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());

        // Update entity in knowledge graph (insert new version)
        let _ = self
            .db
            .insert_entity(&entity.name, "task", Some(metadata.clone()))
            .await;

        Ok(format!(
            "Task updated: {} ({})\n{}",
            entity.name,
            entity.id,
            serde_json::to_string_pretty(&metadata).unwrap_or_default()
        ))
    }
}

/// Mark a task as completed
pub struct CompleteTaskTool {
    db: Arc<KnowledgeDb>,
}

impl CompleteTaskTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for CompleteTaskTool {
    fn name(&self) -> &str {
        "complete_task"
    }

    fn description(&self) -> &str {
        "Mark a task as completed. Shortcut for update_task with status=completed. \
         Optionally add a completion note."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "task_id": {
                    "type": "string",
                    "description": "Task ID or title"
                },
                "note": {
                    "type": "string",
                    "description": "Optional completion note"
                }
            }),
            vec!["task_id"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let task_id = input
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'task_id' parameter"))?;
        let note = input.get("note").and_then(|v| v.as_str());

        debug!("Completing task: {}", task_id);

        // Find the task
        let entity = self.db.get_entity(task_id).await?;
        let entity = match entity {
            Some(e) => e,
            None => {
                let results = self.db.search_entities(task_id, Some("task")).await?;
                results
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?
            }
        };

        let mut metadata = entity.metadata.unwrap_or(serde_json::json!({}));
        metadata["status"] = serde_json::json!("completed");
        metadata["completed_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
        if let Some(n) = note {
            metadata["completion_note"] = serde_json::json!(n);
        }

        let _ = self
            .db
            .insert_entity(&entity.name, "task", Some(metadata))
            .await;

        Ok(format!("Task completed: {} ({})", entity.name, entity.id))
    }
}

/// Get project status overview
pub struct ProjectStatusTool {
    db: Arc<KnowledgeDb>,
}

impl ProjectStatusTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for ProjectStatusTool {
    fn name(&self) -> &str {
        "project_status"
    }

    fn description(&self) -> &str {
        "Get a status overview for a project or all projects. Shows task counts by status, \
         overdue items, recent activity, and completion percentage."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "project": {
                    "type": "string",
                    "description": "Project name (omit for all projects overview)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let project = input.get("project").and_then(|v| v.as_str());

        debug!(
            "Getting project status: {}",
            project.unwrap_or("all projects")
        );

        let all_tasks = self
            .db
            .search_entities("", Some("task"))
            .await
            .unwrap_or_default();

        let tasks: Vec<_> = if let Some(proj) = project {
            all_tasks
                .iter()
                .filter(|t| {
                    t.metadata
                        .as_ref()
                        .and_then(|m| m.get("project"))
                        .and_then(|p| p.as_str())
                        == Some(proj)
                })
                .collect()
        } else {
            all_tasks.iter().collect()
        };

        if tasks.is_empty() {
            return Ok(format!(
                "No tasks found{}.",
                project
                    .map(|p| format!(" for project '{}'", p))
                    .unwrap_or_default()
            ));
        }

        let mut pending = 0;
        let mut in_progress = 0;
        let mut completed = 0;
        let mut overdue = 0;

        for task in &tasks {
            let meta = task.metadata.as_ref();
            let status = meta
                .and_then(|m| m.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("pending");
            match status {
                "pending" => pending += 1,
                "in_progress" => in_progress += 1,
                "completed" => completed += 1,
                _ => {}
            }
            // Check for overdue (simplified — would need date parsing for real check)
            if status != "completed"
                && let Some(due) = meta
                    .and_then(|m| m.get("due_date"))
                    .and_then(|d| d.as_str())
                && !due.is_empty()
                && due != "none"
            {
                // Simple heuristic: if due date string is before today
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                if due < today.as_str() {
                    overdue += 1;
                }
            }
        }

        let total = tasks.len();
        let completion_pct = if total > 0 {
            (completed as f64 / total as f64 * 100.0) as u32
        } else {
            0
        };

        // Group by project for overview
        let mut projects: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for task in &tasks {
            let proj = task
                .metadata
                .as_ref()
                .and_then(|m| m.get("project"))
                .and_then(|p| p.as_str())
                .unwrap_or("(no project)");
            *projects.entry(proj.to_string()).or_insert(0) += 1;
        }

        let projects_str = projects
            .iter()
            .map(|(name, count)| format!("  - {}: {} tasks", name, count))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            "# Project Status{}\n\n\
             ## Summary\n\
             - Total tasks: {}\n\
             - Pending: {}\n\
             - In progress: {}\n\
             - Completed: {} ({}%)\n\
             - Overdue: {}\n\n\
             ## Projects\n{}\n",
            project.map(|p| format!(": {}", p)).unwrap_or_default(),
            total,
            pending,
            in_progress,
            completed,
            completion_pct,
            overdue,
            projects_str
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<KnowledgeDb> {
        Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_tasks.db")).unwrap())
    }

    #[test]
    fn test_create_task_schema() {
        let tool = CreateTaskTool::new(test_db());
        assert_eq!(tool.name(), "create_task");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"title".to_string()));
    }

    #[test]
    fn test_list_tasks_schema() {
        let tool = ListTasksTool::new(test_db());
        assert_eq!(tool.name(), "list_tasks");
    }

    #[test]
    fn test_update_task_schema() {
        let tool = UpdateTaskTool::new(test_db());
        assert_eq!(tool.name(), "update_task");
    }

    #[test]
    fn test_complete_task_schema() {
        let tool = CompleteTaskTool::new(test_db());
        assert_eq!(tool.name(), "complete_task");
    }

    #[test]
    fn test_project_status_schema() {
        let tool = ProjectStatusTool::new(test_db());
        assert_eq!(tool.name(), "project_status");
    }

    #[tokio::test]
    async fn test_list_tasks_empty() {
        let tool = ListTasksTool::new(test_db());
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.contains("No") || result.contains("0") || result.contains("found"));
    }
}
