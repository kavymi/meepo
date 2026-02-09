# Web Search Tool Design

**Date**: 2026-02-08
**Status**: Approved

## Problem

Meepo has no way to search the web. This blocks entire categories of use cases: research, event finding, travel planning, current information queries. The existing `browse_url` tool can fetch a specific URL but returns raw HTML, which is noisy for the agent.

## Solution

1. Add a `web_search` tool using the Tavily Search API
2. Upgrade `browse_url` to use Tavily's Extract API for clean content (with raw fetch fallback)
3. Share a `TavilyClient` between both tools

## Design

### TavilyClient (`crates/meepo-core/src/tavily.rs`)

Shared HTTP client for Tavily's API:

```rust
pub struct TavilyClient {
    client: reqwest::Client,
    api_key: String,
}

impl TavilyClient {
    pub async fn search(&self, query: &str, max_results: usize) -> Result<TavilySearchResponse>;
    pub async fn extract(&self, url: &str) -> Result<String>;
}
```

### WebSearchTool (`crates/meepo-core/src/tools/search.rs`)

```json
{
  "name": "web_search",
  "input_schema": {
    "query": { "type": "string", "description": "The search query" },
    "max_results": { "type": "integer", "description": "Number of results (default 5, max 10)" }
  }
}
```

Returns formatted markdown with Tavily's answer + ranked results with content excerpts.

### BrowseUrlTool upgrade (`crates/meepo-core/src/tools/system.rs`)

- Takes an optional `Arc<TavilyClient>`
- On execute: try Tavily extract first, fall back to raw fetch
- All existing SSRF/URL validation stays in place

### Config

```toml
[providers.tavily]
api_key = "${TAVILY_API_KEY}"
```

## Files

| File | Action |
|------|--------|
| `crates/meepo-core/src/tavily.rs` | Create |
| `crates/meepo-core/src/tools/search.rs` | Create |
| `crates/meepo-core/src/tools/system.rs` | Modify |
| `crates/meepo-core/src/tools/mod.rs` | Modify |
| `crates/meepo-core/src/lib.rs` | Modify |
| `crates/meepo-cli/src/config.rs` | Modify |
| `crates/meepo-cli/src/main.rs` | Modify |
| `config/default.toml` | Modify |
