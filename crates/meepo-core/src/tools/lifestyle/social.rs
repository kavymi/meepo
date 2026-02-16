//! Social & Relationship Manager tools
//!
//! Track relationships, birthdays, follow-up commitments, and contact frequency.
//! Proactively remind about people you haven't connected with recently.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Get relationship summary for a contact or all contacts
pub struct RelationshipSummaryTool {
    db: Arc<KnowledgeDb>,
}

impl RelationshipSummaryTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for RelationshipSummaryTool {
    fn name(&self) -> &str {
        "relationship_summary"
    }

    fn description(&self) -> &str {
        "Get a relationship summary for a contact or overview of all tracked relationships. \
         Shows last contact date, communication frequency, upcoming birthdays, and any \
         follow-up commitments. Uses the knowledge graph to track interactions across \
         email, messages, and calendar."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "contact": {
                    "type": "string",
                    "description": "Contact name to get details for (omit for overview of all contacts)"
                },
                "include_history": {
                    "type": "boolean",
                    "description": "Include recent interaction history (default: true)"
                },
                "days_inactive": {
                    "type": "number",
                    "description": "Flag contacts not contacted in this many days (default: 30)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let contact = input.get("contact").and_then(|v| v.as_str());
        let include_history = input
            .get("include_history")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let days_inactive = input
            .get("days_inactive")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        debug!(
            "Getting relationship summary: {}",
            contact.unwrap_or("all contacts")
        );

        // Get contact entities from knowledge graph
        let contacts = if let Some(name) = contact {
            self.db
                .search_entities(name, Some("contact"))
                .await
                .unwrap_or_default()
        } else {
            self.db
                .search_entities("", Some("contact"))
                .await
                .unwrap_or_default()
        };

        // Also check conversations for contact activity
        let conversations = self
            .db
            .get_recent_conversations(None, 100)
            .await
            .unwrap_or_default();

        // Build contact activity map from conversations
        let mut last_contact: std::collections::HashMap<String, chrono::DateTime<chrono::Utc>> =
            std::collections::HashMap::new();
        let mut contact_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for conv in &conversations {
            let sender = &conv.sender;
            if let Some(name) = contact {
                if !sender.to_lowercase().contains(&name.to_lowercase()) {
                    continue;
                }
            }
            let entry = last_contact
                .entry(sender.clone())
                .or_insert(conv.created_at);
            if conv.created_at > *entry {
                *entry = conv.created_at;
            }
            *contact_count.entry(sender.clone()).or_insert(0) += 1;
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(days_inactive as i64);

        if contact.is_some() && contacts.is_empty() && last_contact.is_empty() {
            return Ok(format!(
                "No information found for '{}'. The contact may not be tracked yet. \
                 Interactions are automatically tracked from conversations.",
                contact.unwrap()
            ));
        }

        let mut output = String::from("# Relationship Summary\n\n");

        // Show tracked contact entities
        if !contacts.is_empty() {
            for c in &contacts {
                let meta = c.metadata.as_ref();
                let birthday = meta
                    .and_then(|m| m.get("birthday"))
                    .and_then(|b| b.as_str())
                    .unwrap_or("unknown");
                let email = meta
                    .and_then(|m| m.get("email"))
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown");
                let phone = meta
                    .and_then(|m| m.get("phone"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("unknown");
                let notes = meta
                    .and_then(|m| m.get("notes"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");

                output.push_str(&format!(
                    "## {}\n\
                     - Email: {}\n\
                     - Phone: {}\n\
                     - Birthday: {}\n",
                    c.name, email, phone, birthday
                ));

                if !notes.is_empty() {
                    output.push_str(&format!("- Notes: {}\n", notes));
                }

                // Check last interaction
                if let Some(last) = last_contact.get(&c.name) {
                    let days_ago = (chrono::Utc::now() - *last).num_days();
                    let count = contact_count.get(&c.name).unwrap_or(&0);
                    output.push_str(&format!(
                        "- Last contact: {} days ago ({} interactions)\n",
                        days_ago, count
                    ));
                    if *last < cutoff {
                        output.push_str("- ⚠ INACTIVE — consider reaching out\n");
                    }
                }

                output.push('\n');
            }
        }

        // Show conversation-based contacts not in the entity list
        if include_history && contact.is_none() {
            let entity_names: std::collections::HashSet<_> =
                contacts.iter().map(|c| c.name.clone()).collect();

            let mut inactive_contacts = Vec::new();
            for (name, last) in &last_contact {
                if !entity_names.contains(name) {
                    let days_ago = (chrono::Utc::now() - *last).num_days();
                    let count = contact_count.get(name).unwrap_or(&0);
                    if *last < cutoff {
                        inactive_contacts.push(format!(
                            "- {} — last contact {} days ago ({} interactions)",
                            name, days_ago, count
                        ));
                    }
                }
            }

            if !inactive_contacts.is_empty() {
                output.push_str(&format!(
                    "## Contacts Not Reached in {} Days\n{}\n\n",
                    days_inactive,
                    inactive_contacts.join("\n")
                ));
            }
        }

        // Show follow-up commitments
        let followups = self
            .db
            .search_entities("followup:", Some("followup"))
            .await
            .unwrap_or_default();

        if !followups.is_empty() {
            output.push_str("## Pending Follow-ups\n");
            for fu in followups.iter().take(10) {
                let meta = fu.metadata.as_ref();
                let person = meta
                    .and_then(|m| m.get("person"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("unknown");
                let reason = meta
                    .and_then(|m| m.get("reason"))
                    .and_then(|r| r.as_str())
                    .unwrap_or("");
                output.push_str(&format!("- {} — {}\n", person, reason));
            }
            output.push('\n');
        }

        Ok(output)
    }
}

/// Suggest follow-ups based on relationship data
pub struct SuggestFollowupsTool {
    db: Arc<KnowledgeDb>,
}

impl SuggestFollowupsTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for SuggestFollowupsTool {
    fn name(&self) -> &str {
        "suggest_followups"
    }

    fn description(&self) -> &str {
        "Suggest people to follow up with based on relationship data. Identifies contacts \
         you haven't reached out to recently, upcoming birthdays, and commitments you made \
         in conversations. Can also record a new follow-up commitment."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "action": {
                    "type": "string",
                    "description": "Action: suggest (get suggestions), add (record a follow-up), complete (mark done). Default: suggest"
                },
                "person": {
                    "type": "string",
                    "description": "Person name (required for 'add' and 'complete' actions)"
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for follow-up (for 'add' action)"
                },
                "due_date": {
                    "type": "string",
                    "description": "When to follow up by (for 'add' action)"
                },
                "max_suggestions": {
                    "type": "number",
                    "description": "Maximum suggestions to return (default: 5)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("suggest");
        let person = input.get("person").and_then(|v| v.as_str());
        let reason = input.get("reason").and_then(|v| v.as_str());
        let due_date = input.get("due_date").and_then(|v| v.as_str());
        let max_suggestions = input
            .get("max_suggestions")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        debug!("Follow-up action: {}", action);

        match action {
            "add" => {
                let person =
                    person.ok_or_else(|| anyhow::anyhow!("Missing 'person' for add action"))?;
                let reason = reason.unwrap_or("General follow-up");

                let _ = self
                    .db
                    .insert_entity(
                        &format!("followup:{}", person),
                        "followup",
                        Some(serde_json::json!({
                            "person": person,
                            "reason": reason,
                            "due_date": due_date,
                            "status": "pending",
                            "created_at": chrono::Utc::now().to_rfc3339(),
                        })),
                    )
                    .await?;

                Ok(format!(
                    "Follow-up recorded:\n- Person: {}\n- Reason: {}\n- Due: {}",
                    person,
                    reason,
                    due_date.unwrap_or("no deadline")
                ))
            }
            "complete" => {
                let person = person
                    .ok_or_else(|| anyhow::anyhow!("Missing 'person' for complete action"))?;

                let followups = self
                    .db
                    .search_entities(&format!("followup:{}", person), Some("followup"))
                    .await
                    .unwrap_or_default();

                if followups.is_empty() {
                    return Ok(format!("No pending follow-up found for '{}'.", person));
                }

                for fu in &followups {
                    let mut meta = fu.metadata.clone().unwrap_or(serde_json::json!({}));
                    meta["status"] = serde_json::json!("completed");
                    meta["completed_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
                    let _ = self
                        .db
                        .insert_entity(&fu.name, "followup", Some(meta))
                        .await;
                }

                Ok(format!("Follow-up with {} marked as completed.", person))
            }
            _ => {
                // Suggest mode
                let conversations = self
                    .db
                    .get_recent_conversations(None, 200)
                    .await
                    .unwrap_or_default();

                // Find contacts with oldest last interaction
                let mut last_contact: std::collections::HashMap<
                    String,
                    chrono::DateTime<chrono::Utc>,
                > = std::collections::HashMap::new();
                for conv in &conversations {
                    let entry = last_contact
                        .entry(conv.sender.clone())
                        .or_insert(conv.created_at);
                    if conv.created_at > *entry {
                        *entry = conv.created_at;
                    }
                }

                let mut sorted: Vec<_> = last_contact.iter().collect();
                sorted.sort_by_key(|(_, date)| *date);

                let mut output = String::from("# Follow-up Suggestions\n\n");

                // Pending follow-ups first
                let followups = self
                    .db
                    .search_entities("followup:", Some("followup"))
                    .await
                    .unwrap_or_default();

                let pending: Vec<_> = followups
                    .iter()
                    .filter(|f| {
                        f.metadata
                            .as_ref()
                            .and_then(|m| m.get("status"))
                            .and_then(|s| s.as_str())
                            != Some("completed")
                    })
                    .collect();

                if !pending.is_empty() {
                    output.push_str("## Committed Follow-ups\n");
                    for fu in pending.iter().take(max_suggestions as usize) {
                        let meta = fu.metadata.as_ref();
                        let person = meta
                            .and_then(|m| m.get("person"))
                            .and_then(|p| p.as_str())
                            .unwrap_or("unknown");
                        let reason = meta
                            .and_then(|m| m.get("reason"))
                            .and_then(|r| r.as_str())
                            .unwrap_or("");
                        let due = meta
                            .and_then(|m| m.get("due_date"))
                            .and_then(|d| d.as_str())
                            .unwrap_or("no deadline");
                        output.push_str(&format!("- {} — {} (due: {})\n", person, reason, due));
                    }
                    output.push('\n');
                }

                // Contacts not reached recently
                output.push_str("## Haven't Connected Recently\n");
                for (name, last) in sorted.iter().take(max_suggestions as usize) {
                    let days_ago = (chrono::Utc::now() - **last).num_days();
                    output.push_str(&format!("- {} — {} days ago\n", name, days_ago));
                }

                // Upcoming birthdays from contacts
                let contacts = self
                    .db
                    .search_entities("", Some("contact"))
                    .await
                    .unwrap_or_default();

                let birthdays: Vec<_> = contacts
                    .iter()
                    .filter_map(|c| {
                        let bday = c.metadata.as_ref()?.get("birthday")?.as_str()?;
                        if !bday.is_empty() && bday != "unknown" {
                            Some(format!("- {} — {}", c.name, bday))
                        } else {
                            None
                        }
                    })
                    .collect();

                if !birthdays.is_empty() {
                    output.push_str("\n## Birthdays\n");
                    for b in birthdays.iter().take(5) {
                        output.push_str(&format!("{}\n", b));
                    }
                }

                Ok(output)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<KnowledgeDb> {
        Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_social.db")).unwrap())
    }

    #[test]
    fn test_relationship_summary_schema() {
        let tool = RelationshipSummaryTool::new(test_db());
        assert_eq!(tool.name(), "relationship_summary");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_suggest_followups_schema() {
        let tool = SuggestFollowupsTool::new(test_db());
        assert_eq!(tool.name(), "suggest_followups");
    }

    #[tokio::test]
    async fn test_suggest_followups_empty() {
        let tool = SuggestFollowupsTool::new(test_db());
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.contains("Follow-up") || result.contains("Suggest"));
    }
}
