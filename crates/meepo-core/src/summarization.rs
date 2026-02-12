//! Conversation history summarization
//!
//! Automatically summarizes older conversation history when approaching
//! context window limits, keeping recent messages intact. Inspired by
//! LangChain v1's SummarizationMiddleware.

use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::api::{ApiClient, ApiMessage, ContentBlock, MessageContent};

/// Configuration for conversation summarization
#[derive(Debug, Clone)]
pub struct SummarizationConfig {
    /// Maximum number of characters in context before triggering summarization
    pub trigger_chars: usize,
    /// Number of recent messages to always keep verbatim
    pub keep_recent: usize,
    /// Model to use for summarization (can be a cheaper/faster model)
    pub model: Option<String>,
    /// Whether summarization is enabled
    pub enabled: bool,
}

impl Default for SummarizationConfig {
    fn default() -> Self {
        Self {
            trigger_chars: 60_000,
            keep_recent: 10,
            model: None, // use same model as agent
            enabled: true,
        }
    }
}

/// Result of a summarization attempt
#[derive(Debug)]
pub struct SummarizationResult {
    /// The summary text (None if summarization was not needed)
    pub summary: Option<String>,
    /// Number of conversations that were summarized
    pub summarized_count: usize,
    /// Number of conversations kept verbatim
    pub kept_count: usize,
}

/// Summarize older conversation history into a concise summary.
///
/// Takes a list of conversation entries (sender: content pairs) and returns
/// a summary of the older messages plus the recent messages kept verbatim.
///
/// Returns `(summary_text, recent_messages)` where `summary_text` is the
/// condensed version of older messages.
pub async fn summarize_conversations(
    api: &ApiClient,
    conversations: &[(String, String)], // (sender, content) pairs
    config: &SummarizationConfig,
) -> Result<SummarizationResult> {
    // Calculate total size
    let total_chars: usize = conversations
        .iter()
        .map(|(s, c)| s.len() + c.len() + 3)
        .sum();

    // Check if summarization is needed
    if !config.enabled || total_chars < config.trigger_chars || conversations.len() <= config.keep_recent {
        debug!(
            "Summarization not needed (total_chars={}, threshold={}, count={})",
            total_chars, config.trigger_chars, conversations.len()
        );
        return Ok(SummarizationResult {
            summary: None,
            summarized_count: 0,
            kept_count: conversations.len(),
        });
    }

    let split_point = conversations.len().saturating_sub(config.keep_recent);
    let older = &conversations[..split_point];

    if older.is_empty() {
        return Ok(SummarizationResult {
            summary: None,
            summarized_count: 0,
            kept_count: conversations.len(),
        });
    }

    info!(
        "Summarizing {} older conversations (keeping {} recent)",
        older.len(),
        config.keep_recent
    );

    // Build the conversation text to summarize
    let mut text_to_summarize = String::new();
    for (sender, content) in older {
        text_to_summarize.push_str(&format!("{}: {}\n", sender, content));
    }

    // Ask Claude to summarize
    let summarization_prompt = format!(
        "Summarize the following conversation history into a concise summary. \
         Preserve key facts, decisions, action items, and important context. \
         Keep entity names, dates, and specific details. Be concise but thorough.\n\n\
         Conversation:\n{}\n\n\
         Provide a structured summary with sections for: Key Topics, Decisions/Actions, \
         and Important Context.",
        text_to_summarize
    );

    let messages = vec![ApiMessage {
        role: "user".to_string(),
        content: MessageContent::Text(summarization_prompt),
    }];

    let system = "You are a conversation summarizer. Produce concise, structured summaries \
                  that preserve all important information. Output only the summary, no preamble.";

    let response = api
        .chat(&messages, &[], system)
        .await
        .context("Failed to generate conversation summary")?;

    // Extract text from response
    let summary = response
        .content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text { text } = block {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    info!(
        "Generated summary ({} chars) from {} conversations",
        summary.len(),
        older.len()
    );

    Ok(SummarizationResult {
        summary: Some(summary),
        summarized_count: older.len(),
        kept_count: conversations.len() - older.len(),
    })
}

/// Build context string from conversations, applying summarization if needed.
///
/// Returns a context string with an optional summary section followed by
/// recent verbatim messages.
pub async fn build_summarized_context(
    api: &ApiClient,
    conversations: &[(String, String)],
    config: &SummarizationConfig,
) -> Result<String> {
    let result = summarize_conversations(api, conversations, config).await?;

    let mut context = String::new();

    if let Some(summary) = &result.summary {
        context.push_str("## Conversation Summary (older messages)\n\n");
        context.push_str(summary);
        context.push_str("\n\n");
    }

    // Add recent messages verbatim
    let keep_start = conversations
        .len()
        .saturating_sub(config.keep_recent);
    let recent = &conversations[keep_start..];

    if !recent.is_empty() {
        context.push_str("## Recent Conversation\n\n");
        for (sender, content) in recent {
            context.push_str(&format!("{}: {}\n", sender, content));
        }
        context.push('\n');
    }

    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SummarizationConfig::default();
        assert!(config.enabled);
        assert_eq!(config.keep_recent, 10);
        assert_eq!(config.trigger_chars, 60_000);
    }

    #[test]
    fn test_summarization_not_needed_small() {
        let config = SummarizationConfig::default();
        let conversations: Vec<(String, String)> = vec![
            ("user".to_string(), "Hello".to_string()),
            ("meepo".to_string(), "Hi there!".to_string()),
        ];

        let rt = tokio::runtime::Runtime::new().unwrap();
        let api = ApiClient::new("test-key".to_string(), None);
        let result = rt
            .block_on(summarize_conversations(&api, &conversations, &config))
            .unwrap();

        assert!(result.summary.is_none());
        assert_eq!(result.summarized_count, 0);
        assert_eq!(result.kept_count, 2);
    }

    #[test]
    fn test_summarization_disabled() {
        let config = SummarizationConfig {
            enabled: false,
            ..Default::default()
        };
        let conversations: Vec<(String, String)> = (0..100)
            .map(|i| ("user".to_string(), format!("Message {}", i)))
            .collect();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let api = ApiClient::new("test-key".to_string(), None);
        let result = rt
            .block_on(summarize_conversations(&api, &conversations, &config))
            .unwrap();

        assert!(result.summary.is_none());
    }
}
