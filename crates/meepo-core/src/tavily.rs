//! Tavily API client for web search and content extraction

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Client for the Tavily search and extract APIs
#[derive(Clone)]
pub struct TavilyClient {
    client: Client,
    api_key: String,
}

impl std::fmt::Debug for TavilyClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TavilyClient")
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// A single search result from the Tavily API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub score: f64,
}

/// Response from the Tavily search API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub results: Vec<SearchResult>,
    #[serde(default)]
    pub query: String,
}

/// Request body for the Tavily search API
#[derive(Serialize)]
struct SearchRequest {
    api_key: String,
    query: String,
    max_results: usize,
    include_answer: bool,
}

impl std::fmt::Debug for SearchRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchRequest")
            .field("api_key", &"[REDACTED]")
            .field("query", &self.query)
            .field("max_results", &self.max_results)
            .field("include_answer", &self.include_answer)
            .finish()
    }
}

/// Request body for the Tavily extract API
#[derive(Serialize)]
struct ExtractRequest {
    api_key: String,
    urls: Vec<String>,
}

impl std::fmt::Debug for ExtractRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractRequest")
            .field("api_key", &"[REDACTED]")
            .field("urls", &self.urls)
            .finish()
    }
}

/// A single result from the Tavily extract API
#[derive(Debug, Deserialize)]
struct ExtractResult {
    #[serde(default)]
    raw_content: Option<String>,
}

/// Response from the Tavily extract API
#[derive(Debug, Deserialize)]
struct ExtractResponse {
    #[serde(default)]
    results: Vec<ExtractResult>,
}

