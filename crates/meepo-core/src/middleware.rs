//! Context Engineering Middleware Architecture
//!
//! A composable hook system for the agent loop, enabling pluggable
//! pre/post processing of model calls and tool calls. Inspired by
//! LangChain v1's middleware system (AgentMiddleware).
//!
//! Middleware can:
//! - Modify messages before they reach the model (summarization, PII redaction)
//! - Filter/select tools per query (tool selection)
//! - Intercept tool results (validation, caching)
//! - Transform the final response (formatting, guardrails)

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::api::{ApiMessage, ToolDefinition};

/// Context passed through the middleware chain
#[derive(Debug, Clone)]
pub struct MiddlewareContext {
    /// The original user query
    pub query: String,
    /// The channel the message came from
    pub channel: String,
    /// The sender of the message
    pub sender: String,
    /// Arbitrary metadata that middleware can read/write
    pub metadata: Value,
}

/// Trait for agent middleware hooks.
///
/// All methods have default no-op implementations, so middleware only
/// needs to implement the hooks it cares about.
#[async_trait]
pub trait AgentMiddleware: Send + Sync {
    /// Human-readable name for logging
    fn name(&self) -> &str;

    /// Called before the model is invoked. Can modify messages and tools.
    ///
    /// Return the (possibly modified) messages and tools.
    async fn before_model(
        &self,
        messages: Vec<ApiMessage>,
        tools: Vec<ToolDefinition>,
        _ctx: &MiddlewareContext,
    ) -> Result<(Vec<ApiMessage>, Vec<ToolDefinition>)> {
        Ok((messages, tools))
    }

    /// Called after the model responds, before tool execution.
    ///
    /// Can inspect or modify the model's response content blocks.
    async fn after_model(
        &self,
        response_content: Vec<crate::api::ContentBlock>,
        _ctx: &MiddlewareContext,
    ) -> Result<Vec<crate::api::ContentBlock>> {
        Ok(response_content)
    }

    /// Called before a tool is executed. Can modify the input or skip execution.
    ///
    /// Return `None` to skip the tool call, or `Some(modified_input)` to proceed.
    async fn before_tool(
        &self,
        _tool_name: &str,
        input: Value,
        _ctx: &MiddlewareContext,
    ) -> Result<Option<Value>> {
        Ok(Some(input))
    }

    /// Called after a tool executes. Can modify the result.
    async fn after_tool(
        &self,
        _tool_name: &str,
        result: String,
        _ctx: &MiddlewareContext,
    ) -> Result<String> {
        Ok(result)
    }

    /// Called after the agent produces its final response.
    ///
    /// Can modify the final text before it's sent to the user.
    async fn after_agent(&self, response: String, _ctx: &MiddlewareContext) -> Result<String> {
        Ok(response)
    }
}

/// A chain of middleware that executes in order.
pub struct MiddlewareChain {
    middlewares: Vec<Arc<dyn AgentMiddleware>>,
}

impl MiddlewareChain {
    /// Create a new empty middleware chain
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// Add a middleware to the chain
    pub fn add(&mut self, middleware: Arc<dyn AgentMiddleware>) {
        debug!("Adding middleware: {}", middleware.name());
        self.middlewares.push(middleware);
    }

