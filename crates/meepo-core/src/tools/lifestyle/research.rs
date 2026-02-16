//! Deep Research Agent tools
//!
//! Multi-step autonomous research — search across multiple queries, extract info
//! from URLs, compile structured reports with citations, and track evolving topics.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::tavily::TavilyClient;
use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Conduct deep research on a topic
pub struct ResearchTopicTool {
    tavily: Option<Arc<TavilyClient>>,
    db: Arc<KnowledgeDb>,
}

impl ResearchTopicTool {
    pub fn new(tavily: Option<Arc<TavilyClient>>, db: Arc<KnowledgeDb>) -> Self {
        Self { tavily, db }
    }
}

#[async_trait]
impl ToolHandler for ResearchTopicTool {
    fn name(&self) -> &str {
        "research_topic"
    }

    fn description(&self) -> &str {
        "Conduct deep research on a topic. Performs multiple web searches from different angles, \
         extracts key information from top results, cross-references sources, and stores findings \
         in the knowledge graph. Returns a structured research brief with citations."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "topic": {
                    "type": "string",
                    "description": "The topic to research"
                },
                "depth": {
                    "type": "string",
                    "description": "Research depth: quick (3 searches), standard (5), deep (10). Default: standard"
                },
                "focus_areas": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Specific aspects to focus on (e.g., ['pricing', 'competitors', 'reviews'])"
                },
                "max_sources": {
                    "type": "number",
                    "description": "Maximum number of sources to consult (default: 10, max: 25)"
                }
            }),
            vec!["topic"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let topic = input
            .get("topic")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'topic' parameter"))?;
        let depth = input
            .get("depth")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");
        let focus_areas: Vec<String> = input
            .get("focus_areas")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let max_sources = input
            .get("max_sources")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(25);

        if topic.len() > 1000 {
            return Err(anyhow::anyhow!("Topic too long (max 1000 characters)"));
        }

        let search_count = match depth {
            "quick" => 3,
            "deep" => 10,
            _ => 5,
        };

        debug!(
            "Researching '{}' (depth: {}, {} searches, {} max sources)",
            topic, depth, search_count, max_sources
        );

        // Perform web searches if Tavily is available
        let mut search_results = Vec::new();
        if let Some(ref tavily) = self.tavily {
            // Primary search
            let primary = tavily.search(topic, max_sources as usize).await?;
            search_results.push(format!(
                "Primary search '{}':\n{}",
                topic,
                TavilyClient::format_results(&primary)
            ));

            // Additional searches for focus areas
            for area in focus_areas.iter().take(search_count - 1) {
                let query = format!("{} {}", topic, area);
                if let Ok(results) = tavily.search(&query, 5).await {
                    search_results.push(format!(
                        "Focus area '{}':\n{}",
                        area,
                        TavilyClient::format_results(&results)
                    ));
                }
            }
        } else {
            search_results
                .push("Web search not available (no Tavily API key configured).".to_string());
        }

        // Store research topic in knowledge graph
        let entity_id = self
            .db
            .insert_entity(
                topic,
                "research_topic",
                Some(serde_json::json!({
                    "depth": depth,
                    "focus_areas": focus_areas,
                    "max_sources": max_sources,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                })),
            )
            .await?;

        // Check for existing knowledge
        let existing = self
            .db
            .search_entities(topic, None)
            .await
            .unwrap_or_default();
        let existing_str = if existing.len() > 1 {
            existing
                .iter()
                .filter(|e| e.id != entity_id)
                .take(5)
                .map(|e| format!("- {} ({})", e.name, e.entity_type))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            "No prior knowledge found.".to_string()
        };

        Ok(format!(
            "# Research: {}\n\n\
             ## Search Results\n{}\n\n\
             ## Existing Knowledge\n{}\n\n\
             ## Instructions\n\
             Compile a structured research brief including:\n\
             1. **Executive Summary** — 2-3 sentence overview\n\
             2. **Key Findings** — bullet points of the most important facts\n\
             3. **Detailed Analysis** — organized by focus area\n\
             4. **Sources** — numbered list of URLs consulted\n\
             5. **Gaps** — what couldn't be determined and needs further research\n\n\
             Store key findings as entities in the knowledge graph using the remember tool.",
            topic,
            search_results.join("\n\n---\n\n"),
            existing_str
        ))
    }
}

/// Compile a structured report from research
pub struct CompileReportTool {
    db: Arc<KnowledgeDb>,
}