impl TavilyClient {
    /// Create a new TavilyClient with the given API key
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self { client, api_key }
    }

    /// Search the web using the Tavily search API
    pub async fn search(&self, query: &str, max_results: usize) -> Result<SearchResponse> {
        let max_results = max_results.min(10);

        let request = SearchRequest {
            api_key: self.api_key.clone(),
            query: query.to_string(),
            max_results,
            include_answer: true,
        };

        debug!(
            query = query,
            max_results = max_results,
            "Tavily search request"
        );

        let response = self
            .client
            .post("https://api.tavily.com/search")
            .json(&request)
            .send()
            .await
            .context("Failed to send Tavily search request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Tavily search failed with status {status}: {body}");
        }

        let search_response: SearchResponse = response
            .json()
            .await
            .context("Failed to parse Tavily search response")?;

        debug!(
            results = search_response.results.len(),
            has_answer = search_response.answer.is_some(),
            "Tavily search response"
        );

        Ok(search_response)
    }

    /// Extract content from a URL using the Tavily extract API
    pub async fn extract(&self, url: &str) -> Result<String> {
        let request = ExtractRequest {
            api_key: self.api_key.clone(),
            urls: vec![url.to_string()],
        };

        debug!(url = url, "Tavily extract request");

        let response = self
            .client
            .post("https://api.tavily.com/extract")
            .json(&request)
            .send()
            .await
            .context("Failed to send Tavily extract request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Tavily extract failed with status {status}: {body}");
        }

        let extract_response: ExtractResponse = response
            .json()
            .await
            .context("Failed to parse Tavily extract response")?;

        let content = extract_response
            .results
            .into_iter()
            .next()
            .and_then(|r| r.raw_content)
            .unwrap_or_default();

        Ok(content)
    }

    /// Format a SearchResponse as markdown
    pub fn format_results(response: &SearchResponse) -> String {
        if response.answer.is_none() && response.results.is_empty() {
            return "No results found.".to_string();
        }

        let mut output = String::new();

        if let Some(answer) = &response.answer {
            output.push_str("## Answer\n\n");
            output.push_str(answer);
            output.push_str("\n\n");
        }

        if !response.results.is_empty() {
            output.push_str("## Results\n\n");
            for (i, result) in response.results.iter().enumerate() {
                output.push_str(&format!(
                    "{}. **{}**\n   {}\n",
                    i + 1,
                    result.title,
                    result.url
                ));
                if let Some(content) = &result.content {
                    output.push_str(&format!("   {}\n", content));
                }
                output.push('\n');
            }
        }

        output.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_request_serialization() {
        let request = SearchRequest {
            api_key: "test-key".to_string(),
            query: "rust programming".to_string(),
            max_results: 5,
            include_answer: true,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["api_key"], "test-key");
        assert_eq!(json["query"], "rust programming");
        assert_eq!(json["max_results"], 5);
        assert_eq!(json["include_answer"], true);
    }

    #[test]
    fn test_search_response_deserialization() {
        let json = serde_json::json!({
            "answer": "Rust is a systems programming language.",
            "query": "what is rust",
            "results": [
                {
                    "title": "Rust Language",
                    "url": "https://rust-lang.org",
                    "content": "Rust is a multi-paradigm language.",
                    "score": 0.95
                },
                {
                    "title": "Rust Book",
                    "url": "https://doc.rust-lang.org/book/",
                    "content": "The Rust Programming Language book.",
                    "score": 0.90
                }
            ]
        });

        let response: SearchResponse = serde_json::from_value(json).unwrap();
        assert_eq!(
            response.answer.as_deref(),
            Some("Rust is a systems programming language.")
        );
        assert_eq!(response.query, "what is rust");
        assert_eq!(response.results.len(), 2);
        assert_eq!(response.results[0].title, "Rust Language");
        assert_eq!(response.results[0].url, "https://rust-lang.org");
        assert_eq!(
            response.results[0].content.as_deref(),
            Some("Rust is a multi-paradigm language.")
        );
        assert!((response.results[0].score - 0.95).abs() < f64::EPSILON);
        assert_eq!(response.results[1].title, "Rust Book");
    }

    #[test]
    fn test_search_response_missing_optional_fields() {
        let json = serde_json::json!({
            "results": [
                {
                    "title": "Example",
                    "url": "https://example.com"
                }
            ]
        });

        let response: SearchResponse = serde_json::from_value(json).unwrap();
        assert!(response.answer.is_none());
        assert_eq!(response.query, "");
        assert_eq!(response.results.len(), 1);
        assert!(response.results[0].content.is_none());
        assert!((response.results[0].score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_response_deserialization() {
        let json = serde_json::json!({
            "results": [
                {
                    "raw_content": "This is the extracted content from the page."
                }
            ]
        });

        let response: ExtractResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.results.len(), 1);
        assert_eq!(
            response.results[0].raw_content.as_deref(),
            Some("This is the extracted content from the page.")
        );
    }

    #[test]
    fn test_extract_response_empty() {
        let json = serde_json::json!({
            "results": []
        });

        let response: ExtractResponse = serde_json::from_value(json).unwrap();
        assert!(response.results.is_empty());
    }

    #[test]
    fn test_format_results_with_answer_and_results() {
        let response = SearchResponse {
            answer: Some("Rust is great.".to_string()),
            query: "rust".to_string(),
            results: vec![
                SearchResult {
                    title: "Rust Lang".to_string(),
                    url: "https://rust-lang.org".to_string(),
                    content: Some("Official site.".to_string()),
                    score: 0.9,
                },
                SearchResult {
                    title: "Rust Book".to_string(),
                    url: "https://doc.rust-lang.org/book/".to_string(),
                    content: Some("Learn Rust.".to_string()),
                    score: 0.8,
                },
            ],
        };

        let output = TavilyClient::format_results(&response);
        assert!(output.contains("## Answer"));
        assert!(output.contains("Rust is great."));
        assert!(output.contains("## Results"));
        assert!(output.contains("1. **Rust Lang**"));
        assert!(output.contains("https://rust-lang.org"));
        assert!(output.contains("2. **Rust Book**"));
        assert!(output.contains("https://doc.rust-lang.org/book/"));
        assert!(output.contains("Official site."));
        assert!(output.contains("Learn Rust."));
    }

    #[test]
    fn test_format_results_no_answer() {
        let response = SearchResponse {
            answer: None,
            query: "test".to_string(),
            results: vec![SearchResult {
                title: "Test".to_string(),
                url: "https://test.com".to_string(),
                content: Some("Test content.".to_string()),
                score: 0.5,
            }],
        };

        let output = TavilyClient::format_results(&response);
        assert!(!output.contains("## Answer"));
        assert!(output.contains("## Results"));
        assert!(output.contains("1. **Test**"));
    }

    #[test]
    fn test_format_results_empty() {
        let response = SearchResponse {
            answer: None,
            query: "nothing".to_string(),
            results: vec![],
        };

        let output = TavilyClient::format_results(&response);
        assert_eq!(output, "No results found.");
    }

    #[test]
    fn test_max_results_clamped() {
        // The clamping logic: max_results.min(10)
        assert_eq!(20_usize.min(10), 10);
        assert_eq!(5_usize.min(10), 5);
        assert_eq!(10_usize.min(10), 10);
    }
}