    /// Number of middleware in the chain
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }

    /// Check if chain is empty
    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    /// Run all before_model hooks in order
    pub async fn run_before_model(
        &self,
        mut messages: Vec<ApiMessage>,
        mut tools: Vec<ToolDefinition>,
        ctx: &MiddlewareContext,
    ) -> Result<(Vec<ApiMessage>, Vec<ToolDefinition>)> {
        for mw in &self.middlewares {
            let result = mw.before_model(messages, tools, ctx).await?;
            messages = result.0;
            tools = result.1;
        }
        Ok((messages, tools))
    }

    /// Run all after_model hooks in order
    pub async fn run_after_model(
        &self,
        mut content: Vec<crate::api::ContentBlock>,
        ctx: &MiddlewareContext,
    ) -> Result<Vec<crate::api::ContentBlock>> {
        for mw in &self.middlewares {
            content = mw.after_model(content, ctx).await?;
        }
        Ok(content)
    }

    /// Run all before_tool hooks in order.
    /// Returns None if any middleware wants to skip the tool call.
    pub async fn run_before_tool(
        &self,
        tool_name: &str,
        mut input: Value,
        ctx: &MiddlewareContext,
    ) -> Result<Option<Value>> {
        for mw in &self.middlewares {
            match mw.before_tool(tool_name, input, ctx).await? {
                Some(modified) => input = modified,
                None => return Ok(None), // skip tool
            }
        }
        Ok(Some(input))
    }

    /// Run all after_tool hooks in order
    pub async fn run_after_tool(
        &self,
        tool_name: &str,
        mut result: String,
        ctx: &MiddlewareContext,
    ) -> Result<String> {
        for mw in &self.middlewares {
            result = mw.after_tool(tool_name, result, ctx).await?;
        }
        Ok(result)
    }

    /// Run all after_agent hooks in order
    pub async fn run_after_agent(
        &self,
        mut response: String,
        ctx: &MiddlewareContext,
    ) -> Result<String> {
        for mw in &self.middlewares {
            response = mw.after_agent(response, ctx).await?;
        }
        Ok(response)
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}

// ── Built-in Middleware Implementations ──────────────────────────────

/// Middleware that logs all model and tool calls for debugging
pub struct LoggingMiddleware;

#[async_trait]
impl AgentMiddleware for LoggingMiddleware {
    fn name(&self) -> &str {
        "logging"
    }

    async fn before_model(
        &self,
        messages: Vec<ApiMessage>,
        tools: Vec<ToolDefinition>,
        ctx: &MiddlewareContext,
    ) -> Result<(Vec<ApiMessage>, Vec<ToolDefinition>)> {
        debug!(
            "[logging] before_model: {} messages, {} tools, query='{}'",
            messages.len(),
            tools.len(),
            ctx.query.chars().take(50).collect::<String>()
        );
        Ok((messages, tools))
    }

    async fn before_tool(
        &self,
        tool_name: &str,
        input: Value,
        _ctx: &MiddlewareContext,
    ) -> Result<Option<Value>> {
        debug!("[logging] before_tool: {}", tool_name);
        Ok(Some(input))
    }

    async fn after_tool(
        &self,
        tool_name: &str,
        result: String,
        _ctx: &MiddlewareContext,
    ) -> Result<String> {
        debug!(
            "[logging] after_tool: {} ({} chars)",
            tool_name,
            result.len()
        );
        Ok(result)
    }

    async fn after_agent(&self, response: String, _ctx: &MiddlewareContext) -> Result<String> {
        debug!("[logging] after_agent: {} chars", response.len());
        Ok(response)
    }
}

/// Middleware that enforces a maximum number of tool calls per interaction
pub struct ToolCallLimitMiddleware {
    max_calls: usize,
    call_count: std::sync::atomic::AtomicUsize,
}

