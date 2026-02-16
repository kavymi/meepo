//! Docker sandbox — secure code execution in isolated containers
//!
//! Provides a sandboxed execution environment using Docker containers
//! for running untrusted code safely. Addresses OpenClaw's need for
//! isolated code execution without risking the host system.

pub mod docker;
pub mod policy;

pub use docker::{DockerSandbox, SandboxConfig, SandboxResult};
pub use policy::{ExecutionPolicy, ResourceLimits};
