//! Provider-agnostic types for multi-model LLM support

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::ToolDefinition;

/// Provider-agnostic chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: ChatMessageContent,
}

/// Message role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

/// Content of a chat message — either plain text or structured blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatMessageContent {
    Text(String),
    Blocks(Vec<ChatBlock>),
}

/// A single block within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatBlock {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

/// Provider-agnostic response from an LLM
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub blocks: Vec<ChatResponseBlock>,
    pub stop_reason: StopReason,
    pub usage: ChatUsage,
}

/// A block in the response
#[derive(Debug, Clone)]
pub enum ChatResponseBlock {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
}

/// Why the model stopped generating
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Unknown,
}

/// Token usage from a single API call
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Trait that all LLM providers implement
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Human-readable provider name (e.g. "anthropic", "openai")
    fn provider_name(&self) -> &str;

    /// Model identifier (e.g. "claude-opus-4-6", "gpt-4o")
    fn model(&self) -> &str;

    /// Send a chat request with optional tools and system prompt
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<ChatResponse>;
}

impl std::fmt::Display for ChatRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::System => write!(f, "system"),
        }
    }
}

impl StopReason {
    /// Whether the model wants to call tools
    pub fn is_tool_use(&self) -> bool {
        matches!(self, Self::ToolUse)
    }

    /// Whether the model finished its turn
    pub fn is_end_turn(&self) -> bool {
        matches!(self, Self::EndTurn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_role_display() {
        assert_eq!(ChatRole::User.to_string(), "user");
        assert_eq!(ChatRole::Assistant.to_string(), "assistant");
        assert_eq!(ChatRole::System.to_string(), "system");
    }

    #[test]
    fn test_stop_reason_predicates() {
        assert!(StopReason::ToolUse.is_tool_use());
        assert!(!StopReason::EndTurn.is_tool_use());
        assert!(StopReason::EndTurn.is_end_turn());
        assert!(!StopReason::ToolUse.is_end_turn());
    }

    #[test]
    fn test_chat_message_text() {
        let msg = ChatMessage {
            role: ChatRole::User,
            content: ChatMessageContent::Text("hello".to_string()),
        };
        assert_eq!(msg.role, ChatRole::User);
        if let ChatMessageContent::Text(t) = &msg.content {
            assert_eq!(t, "hello");
        } else {
            panic!("expected text content");
        }
    }

    #[test]
    fn test_chat_usage_default() {
        let usage = ChatUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    #[test]
    fn test_chat_role_serde_roundtrip() {
        let roles = [ChatRole::User, ChatRole::Assistant, ChatRole::System];
        for role in &roles {
            let json = serde_json::to_string(role).unwrap();
            let parsed: ChatRole = serde_json::from_str(&json).unwrap();
            assert_eq!(*role, parsed);
        }
    }

    #[test]
    fn test_chat_message_content_text_serde() {
        let content = ChatMessageContent::Text("hello world".to_string());
        let json = serde_json::to_string(&content).unwrap();
        let parsed: ChatMessageContent = serde_json::from_str(&json).unwrap();
        if let ChatMessageContent::Text(t) = parsed {
            assert_eq!(t, "hello world");
        } else {
            panic!("expected text content");
        }
    }

    #[test]
    fn test_chat_message_serde_roundtrip() {
        let msg = ChatMessage {
            role: ChatRole::Assistant,
            content: ChatMessageContent::Text("response".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, ChatRole::Assistant);
    }

    #[test]
    fn test_stop_reason_all_variants() {
        assert!(!StopReason::MaxTokens.is_tool_use());
        assert!(!StopReason::MaxTokens.is_end_turn());
        assert!(!StopReason::Unknown.is_tool_use());
        assert!(!StopReason::Unknown.is_end_turn());
    }

    #[test]
    fn test_chat_block_variants() {
        let text = ChatBlock::Text {
            text: "hello".to_string(),
        };
        let tool_call = ChatBlock::ToolCall {
            id: "tc_1".to_string(),
            name: "search".to_string(),
            input: serde_json::json!({"query": "test"}),
        };
        let tool_result = ChatBlock::ToolResult {
            tool_call_id: "tc_1".to_string(),
            content: "result".to_string(),
        };

        // Verify they serialize without panic
        let _ = serde_json::to_string(&text).unwrap();
        let _ = serde_json::to_string(&tool_call).unwrap();
        let _ = serde_json::to_string(&tool_result).unwrap();
    }

    #[test]
    fn test_chat_message_blocks_content() {
        let msg = ChatMessage {
            role: ChatRole::Assistant,
            content: ChatMessageContent::Blocks(vec![
                ChatBlock::Text {
                    text: "Let me search".to_string(),
                },
                ChatBlock::ToolCall {
                    id: "tc_1".to_string(),
                    name: "web_search".to_string(),
                    input: serde_json::json!({"q": "rust"}),
                },
            ]),
        };
        if let ChatMessageContent::Blocks(blocks) = &msg.content {
            assert_eq!(blocks.len(), 2);
        } else {
            panic!("expected blocks content");
        }
    }
}