impl ToolCallLimitMiddleware {
    pub fn new(max_calls: usize) -> Self {
        Self {
            max_calls,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl AgentMiddleware for ToolCallLimitMiddleware {
    fn name(&self) -> &str {
        "tool_call_limit"
    }

    async fn before_tool(
        &self,
        tool_name: &str,
        input: Value,
        _ctx: &MiddlewareContext,
    ) -> Result<Option<Value>> {
        let count = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count >= self.max_calls {
            debug!(
                "[tool_call_limit] Blocking tool {} (limit {} reached)",
                tool_name, self.max_calls
            );
            return Ok(None);
        }
        Ok(Some(input))
    }
}

/// Middleware that truncates oversized tool outputs
pub struct ToolOutputTruncationMiddleware {
    max_chars: usize,
}

impl ToolOutputTruncationMiddleware {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }
}

#[async_trait]
impl AgentMiddleware for ToolOutputTruncationMiddleware {
    fn name(&self) -> &str {
        "tool_output_truncation"
    }

    async fn after_tool(
        &self,
        tool_name: &str,
        mut result: String,
        _ctx: &MiddlewareContext,
    ) -> Result<String> {
        if result.len() > self.max_chars {
            debug!(
                "[truncation] Truncating {} output from {} to {} chars",
                tool_name,
                result.len(),
                self.max_chars
            );
            result.truncate(self.max_chars);
            result.push_str("\n[Output truncated]");
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_chain() {
        let chain = MiddlewareChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);

        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "internal".to_string(),
            sender: "user".to_string(),
            metadata: Value::Null,
        };

        // All hooks should pass through unchanged
        let (msgs, tools) = chain.run_before_model(vec![], vec![], &ctx).await.unwrap();
        assert!(msgs.is_empty());
        assert!(tools.is_empty());

        let result = chain
            .run_after_agent("hello".to_string(), &ctx)
            .await
            .unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_logging_middleware() {
        let mut chain = MiddlewareChain::new();
        chain.add(Arc::new(LoggingMiddleware));

        let ctx = MiddlewareContext {
            query: "test query".to_string(),
            channel: "internal".to_string(),
            sender: "user".to_string(),
            metadata: Value::Null,
        };

        // Should pass through without modification
        let result = chain
            .run_after_agent("response".to_string(), &ctx)
            .await
            .unwrap();
        assert_eq!(result, "response");
    }

    #[tokio::test]
    async fn test_tool_call_limit() {
        let mw = ToolCallLimitMiddleware::new(2);
        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "internal".to_string(),
            sender: "user".to_string(),
            metadata: Value::Null,
        };

        // First two calls should succeed
        let r1 = mw.before_tool("tool1", Value::Null, &ctx).await.unwrap();
        assert!(r1.is_some());

        let r2 = mw.before_tool("tool2", Value::Null, &ctx).await.unwrap();
        assert!(r2.is_some());

        // Third call should be blocked
        let r3 = mw.before_tool("tool3", Value::Null, &ctx).await.unwrap();
        assert!(r3.is_none());
    }

    #[tokio::test]
    async fn test_truncation_middleware() {
        let mw = ToolOutputTruncationMiddleware::new(10);
        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "internal".to_string(),
            sender: "user".to_string(),
            metadata: Value::Null,
        };

        // Short output should pass through
        let result = mw
            .after_tool("tool1", "short".to_string(), &ctx)
            .await
            .unwrap();
        assert_eq!(result, "short");

        // Long output should be truncated
        let result = mw
            .after_tool("tool1", "this is a very long output".to_string(), &ctx)
            .await
            .unwrap();
        assert!(result.contains("[Output truncated]"));
        assert!(result.len() < 40);
    }

    #[tokio::test]
    async fn test_chain_ordering() {
        // Verify middleware runs in order
        struct AppendMiddleware {
            suffix: String,
        }

        #[async_trait]
        impl AgentMiddleware for AppendMiddleware {
            fn name(&self) -> &str {
                "append"
            }

            async fn after_agent(
                &self,
                response: String,
                _ctx: &MiddlewareContext,
            ) -> Result<String> {
                Ok(format!("{}{}", response, self.suffix))
            }
        }

        let mut chain = MiddlewareChain::new();
        chain.add(Arc::new(AppendMiddleware {
            suffix: "_A".to_string(),
        }));
        chain.add(Arc::new(AppendMiddleware {
            suffix: "_B".to_string(),
        }));

        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "internal".to_string(),
            sender: "user".to_string(),
            metadata: Value::Null,
        };

        let result = chain
            .run_after_agent("start".to_string(), &ctx)
            .await
            .unwrap();
        assert_eq!(result, "start_A_B"); // A runs first, then B
    }