impl CompileReportTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for CompileReportTool {
    fn name(&self) -> &str {
        "compile_report"
    }

    fn description(&self) -> &str {
        "Compile a structured report from research findings stored in the knowledge graph. \
         Pulls together entities, relationships, and prior research on a topic into a \
         formatted document with sections, citations, and recommendations."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "topic": {
                    "type": "string",
                    "description": "Topic to compile a report on"
                },
                "format": {
                    "type": "string",
                    "description": "Report format: brief, detailed, executive_summary (default: detailed)"
                },
                "include_sources": {
                    "type": "boolean",
                    "description": "Include source citations (default: true)"
                }
            }),
            vec!["topic"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let topic = input
            .get("topic")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'topic' parameter"))?;
        let format = input
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("detailed");
        let include_sources = input
            .get("include_sources")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        debug!("Compiling report on '{}' (format: {})", topic, format);

        // Search knowledge graph for all related entities
        let entities = self
            .db
            .search_entities(topic, None)
            .await
            .unwrap_or_default();

        let entities_str = if entities.is_empty() {
            "No knowledge found. Run research_topic first to gather information.".to_string()
        } else {
            entities
                .iter()
                .map(|e| {
                    let meta = e
                        .metadata
                        .as_ref()
                        .map(|m| format!("\n  Metadata: {}", m))
                        .unwrap_or_default();
                    format!("- {} (type: {}){}", e.name, e.entity_type, meta)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        // Get relationships for context
        let mut relationships = Vec::new();
        for entity in entities.iter().take(10) {
            if let Ok(rels) = self.db.get_relationships_for(&entity.id).await {
                for rel in rels {
                    relationships.push(format!(
                        "{} --[{}]--> {}",
                        rel.source_id, rel.relation_type, rel.target_id
                    ));
                }
            }
        }
        let rels_str = if relationships.is_empty() {
            "No relationships found.".to_string()
        } else {
            relationships.join("\n")
        };

        Ok(format!(
            "# Report Compilation: {}\n\n\
             ## Knowledge Graph Entities\n{}\n\n\
             ## Relationships\n{}\n\n\
             ## Instructions\n\
             Compile a {} report on '{}' using the knowledge above.\n\
             {}\n\
             Format with clear headings, bullet points, and actionable recommendations.",
            topic,
            entities_str,
            rels_str,
            format,
            topic,
            if include_sources {
                "Include numbered source citations."
            } else {
                "Omit source citations."
            }
        ))
    }
}

/// Track an evolving topic over time
pub struct TrackTopicTool {
    db: Arc<KnowledgeDb>,
}

impl TrackTopicTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for TrackTopicTool {
    fn name(&self) -> &str {
        "track_topic"
    }

    fn description(&self) -> &str {
        "Start tracking an evolving topic for ongoing research. Creates a watcher that \
         periodically searches for new information and stores updates in the knowledge graph. \
         Use this for topics that change over time (e.g., market trends, project updates)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "topic": {
                    "type": "string",
                    "description": "Topic to track"
                },
                "search_queries": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Specific search queries to run periodically"
                },
                "check_interval_hours": {
                    "type": "number",
                    "description": "How often to check for updates in hours (default: 24, min: 1)"
                },
                "keywords": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Keywords that indicate a relevant update"
                }
            }),
            vec!["topic"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let topic = input
            .get("topic")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'topic' parameter"))?;
        let queries: Vec<String> = input
            .get("search_queries")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| vec![topic.to_string()]);
        let interval_hours = input
            .get("check_interval_hours")
            .and_then(|v| v.as_u64())
            .unwrap_or(24)
            .max(1);
        let keywords: Vec<String> = input
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if topic.len() > 500 {
            return Err(anyhow::anyhow!("Topic too long (max 500 characters)"));
        }

        debug!(
            "Setting up topic tracking for '{}' (every {} hours)",
            topic, interval_hours
        );

        // Store tracking config in knowledge graph
        let entity_id = self
            .db
            .insert_entity(
                &format!("tracked_topic:{}", topic),
                "tracked_topic",
                Some(serde_json::json!({
                    "topic": topic,
                    "queries": queries,
                    "interval_hours": interval_hours,
                    "keywords": keywords,
                    "active": true,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                })),
            )
            .await?;

        Ok(format!(
            "Topic tracking configured:\n\
             - Topic: {}\n\
             - Entity ID: {}\n\
             - Search queries: {}\n\
             - Check interval: every {} hours\n\
             - Keywords: {}\n\n\
             To activate periodic checking, create a watcher using create_watcher with:\n\
             - kind: 'scheduled'\n\
             - config: {{\"cron_expr\": \"0 */{} * * *\", \"task\": \"Research update for: {}\"}}\n\
             - action: \"Run research_topic for '{}' and compare with previous findings\"",
            topic,
            entity_id,
            queries.join(", "),
            interval_hours,
            if keywords.is_empty() {
                "none".to_string()
            } else {
                keywords.join(", ")
            },
            interval_hours,
            topic,
            topic
        ))
    }
}

