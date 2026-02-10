//! MCP (Model Context Protocol) support for Meepo
//!
//! Provides both server (expose Meepo tools to MCP clients) and client
//! (consume tools from external MCP servers) functionality.

pub mod adapter;
pub mod client;
pub mod protocol;
pub mod server;

pub use adapter::McpToolAdapter;
pub use client::{McpClient, McpClientConfig};
pub use server::McpServer;