    #[test]
    fn test_middleware_chain_default() {
        let chain = MiddlewareChain::default();
        assert!(chain.is_empty());
    }

    #[test]
    fn test_middleware_chain_add_and_len() {
        let mut chain = MiddlewareChain::new();
        assert_eq!(chain.len(), 0);
        chain.add(Arc::new(LoggingMiddleware));
        assert_eq!(chain.len(), 1);
        assert!(!chain.is_empty());
    }

    #[test]
    fn test_middleware_names() {
        assert_eq!(LoggingMiddleware.name(), "logging");
        assert_eq!(ToolCallLimitMiddleware::new(5).name(), "tool_call_limit");
        assert_eq!(
            ToolOutputTruncationMiddleware::new(100).name(),
            "tool_output_truncation"
        );
    }

    #[tokio::test]
    async fn test_chain_run_before_tool_skip() {
        struct SkipMiddleware;

        #[async_trait]
        impl AgentMiddleware for SkipMiddleware {
            fn name(&self) -> &str {
                "skip"
            }
            async fn before_tool(
                &self,
                _tool_name: &str,
                _input: Value,
                _ctx: &MiddlewareContext,
            ) -> Result<Option<Value>> {
                Ok(None)
            }
        }

        let mut chain = MiddlewareChain::new();
        chain.add(Arc::new(SkipMiddleware));

        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "ch".to_string(),
            sender: "u".to_string(),
            metadata: Value::Null,
        };

        let result = chain
            .run_before_tool("any_tool", serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_chain_run_after_tool() {
        let mut chain = MiddlewareChain::new();
        chain.add(Arc::new(ToolOutputTruncationMiddleware::new(5)));

        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "ch".to_string(),
            sender: "u".to_string(),
            metadata: Value::Null,
        };

        let result = chain
            .run_after_tool("tool", "very long output text".to_string(), &ctx)
            .await
            .unwrap();
        assert!(result.contains("[Output truncated]"));
    }

    #[tokio::test]
    async fn test_chain_run_after_model_empty() {
        let chain = MiddlewareChain::new();
        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "ch".to_string(),
            sender: "u".to_string(),
            metadata: Value::Null,
        };

        let content = chain.run_after_model(vec![], &ctx).await.unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_middleware_context_debug() {
        let ctx = MiddlewareContext {
            query: "hello".to_string(),
            channel: "discord".to_string(),
            sender: "alice".to_string(),
            metadata: serde_json::json!({"key": "val"}),
        };
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("hello"));
        assert!(debug.contains("discord"));
        assert!(debug.contains("alice"));
    }

    #[test]
    fn test_middleware_context_clone() {
        let ctx = MiddlewareContext {
            query: "q".to_string(),
            channel: "c".to_string(),
            sender: "s".to_string(),
            metadata: Value::Null,
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.query, "q");
        assert_eq!(cloned.channel, "c");
    }

    #[tokio::test]
    async fn test_tool_call_limit_zero() {
        let mw = ToolCallLimitMiddleware::new(0);
        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "ch".to_string(),
            sender: "u".to_string(),
            metadata: Value::Null,
        };
        // Even the first call should be blocked with limit 0
        let result = mw.before_tool("tool", Value::Null, &ctx).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_truncation_exact_boundary() {
        let mw = ToolOutputTruncationMiddleware::new(5);
        let ctx = MiddlewareContext {
            query: "test".to_string(),
            channel: "ch".to_string(),
            sender: "u".to_string(),
            metadata: Value::Null,
        };
        // Exactly at limit should not truncate
        let result = mw.after_tool("t", "12345".to_string(), &ctx).await.unwrap();
        assert_eq!(result, "12345");

        // One over should truncate
        let result = mw
            .after_tool("t", "123456".to_string(), &ctx)
            .await
            .unwrap();
        assert!(result.contains("[Output truncated]"));
    }
}
