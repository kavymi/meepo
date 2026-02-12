//! Web search tool using Tavily Search API

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::tavily::TavilyClient;

/// Search the web using Tavily
pub struct WebSearchTool {
    client: Arc<TavilyClient>,
}

impl WebSearchTool {
    pub fn new(client: Arc<TavilyClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ToolHandler for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for current information. Returns ranked results with content excerpts."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Number of results to return (default 5, max 10)"
                }
            }),
            vec!["query"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;

        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        debug!("Web search: '{}' (max_results: {})", query, max_results);

        let response = self.client.search(query, max_results).await?;
        Ok(TavilyClient::format_results(&response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_tool_schema() {
        let client = Arc::new(TavilyClient::new("test-key".to_string()));
        let tool = WebSearchTool::new(client);
        assert_eq!(tool.name(), "web_search");
        assert!(!tool.description().is_empty());
        let schema = tool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("query").is_some());
        assert!(props.get("max_results").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&Value::String("query".to_string())));
    }

    #[tokio::test]
    async fn test_web_search_tool_missing_query() {
        let client = Arc::new(TavilyClient::new("test-key".to_string()));
        let tool = WebSearchTool::new(client);
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("query"));
    }
}
