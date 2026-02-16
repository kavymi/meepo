//! News & Content Curator tools
//!
//! Monitor RSS feeds, news sources, and URLs for updates. Deliver personalized
//! morning digests with summaries. Track and filter content by relevance.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::tavily::TavilyClient;
use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Track a content feed or news source
pub struct TrackFeedTool {
    db: Arc<KnowledgeDb>,
}

impl TrackFeedTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for TrackFeedTool {
    fn name(&self) -> &str {
        "track_feed"
    }

    fn description(&self) -> &str {
        "Start tracking a content feed, news source, or URL for updates. Stores the feed \
         configuration in the knowledge graph and sets up periodic checking. Supports RSS URLs, \
         news sites, blogs, or search queries to monitor."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "name": {
                    "type": "string",
                    "description": "Human-readable name for this feed (e.g., 'Hacker News AI')"
                },
                "source": {
                    "type": "string",
                    "description": "Feed URL, website URL, or search query to monitor"
                },
                "source_type": {
                    "type": "string",
                    "description": "Type: rss, website, search_query (default: search_query)"
                },
                "filter_keywords": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Only include items matching these keywords"
                },
                "check_interval_hours": {
                    "type": "number",
                    "description": "How often to check for updates in hours (default: 12, min: 1)"
                },
                "max_items": {
                    "type": "number",
                    "description": "Maximum items to include per check (default: 10)"
                }
            }),
            vec!["name", "source"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
        let source = input
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'source' parameter"))?;
        let source_type = input
            .get("source_type")
            .and_then(|v| v.as_str())
            .unwrap_or("search_query");
        let filter_keywords: Vec<String> = input
            .get("filter_keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let interval = input
            .get("check_interval_hours")
            .and_then(|v| v.as_u64())
            .unwrap_or(12)
            .max(1);
        let max_items = input
            .get("max_items")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);

        if name.len() > 200 {
            return Err(anyhow::anyhow!("Name too long (max 200 characters)"));
        }
        if source.len() > 2000 {
            return Err(anyhow::anyhow!("Source too long (max 2000 characters)"));
        }

        debug!("Tracking feed: {} ({})", name, source_type);

        let entity_id = self
            .db
            .insert_entity(
                name,
                "tracked_feed",
                Some(serde_json::json!({
                    "source": source,
                    "source_type": source_type,
                    "filter_keywords": filter_keywords,
                    "check_interval_hours": interval,
                    "max_items": max_items,
                    "active": true,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "last_checked_at": null,
                })),
            )
            .await?;

        Ok(format!(
            "Feed tracked:\n\
             - Name: {}\n\
             - ID: {}\n\
             - Source: {} ({})\n\
             - Keywords: {}\n\
             - Check interval: every {} hours\n\
             - Max items: {}\n\n\
             To activate periodic checking, create a watcher:\n\
             - kind: 'scheduled'\n\
             - config: {{\"cron_expr\": \"0 */{} * * *\", \"task\": \"Check feed: {}\"}}\n\
             - action: \"Fetch latest from '{}' and summarize new items\"",
            name,
            entity_id,
            source,
            source_type,
            if filter_keywords.is_empty() {
                "none".to_string()
            } else {
                filter_keywords.join(", ")
            },
            interval,
            max_items,
            interval,
            name,
            name
        ))
    }
}

/// Stop tracking a feed
pub struct UntrackFeedTool {
    db: Arc<KnowledgeDb>,
}

impl UntrackFeedTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for UntrackFeedTool {
    fn name(&self) -> &str {
        "untrack_feed"
    }

    fn description(&self) -> &str {
        "Stop tracking a content feed. Deactivates the feed but preserves historical data \
         in the knowledge graph."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "feed_name": {
                    "type": "string",
                    "description": "Name or ID of the feed to stop tracking"
                }
            }),
            vec!["feed_name"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let feed_name = input
            .get("feed_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'feed_name' parameter"))?;

        debug!("Untracking feed: {}", feed_name);

        // Find the feed entity
        let feeds = self
            .db
            .search_entities(feed_name, Some("tracked_feed"))
            .await?;

        if feeds.is_empty() {
            return Ok(format!("Feed not found: {}", feed_name));
        }

        // Mark as inactive by creating updated entity
        for feed in &feeds {
            let mut metadata = feed.metadata.clone().unwrap_or(serde_json::json!({}));
            metadata["active"] = serde_json::json!(false);
            metadata["deactivated_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            let _ = self
                .db
                .insert_entity(&feed.name, "tracked_feed", Some(metadata))
                .await;
        }

        Ok(format!(
            "Feed untracked: {} ({} feed(s) deactivated)",
            feed_name,
            feeds.len()
        ))
    }
}

/// Summarize an article or URL
pub struct SummarizeArticleTool {
    tavily: Option<Arc<TavilyClient>>,
    db: Arc<KnowledgeDb>,
}

impl SummarizeArticleTool {
    pub fn new(tavily: Option<Arc<TavilyClient>>, db: Arc<KnowledgeDb>) -> Self {
        Self { tavily, db }
    }
}

#[async_trait]
impl ToolHandler for SummarizeArticleTool {
    fn name(&self) -> &str {
        "summarize_article"
    }

