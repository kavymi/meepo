//! macOS Spotlight search tools — file search and metadata

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::SpotlightProvider;

pub struct SpotlightSearchTool {
    provider: Box<dyn SpotlightProvider>,
}

impl SpotlightSearchTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_spotlight_provider()
                .expect("Spotlight provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SpotlightSearchTool {
    fn name(&self) -> &str {
        "spotlight_search"
    }

    fn description(&self) -> &str {
        "Search files using macOS Spotlight (mdfind). Supports content search, file names, and metadata queries."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "query": {
                    "type": "string",
                    "description": "Spotlight query (e.g., 'kMDItemKind == PDF', 'budget report', 'kind:image date:today')"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of results (default: 20, max: 100)"
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
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);
        debug!("Spotlight search: {}", query);
        self.provider.search(query, limit).await
    }
}

pub struct SpotlightMetadataTool {
    provider: Box<dyn SpotlightProvider>,
}

impl SpotlightMetadataTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_spotlight_provider()
                .expect("Spotlight provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SpotlightMetadataTool {
    fn name(&self) -> &str {
        "spotlight_metadata"
    }

    fn description(&self) -> &str {
        "Get detailed Spotlight metadata for a file (EXIF, creation date, content type, etc.)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file"
                }
            }),
            vec!["path"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        debug!("Getting metadata for: {}", path);
        self.provider.get_metadata(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_spotlight_search_schema() {
        let tool = SpotlightSearchTool::new();
        assert_eq!(tool.name(), "spotlight_search");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema.get("required").cloned().unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"query".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_spotlight_search_missing_query() {
        let tool = SpotlightSearchTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_spotlight_metadata_schema() {
        let tool = SpotlightMetadataTool::new();
        assert_eq!(tool.name(), "spotlight_metadata");
    }
}
