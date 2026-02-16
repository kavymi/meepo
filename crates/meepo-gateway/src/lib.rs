//! meepo-gateway — WebSocket control plane for Meepo
//!
//! Provides a WebSocket server that clients (WebChat, macOS app, mobile nodes)
//! connect to for real-time chat, session management, and event streaming.

pub mod auth;
pub mod events;
pub mod protocol;
pub mod server;
pub mod session;
pub mod session_tools;
pub mod webchat;

pub use server::GatewayServer;
pub use session_tools::{
    AgentToAgentConfig, AgentsListTool, SessionsHistoryTool, SessionsListTool, SessionsSendTool,
    SessionsSpawnTool,
};
