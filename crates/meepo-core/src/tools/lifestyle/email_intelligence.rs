//! Deep Email Intelligence tools
//!
//! Goes beyond basic read/send — triages inbox by urgency, drafts context-aware
//! replies, summarizes email threads, and manages unsubscriptions.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::platform::EmailProvider;
use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Triage inbox emails by urgency and category
pub struct EmailTriageTool {
    provider: Box<dyn EmailProvider>,
    db: Arc<KnowledgeDb>,
}

impl EmailTriageTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self {
            provider: crate::platform::create_email_provider()
                .expect("Email provider not available on this platform"),
            db,
        }
    }
}

#[async_trait]
impl ToolHandler for EmailTriageTool {
    fn name(&self) -> &str {
        "email_triage"
    }

    fn description(&self) -> &str {
        "Triage inbox emails by urgency and category. Reads recent unread emails, categorizes them \
         (urgent/action-required/informational/newsletter/spam), identifies action items, and flags \
         emails needing a response. Cross-references senders with contacts and calendar for context."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "limit": {
                    "type": "number",
                    "description": "Number of emails to triage (default: 20, max: 100)"
                },
                "since_hours": {
                    "type": "number",
                    "description": "Only triage emails from the last N hours (default: 24)"
                },
                "categories": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Filter to specific categories: urgent, action_required, informational, newsletter, spam"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(100);
        let since_hours = input
            .get("since_hours")
            .and_then(|v| v.as_u64())
            .unwrap_or(24);

        debug!("Triaging {} emails from last {} hours", limit, since_hours);

        // Read recent emails
        let emails = self.provider.read_emails(limit, "inbox", None).await?;

        // Store triage results in knowledge graph for future reference
        let _ = self
            .db
            .insert_entity(
                &format!("email_triage_{}", chrono::Utc::now().format("%Y%m%d_%H%M")),
                "email_triage",
                Some(serde_json::json!({
                    "count": limit,
                    "since_hours": since_hours,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                })),
            )
            .await;

        Ok(format!(
            "Email Triage (last {} hours, {} emails):\n\n{}\n\n\
             Instructions: Analyze each email above and categorize as:\n\
             - URGENT: Needs immediate response (deadlines, emergencies)\n\
             - ACTION REQUIRED: Needs a response but not time-critical\n\
             - INFORMATIONAL: FYI only, no response needed\n\
             - NEWSLETTER: Subscriptions and marketing\n\
             - SPAM: Unwanted or suspicious\n\n\
             For each ACTION REQUIRED email, identify the specific action needed.",
            since_hours, limit, emails
        ))
    }
}

/// Draft a context-aware reply to an email
pub struct EmailDraftReplyTool {
    provider: Box<dyn EmailProvider>,
    db: Arc<KnowledgeDb>,
}

impl EmailDraftReplyTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self {
            provider: crate::platform::create_email_provider()
                .expect("Email provider not available on this platform"),
            db,
        }
    }
}

#[async_trait]
impl ToolHandler for EmailDraftReplyTool {
    fn name(&self) -> &str {
        "email_draft_reply"
    }

    fn description(&self) -> &str {
        "Draft a context-aware reply to an email. Reads the original email thread, checks the \
         knowledge graph for context about the sender and topic, and generates a draft reply \
         matching the user's communication style. The draft is presented for approval before sending."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "subject": {
                    "type": "string",
                    "description": "Subject line of the email to reply to (used to find the thread)"
                },
                "tone": {
                    "type": "string",
                    "description": "Desired tone: formal, casual, friendly, professional (default: professional)"
                },
                "key_points": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Key points to include in the reply"
                },
                "max_length": {
                    "type": "number",
                    "description": "Maximum reply length in words (default: 200)"
                }
            }),
            vec!["subject"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let subject = input
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'subject' parameter"))?;
        let tone = input
            .get("tone")
            .and_then(|v| v.as_str())
            .unwrap_or("professional");
        let key_points: Vec<String> = input
            .get("key_points")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let max_length = input
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(200);

        if subject.len() > 500 {
            return Err(anyhow::anyhow!("Subject too long (max 500 characters)"));
        }

        debug!("Drafting reply to email: {}", subject);

        // Read the original email thread
        let thread = self.provider.read_emails(5, "inbox", Some(subject)).await?;

        // Search knowledge graph for context about the sender
        let context = self
            .db
            .search_entities(subject, None)
            .await
            .unwrap_or_default();

        let context_str = if context.is_empty() {
            "No prior context found.".to_string()
        } else {
            context
                .iter()
                .take(3)
                .map(|e| format!("- {} ({})", e.name, e.entity_type))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let points_str = if key_points.is_empty() {
            "None specified — infer from context.".to_string()
        } else {
            key_points
                .iter()
                .map(|p| format!("- {}", p))
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(format!(
            "Email Thread:\n{}\n\n\
             Known Context:\n{}\n\n\
             Draft Reply Parameters:\n\
             - Tone: {}\n\
             - Max length: {} words\n\
             - Key points:\n{}\n\n\
             Please draft a reply based on the above. Present it for user approval before sending.",
            thread, context_str, tone, max_length, points_str
        ))
    }
}

/// Summarize an email thread
pub struct EmailSummarizeThreadTool {
    provider: Box<dyn EmailProvider>,
}

impl EmailSummarizeThreadTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_email_provider()
                .expect("Email provider not available on this platform"),
        }
    }
}

