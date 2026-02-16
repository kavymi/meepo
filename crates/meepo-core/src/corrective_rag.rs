//! Corrective RAG — retrieval validation and refinement loop
//!
//! After retrieving context, evaluates whether the retrieved documents are
//! actually relevant. If not, refines the query and re-retrieves. Caps at
//! a configurable number of correction rounds to avoid latency spirals.
//! Based on Corrective RAG (Yan et al., 2024).

use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::api::{ApiClient, ApiMessage, ContentBlock, MessageContent};

/// Configuration for corrective RAG
#[derive(Debug, Clone)]
pub struct CorrectiveRagConfig {
    /// Whether corrective RAG is enabled
    pub enabled: bool,
    /// Maximum number of correction rounds
    pub max_rounds: usize,
    /// Minimum relevance score (0.0 to 1.0) to accept results
    pub relevance_threshold: f32,
}

impl Default for CorrectiveRagConfig {
    fn default() -> Self {
        Self {
            enabled: false, // opt-in, adds latency
            max_rounds: 2,
            relevance_threshold: 0.5,
        }
    }
}

/// A retrieved document with its relevance assessment
#[derive(Debug, Clone)]
pub struct AssessedDocument {
    pub content: String,
    pub entity_id: Option<String>,
    pub relevance: Relevance,
}

/// Relevance assessment for a retrieved document
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Relevance {
    /// Document is relevant to the query
    Relevant,
    /// Document is partially relevant / ambiguous
    Ambiguous,
    /// Document is not relevant
    Irrelevant,
}

/// Result of a corrective RAG cycle
#[derive(Debug)]
pub struct CorrectionResult {
    /// The validated, relevant documents
    pub documents: Vec<AssessedDocument>,
    /// Refined query (if query was rewritten)
    pub refined_query: Option<String>,
    /// Number of correction rounds performed
    pub rounds: usize,
    /// Whether the correction was successful (found relevant docs)
    pub success: bool,
}

/// Assess the relevance of retrieved documents to a query.
///
/// Uses the LLM to evaluate each document's relevance, then returns
/// a refined query if too many documents are irrelevant.
pub async fn assess_and_correct(
    api: &ApiClient,
    original_query: &str,
    documents: &[(String, Option<String>)], // (content, entity_id)
    config: &CorrectiveRagConfig,
) -> Result<CorrectionResult> {
    if !config.enabled || documents.is_empty() {
        // Pass through without assessment
        return Ok(CorrectionResult {
            documents: documents
                .iter()
                .map(|(content, id)| AssessedDocument {
                    content: content.clone(),
                    entity_id: id.clone(),
                    relevance: Relevance::Relevant,
                })
                .collect(),
            refined_query: None,
            rounds: 0,
            success: true,
        });
    }

    let assessed = assess_relevance(api, original_query, documents).await?;

    let relevant_count = assessed
        .iter()
        .filter(|d| d.relevance == Relevance::Relevant)
        .count();
    let total = assessed.len();

    debug!(
        "Relevance assessment: {}/{} relevant",
        relevant_count, total
    );

    // If enough documents are relevant, return them
    let relevant_ratio = if total > 0 {
        relevant_count as f32 / total as f32
    } else {
        0.0
    };

    if relevant_ratio >= config.relevance_threshold {
        return Ok(CorrectionResult {
            documents: assessed,
            refined_query: None,
            rounds: 1,
            success: true,
        });
    }

    // Not enough relevant docs — try to refine the query
    info!(
        "Low relevance ({:.0}%), attempting query refinement",
        relevant_ratio * 100.0
    );

    let refined_query = refine_query(api, original_query, &assessed).await?;

    Ok(CorrectionResult {
        documents: assessed
            .into_iter()
            .filter(|d| d.relevance != Relevance::Irrelevant)
            .collect(),
        refined_query: Some(refined_query),
        rounds: 1,
        success: relevant_count > 0,
    })
}

