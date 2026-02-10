//! A2A (Agent-to-Agent) protocol support for Meepo
//!
//! Implements Google's Agent-to-Agent protocol for multi-agent task delegation.
//! Provides both server (receive tasks from peers) and client (send tasks to peers).

pub mod client;
pub mod protocol;
pub mod server;
pub mod tool;

pub use client::{A2aClient, PeerAgentConfig};
pub use protocol::{AgentCard, AuthConfig, TaskRequest, TaskResponse, TaskStatus};
pub use server::A2aServer;
pub use tool::DelegateToAgentTool;