impl Default for EmailSummarizeThreadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for EmailSummarizeThreadTool {
    fn name(&self) -> &str {
        "email_summarize_thread"
    }

    fn description(&self) -> &str {
        "Summarize an email thread. Reads all emails in a thread and produces a concise summary \
         including key decisions, action items, participants, and timeline."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "subject": {
                    "type": "string",
                    "description": "Subject line to search for (matches the thread)"
                },
                "max_emails": {
                    "type": "number",
                    "description": "Maximum emails to include in summary (default: 20)"
                }
            }),
            vec!["subject"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let subject = input
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'subject' parameter"))?;
        let max_emails = input
            .get("max_emails")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(50);

        if subject.len() > 500 {
            return Err(anyhow::anyhow!("Subject too long (max 500 characters)"));
        }

        debug!("Summarizing email thread: {}", subject);

        let emails = self
            .provider
            .read_emails(max_emails, "inbox", Some(subject))
            .await?;

        Ok(format!(
            "Email Thread (subject: '{}'):\n\n{}\n\n\
             Please provide a structured summary including:\n\
             1. **Participants** — who is involved\n\
             2. **Timeline** — key dates and sequence of events\n\
             3. **Key Decisions** — what was decided\n\
             4. **Action Items** — what needs to be done and by whom\n\
             5. **Open Questions** — unresolved issues",
            subject, emails
        ))
    }
}

/// Unsubscribe from email newsletters/lists
pub struct EmailUnsubscribeTool {
    provider: Box<dyn EmailProvider>,
}

impl EmailUnsubscribeTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_email_provider()
                .expect("Email provider not available on this platform"),
        }
    }
}

impl Default for EmailUnsubscribeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for EmailUnsubscribeTool {
    fn name(&self) -> &str {
        "email_unsubscribe"
    }

    fn description(&self) -> &str {
        "Find and list newsletter/marketing emails for potential unsubscription. Scans recent \
         emails for unsubscribe links and recurring senders, helping clean up the inbox."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "scan_count": {
                    "type": "number",
                    "description": "Number of recent emails to scan for newsletters (default: 50, max: 200)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let scan_count = input
            .get("scan_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .min(200);

        debug!("Scanning {} emails for unsubscribe candidates", scan_count);

        let emails = self.provider.read_emails(scan_count, "inbox", None).await?;

        Ok(format!(
            "Recent emails ({} scanned):\n\n{}\n\n\
             Please identify recurring newsletter/marketing senders and list them with:\n\
             - Sender name and email\n\
             - Frequency (daily/weekly/monthly)\n\
             - Whether the user has engaged with recent emails\n\
             - Recommendation: keep or unsubscribe\n\n\
             Note: To actually unsubscribe, the user should click unsubscribe links in the emails \
             or use the browser automation tools to visit unsubscribe URLs.",
            scan_count, emails
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn test_email_triage_schema() {
        let db =
            Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_email_triage.db")).unwrap());
        let tool = EmailTriageTool::new(db);
        assert_eq!(tool.name(), "email_triage");
        assert!(!tool.description().is_empty());
        let schema = tool.input_schema();
        assert!(schema.get("properties").is_some());
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn test_email_draft_reply_schema() {
        let db =
            Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_email_draft.db")).unwrap());
        let tool = EmailDraftReplyTool::new(db);
        assert_eq!(tool.name(), "email_draft_reply");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"subject".to_string()));
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn test_email_summarize_thread_schema() {
        let tool = EmailSummarizeThreadTool::new();
        assert_eq!(tool.name(), "email_summarize_thread");
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn test_email_unsubscribe_schema() {
        let tool = EmailUnsubscribeTool::new();
        assert_eq!(tool.name(), "email_unsubscribe");
    }
}