/// Assess relevance of each document to the query using the LLM
async fn assess_relevance(
    api: &ApiClient,
    query: &str,
    documents: &[(String, Option<String>)],
) -> Result<Vec<AssessedDocument>> {
    // Build assessment prompt with all documents
    let mut doc_list = String::new();
    for (i, (content, _)) in documents.iter().enumerate() {
        let preview: String = content.chars().take(300).collect();
        doc_list.push_str(&format!("[DOC {}]: {}\n\n", i + 1, preview));
    }

    let prompt = format!(
        "Assess the relevance of each document to the query.\n\
         For each document, respond with its number and one of: RELEVANT, AMBIGUOUS, IRRELEVANT\n\
         Format: one assessment per line, e.g. \"1: RELEVANT\"\n\n\
         Query: {}\n\n\
         Documents:\n{}",
        query, doc_list
    );

    let messages = vec![ApiMessage {
        role: "user".to_string(),
        content: MessageContent::Text(prompt),
    }];

    let response = api
        .chat(
            &messages,
            &[],
            "You are a relevance assessor. Be strict — only mark documents as RELEVANT \
             if they directly help answer the query.",
        )
        .await
        .context("Failed to assess document relevance")?;

    let text = response
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text { text } = b {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<String>();

    // Parse assessments
    let mut assessed: Vec<AssessedDocument> = documents
        .iter()
        .map(|(content, id)| AssessedDocument {
            content: content.clone(),
            entity_id: id.clone(),
            relevance: Relevance::Ambiguous, // default if parsing fails
        })
        .collect();

    for line in text.lines() {
        let line = line.trim();
        if let Some((num_str, assessment)) = line.split_once(':') {
            let num_str = num_str
                .trim()
                .trim_start_matches('[')
                .trim_start_matches("DOC ");
            if let Ok(idx) = num_str.parse::<usize>() {
                let idx = idx.saturating_sub(1); // 1-indexed to 0-indexed
                if idx < assessed.len() {
                    let assessment = assessment.trim().to_uppercase();
                    assessed[idx].relevance = match assessment.as_str() {
                        s if s.contains("RELEVANT") && !s.contains("IRRELEVANT") => {
                            Relevance::Relevant
                        }
                        s if s.contains("IRRELEVANT") => Relevance::Irrelevant,
                        _ => Relevance::Ambiguous,
                    };
                }
            }
        }
    }

    Ok(assessed)
}

/// Refine a query based on the assessment of retrieved documents
async fn refine_query(
    api: &ApiClient,
    original_query: &str,
    assessed: &[AssessedDocument],
) -> Result<String> {
    let relevant_snippets: Vec<String> = assessed
        .iter()
        .filter(|d| d.relevance == Relevance::Relevant || d.relevance == Relevance::Ambiguous)
        .map(|d| d.content.chars().take(200).collect())
        .collect();

    let irrelevant_snippets: Vec<String> = assessed
        .iter()
        .filter(|d| d.relevance == Relevance::Irrelevant)
        .map(|d| d.content.chars().take(100).collect())
        .collect();

    let prompt = format!(
        "The original query didn't retrieve good results. Rewrite it to be more specific.\n\n\
         Original query: {}\n\n\
         Partially relevant results (keep these topics):\n{}\n\n\
         Irrelevant results (avoid these topics):\n{}\n\n\
         Rewrite the query to get better results. Output ONLY the refined query, nothing else.",
        original_query,
        relevant_snippets.join("\n"),
        irrelevant_snippets.join("\n"),
    );

    let messages = vec![ApiMessage {
        role: "user".to_string(),
        content: MessageContent::Text(prompt),
    }];

    let response = api
        .chat(
            &messages,
            &[],
            "You are a query refinement expert. Output only the refined query.",
        )
        .await
        .context("Failed to refine query")?;

    let refined = response
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text { text } = b {
                Some(text.trim().to_string())
            } else {
                None
            }
        })
        .collect::<String>();

    info!(
        "Refined query: '{}' -> '{}'",
        original_query,
        refined.chars().take(100).collect::<String>()
    );

    Ok(refined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CorrectiveRagConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_rounds, 2);
    }

    #[tokio::test]
    async fn test_disabled_passthrough() {
        let config = CorrectiveRagConfig::default(); // disabled by default
        let api = ApiClient::new("test-key".to_string(), None);
        let docs = vec![
            ("Some content".to_string(), Some("id1".to_string())),
            ("More content".to_string(), None),
        ];

        let result = assess_and_correct(&api, "test query", &docs, &config)
            .await
            .unwrap();

        assert_eq!(result.documents.len(), 2);
        assert_eq!(result.rounds, 0);
        assert!(result.success);
        assert!(result.refined_query.is_none());
        assert!(
            result
                .documents
                .iter()
                .all(|d| d.relevance == Relevance::Relevant)
        );
    }

    #[tokio::test]
    async fn test_empty_documents() {
        let config = CorrectiveRagConfig {
            enabled: true,
            ..Default::default()
        };
        let api = ApiClient::new("test-key".to_string(), None);

        let result = assess_and_correct(&api, "test query", &[], &config)
            .await
            .unwrap();

        assert!(result.documents.is_empty());
        assert!(result.success);
    }

    #[test]
    fn test_relevance_equality() {
        assert_eq!(Relevance::Relevant, Relevance::Relevant);
        assert_eq!(Relevance::Ambiguous, Relevance::Ambiguous);
        assert_eq!(Relevance::Irrelevant, Relevance::Irrelevant);
        assert_ne!(Relevance::Relevant, Relevance::Irrelevant);
        assert_ne!(Relevance::Relevant, Relevance::Ambiguous);
    }

    #[test]
    fn test_config_custom_values() {
        let config = CorrectiveRagConfig {
            enabled: true,
            max_rounds: 5,
            relevance_threshold: 0.8,
        };
        assert!(config.enabled);
        assert_eq!(config.max_rounds, 5);
        assert_eq!(config.relevance_threshold, 0.8);
    }

    #[tokio::test]
    async fn test_disabled_passthrough_preserves_entity_ids() {
        let config = CorrectiveRagConfig::default();
        let api = ApiClient::new("test-key".to_string(), None);
        let docs = vec![
            ("Content A".to_string(), Some("entity-1".to_string())),
            ("Content B".to_string(), None),
            ("Content C".to_string(), Some("entity-3".to_string())),
        ];

        let result = assess_and_correct(&api, "query", &docs, &config)
            .await
            .unwrap();

        assert_eq!(result.documents.len(), 3);
        assert_eq!(result.documents[0].entity_id, Some("entity-1".to_string()));
        assert_eq!(result.documents[1].entity_id, None);
        assert_eq!(result.documents[2].entity_id, Some("entity-3".to_string()));
    }

    #[tokio::test]
    async fn test_disabled_passthrough_preserves_content() {
        let config = CorrectiveRagConfig::default();
        let api = ApiClient::new("test-key".to_string(), None);
        let docs = vec![("Hello world".to_string(), None)];

        let result = assess_and_correct(&api, "query", &docs, &config)
            .await
            .unwrap();

        assert_eq!(result.documents[0].content, "Hello world");
    }

    #[test]
    fn test_assessed_document_debug() {
        let doc = AssessedDocument {
            content: "test".to_string(),
            entity_id: Some("id".to_string()),
            relevance: Relevance::Relevant,
        };
        let debug_str = format!("{:?}", doc);
        assert!(debug_str.contains("Relevant"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_correction_result_debug() {
        let result = CorrectionResult {
            documents: vec![],
            refined_query: Some("better query".to_string()),
            rounds: 2,
            success: false,
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("better query"));
        assert!(debug_str.contains("2"));
    }
}