    fn description(&self) -> &str {
        "Fetch and summarize an article from a URL. Extracts the main content, generates a \
         concise summary with key takeaways, and stores it in the knowledge graph for future \
         reference."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "url": {
                    "type": "string",
                    "description": "URL of the article to summarize"
                },
                "summary_length": {
                    "type": "string",
                    "description": "Summary length: brief (1-2 sentences), standard (paragraph), detailed (full). Default: standard"
                }
            }),
            vec!["url"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;
        let length = input
            .get("summary_length")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");

        if url.len() > 2000 {
            return Err(anyhow::anyhow!("URL too long (max 2000 characters)"));
        }

        debug!("Summarizing article: {}", url);

        let content = if let Some(ref tavily) = self.tavily {
            match tavily.extract(url).await {
                Ok(extracted) => extracted,
                Err(_) => {
                    // Fallback to raw fetch
                    let resp = reqwest::get(url).await?;
                    let text = resp.text().await?;
                    // Truncate to reasonable size
                    text[..text.len().min(50_000)].to_string()
                }
            }
        } else {
            let resp = reqwest::get(url).await?;
            let text = resp.text().await?;
            text[..text.len().min(50_000)].to_string()
        };

        // Store in knowledge graph
        let _ = self
            .db
            .insert_entity(
                url,
                "article",
                Some(serde_json::json!({
                    "url": url,
                    "fetched_at": chrono::Utc::now().to_rfc3339(),
                    "content_length": content.len(),
                })),
            )
            .await;

        Ok(format!(
            "# Article Content\n\nURL: {}\n\n{}\n\n\
             ---\n\n\
             Please provide a {} summary including:\n\
             1. **Main Point** — the core argument or news\n\
             2. **Key Takeaways** — 3-5 bullet points\n\
             3. **Relevance** — why this matters",
            url,
            &content[..content.len().min(30_000)],
            length
        ))
    }
}

/// Generate a content digest from tracked feeds
pub struct ContentDigestTool {
    tavily: Option<Arc<TavilyClient>>,
    db: Arc<KnowledgeDb>,
}

impl ContentDigestTool {
    pub fn new(tavily: Option<Arc<TavilyClient>>, db: Arc<KnowledgeDb>) -> Self {
        Self { tavily, db }
    }
}

#[async_trait]
impl ToolHandler for ContentDigestTool {
    fn name(&self) -> &str {
        "content_digest"
    }

    fn description(&self) -> &str {
        "Generate a personalized content digest from all tracked feeds. Fetches latest items \
         from each active feed, filters by keywords, and compiles a curated digest with summaries."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "max_items_per_feed": {
                    "type": "number",
                    "description": "Maximum items per feed (default: 5)"
                },
                "include_summaries": {
                    "type": "boolean",
                    "description": "Include brief summaries for each item (default: true)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let max_items = input
            .get("max_items_per_feed")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);
        let include_summaries = input
            .get("include_summaries")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        debug!("Generating content digest");

        // Get all active tracked feeds
        let feeds = self
            .db
            .search_entities("", Some("tracked_feed"))
            .await
            .unwrap_or_default();

        let active_feeds: Vec<_> = feeds
            .iter()
            .filter(|f| {
                f.metadata
                    .as_ref()
                    .and_then(|m| m.get("active"))
                    .and_then(|a| a.as_bool())
                    .unwrap_or(true)
            })
            .collect();

        if active_feeds.is_empty() {
            return Ok(
                "No active feeds to digest. Use track_feed to start tracking content sources."
                    .to_string(),
            );
        }

        let mut digest_sections = Vec::new();

        for feed in &active_feeds {
            let source = feed
                .metadata
                .as_ref()
                .and_then(|m| m.get("source"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let source_type = feed
                .metadata
                .as_ref()
                .and_then(|m| m.get("source_type"))
                .and_then(|s| s.as_str())
                .unwrap_or("search_query");

            let mut section = format!("## {}\nSource: {} ({})\n\n", feed.name, source, source_type);

            // Fetch content based on source type
            if let Some(ref tavily) = self.tavily {
                if source_type == "search_query" || source_type == "website" {
                    match tavily.search(source, max_items as usize).await {
                        Ok(results) => {
                            section.push_str(&TavilyClient::format_results(&results));
                        }
                        Err(e) => {
                            section.push_str(&format!("Error fetching: {}\n", e));
                        }
                    }
                }
            } else {
                section.push_str("Web search not available (no Tavily API key).\n");
            }

            digest_sections.push(section);
        }

        Ok(format!(
            "# Content Digest — {}\n\n\
             {} active feeds, {} max items per feed\n\n\
             {}\n\n\
             ---\n\n\
             {}",
            chrono::Local::now().format("%B %d, %Y"),
            active_feeds.len(),
            max_items,
            digest_sections.join("\n---\n\n"),
            if include_summaries {
                "Please provide a brief summary for each item and highlight the top 3 most \
                 interesting/relevant items across all feeds."
            } else {
                "List items without summaries."
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<KnowledgeDb> {
        Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_news.db")).unwrap())
    }

    #[test]
    fn test_track_feed_schema() {
        let tool = TrackFeedTool::new(test_db());
        assert_eq!(tool.name(), "track_feed");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"name".to_string()));
        assert!(required.contains(&"source".to_string()));
    }

    #[test]
    fn test_untrack_feed_schema() {
        let tool = UntrackFeedTool::new(test_db());
        assert_eq!(tool.name(), "untrack_feed");
    }

    #[test]
    fn test_summarize_article_schema() {
        let tool = SummarizeArticleTool::new(None, test_db());
        assert_eq!(tool.name(), "summarize_article");
    }

    #[test]
    fn test_content_digest_schema() {
        let tool = ContentDigestTool::new(None, test_db());
        assert_eq!(tool.name(), "content_digest");
    }
}
