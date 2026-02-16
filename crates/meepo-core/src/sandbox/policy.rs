//! Execution policy — resource limits and security constraints for sandboxed execution

use serde::{Deserialize, Serialize};

/// Resource limits for a sandboxed execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub memory_mb: u64,
    pub cpu_shares: u64,
    pub timeout_secs: u64,
    pub max_output_bytes: usize,
    pub network_enabled: bool,
    pub read_only_root: bool,
    pub max_pids: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_mb: 256,
            cpu_shares: 512,
            timeout_secs: 30,
            max_output_bytes: 1024 * 1024, // 1MB
            network_enabled: false,
            read_only_root: true,
            max_pids: 64,
        }
    }
}

/// Execution policy — what languages and operations are allowed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub allowed_languages: Vec<String>,
    pub allowed_images: Vec<String>,
    pub resource_limits: ResourceLimits,
    pub mount_workspace: bool,
    pub workspace_read_only: bool,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            allowed_languages: vec![
                "python".to_string(),
                "javascript".to_string(),
                "typescript".to_string(),
                "bash".to_string(),
                "rust".to_string(),
                "go".to_string(),
            ],
            allowed_images: vec![
                "python:3.12-slim".to_string(),
                "node:22-slim".to_string(),
                "rust:1-slim".to_string(),
                "golang:1-alpine".to_string(),
                "alpine:latest".to_string(),
            ],
            resource_limits: ResourceLimits::default(),
            mount_workspace: false,
            workspace_read_only: true,
        }
    }
}

impl ExecutionPolicy {
    /// Check if a language is allowed
    pub fn is_language_allowed(&self, language: &str) -> bool {
        self.allowed_languages
            .iter()
            .any(|l| l.eq_ignore_ascii_case(language))
    }

    /// Check if a Docker image is allowed
    pub fn is_image_allowed(&self, image: &str) -> bool {
        self.allowed_images
            .iter()
            .any(|i| i == image)
    }

    /// Get the default Docker image for a language
    pub fn default_image_for(&self, language: &str) -> Option<&str> {
        match language.to_lowercase().as_str() {
            "python" | "python3" => Some("python:3.12-slim"),
            "javascript" | "js" | "node" => Some("node:22-slim"),
            "typescript" | "ts" => Some("node:22-slim"),
            "rust" => Some("rust:1-slim"),
            "go" | "golang" => Some("golang:1-alpine"),
            "bash" | "sh" | "shell" => Some("alpine:latest"),
            _ => None,
        }
    }

    /// Get the command to run code in a given language
    pub fn run_command_for(&self, language: &str) -> Option<Vec<String>> {
        match language.to_lowercase().as_str() {
            "python" | "python3" => Some(vec!["python3".to_string(), "/tmp/code.py".to_string()]),
            "javascript" | "js" | "node" => {
                Some(vec!["node".to_string(), "/tmp/code.js".to_string()])
            }
            "typescript" | "ts" => Some(vec![
                "sh".to_string(),
                "-c".to_string(),
                "npx tsx /tmp/code.ts".to_string(),
            ]),
            "rust" => Some(vec![
                "sh".to_string(),
                "-c".to_string(),
                "rustc /tmp/code.rs -o /tmp/code && /tmp/code".to_string(),
            ]),
            "go" | "golang" => Some(vec!["go".to_string(), "run".to_string(), "/tmp/code.go".to_string()]),
            "bash" | "sh" | "shell" => Some(vec!["sh".to_string(), "/tmp/code.sh".to_string()]),
            _ => None,
        }
    }

    /// Get the file extension for a language
    pub fn file_extension_for(&self, language: &str) -> Option<&str> {
        match language.to_lowercase().as_str() {
            "python" | "python3" => Some("py"),
            "javascript" | "js" | "node" => Some("js"),
            "typescript" | "ts" => Some("ts"),
            "rust" => Some("rs"),
            "go" | "golang" => Some("go"),
            "bash" | "sh" | "shell" => Some("sh"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.memory_mb, 256);
        assert_eq!(limits.timeout_secs, 30);
        assert!(!limits.network_enabled);
        assert!(limits.read_only_root);
    }

    #[test]
    fn test_execution_policy_default() {
        let policy = ExecutionPolicy::default();
        assert!(policy.is_language_allowed("python"));
        assert!(policy.is_language_allowed("JavaScript"));
        assert!(!policy.is_language_allowed("haskell"));
    }

    #[test]
    fn test_default_image_for() {
        let policy = ExecutionPolicy::default();
        assert_eq!(policy.default_image_for("python"), Some("python:3.12-slim"));
        assert_eq!(policy.default_image_for("JavaScript"), Some("node:22-slim"));
        assert_eq!(policy.default_image_for("rust"), Some("rust:1-slim"));
        assert_eq!(policy.default_image_for("haskell"), None);
    }

    #[test]
    fn test_run_command_for() {
        let policy = ExecutionPolicy::default();
        let cmd = policy.run_command_for("python").unwrap();
        assert_eq!(cmd[0], "python3");

        assert!(policy.run_command_for("haskell").is_none());
    }

    #[test]
    fn test_file_extension_for() {
        let policy = ExecutionPolicy::default();
        assert_eq!(policy.file_extension_for("python"), Some("py"));
        assert_eq!(policy.file_extension_for("typescript"), Some("ts"));
        assert_eq!(policy.file_extension_for("unknown"), None);
    }

    #[test]
    fn test_image_allowed() {
        let policy = ExecutionPolicy::default();
        assert!(policy.is_image_allowed("python:3.12-slim"));
        assert!(!policy.is_image_allowed("ubuntu:latest"));
    }
}