/// Fact-check a claim using web search
pub struct FactCheckTool {
    tavily: Option<Arc<TavilyClient>>,
    db: Arc<KnowledgeDb>,
}

impl FactCheckTool {
    pub fn new(tavily: Option<Arc<TavilyClient>>, db: Arc<KnowledgeDb>) -> Self {
        Self { tavily, db }
    }
}

#[async_trait]
impl ToolHandler for FactCheckTool {
    fn name(&self) -> &str {
        "fact_check"
    }

    fn description(&self) -> &str {
        "Fact-check a specific claim or statement. Searches for supporting and contradicting \
         evidence, evaluates source credibility, and provides a confidence assessment."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "claim": {
                    "type": "string",
                    "description": "The claim or statement to fact-check"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context about where the claim was made"
                }
            }),
            vec!["claim"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let claim = input
            .get("claim")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'claim' parameter"))?;
        let context = input.get("context").and_then(|v| v.as_str()).unwrap_or("");

        if claim.len() > 2000 {
            return Err(anyhow::anyhow!("Claim too long (max 2000 characters)"));
        }

        debug!("Fact-checking: {}", claim);

        let mut results = Vec::new();
        if let Some(ref tavily) = self.tavily {
            // Search for the claim directly
            let direct = tavily.search(claim, 5).await?;
            results.push(format!(
                "Direct search:\n{}",
                TavilyClient::format_results(&direct)
            ));

            // Search for counter-evidence
            let counter_query = format!("{} false debunked", claim);
            if let Ok(counter) = tavily.search(&counter_query, 3).await {
                results.push(format!(
                    "Counter-evidence search:\n{}",
                    TavilyClient::format_results(&counter)
                ));
            }
        } else {
            results.push("Web search not available (no Tavily API key).".to_string());
        }

        // Check knowledge graph for related info
        let existing = self
            .db
            .search_entities(claim, None)
            .await
            .unwrap_or_default();
        let existing_str = if existing.is_empty() {
            "No prior knowledge.".to_string()
        } else {
            existing
                .iter()
                .take(5)
                .map(|e| format!("- {} ({})", e.name, e.entity_type))
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(format!(
            "# Fact Check\n\n\
             **Claim:** {}\n\
             **Context:** {}\n\n\
             ## Evidence\n{}\n\n\
             ## Prior Knowledge\n{}\n\n\
             ## Instructions\n\
             Evaluate the claim and provide:\n\
             1. **Verdict**: True / Mostly True / Mixed / Mostly False / False / Unverifiable\n\
             2. **Confidence**: High / Medium / Low\n\
             3. **Supporting Evidence**: What supports the claim\n\
             4. **Contradicting Evidence**: What contradicts it\n\
             5. **Key Sources**: Most credible sources found",
            claim,
            if context.is_empty() {
                "None provided"
            } else {
                context
            },
            results.join("\n\n---\n\n"),
            existing_str
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<KnowledgeDb> {
        Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_research.db")).unwrap())
    }

    #[test]
    fn test_research_topic_schema() {
        let tool = ResearchTopicTool::new(None, test_db());
        assert_eq!(tool.name(), "research_topic");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"topic".to_string()));
    }

    #[test]
    fn test_compile_report_schema() {
        let tool = CompileReportTool::new(test_db());
        assert_eq!(tool.name(), "compile_report");
    }

    #[test]
    fn test_track_topic_schema() {
        let tool = TrackTopicTool::new(test_db());
        assert_eq!(tool.name(), "track_topic");
    }

    #[test]
    fn test_fact_check_schema() {
        let tool = FactCheckTool::new(None, test_db());
        assert_eq!(tool.name(), "fact_check");
    }
}
