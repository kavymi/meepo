//! Docker sandbox — run code in isolated Docker containers

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::policy::{ExecutionPolicy, ResourceLimits};

/// Configuration for the Docker sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_docker_socket")]
    pub docker_socket: String,
    #[serde(default)]
    pub policy: ExecutionPolicy,
}

fn default_docker_socket() -> String {
    "/var/run/docker.sock".to_string()
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            docker_socket: default_docker_socket(),
            policy: ExecutionPolicy::default(),
        }
    }
}

/// Result of a sandboxed execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
    pub duration_ms: u64,
}

/// Docker sandbox for secure code execution
pub struct DockerSandbox {
    config: SandboxConfig,
}

impl DockerSandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Check if Docker is available on the system
    pub async fn is_available(&self) -> bool {
        Command::new("docker")
            .arg("info")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Execute code in a sandboxed Docker container
    pub async fn execute(
        &self,
        language: &str,
        code: &str,
        limits: Option<ResourceLimits>,
    ) -> Result<SandboxResult> {
        if !self.config.enabled {
            return Err(anyhow!("Docker sandbox is not enabled in configuration"));
        }

        let policy = &self.config.policy;
        let limits = limits.unwrap_or_else(|| policy.resource_limits.clone());

        // Validate language
        if !policy.is_language_allowed(language) {
            return Err(anyhow!(
                "Language '{}' is not allowed. Allowed: {:?}",
                language,
                policy.allowed_languages
            ));
        }

        // Get image and command
        let image = policy
            .default_image_for(language)
            .ok_or_else(|| anyhow!("No Docker image configured for language '{}'", language))?;

        if !policy.is_image_allowed(image) {
            return Err(anyhow!("Docker image '{}' is not in the allowlist", image));
        }

        let run_cmd = policy
            .run_command_for(language)
            .ok_or_else(|| anyhow!("No run command configured for language '{}'", language))?;

        let ext = policy
            .file_extension_for(language)
            .ok_or_else(|| anyhow!("No file extension for language '{}'", language))?;

        // Validate code size
        if code.len() > 100_000 {
            return Err(anyhow!("Code too large (max 100KB)"));
        }

        debug!(
            "Sandbox: executing {} code ({} bytes) in {}",
            language,
            code.len(),
            image
        );

        // Write code to a temp file
        let temp_dir = std::env::temp_dir();
        let code_file = temp_dir.join(format!("meepo_sandbox_{}.{}", uuid::Uuid::new_v4(), ext));
        tokio::fs::write(&code_file, code)
            .await
            .context("Failed to write code to temp file")?;

        let start = std::time::Instant::now();

        // Build docker run command
        let container_name = format!("meepo-sandbox-{}", uuid::Uuid::new_v4());
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--name".to_string(),
            container_name.clone(),
            // Resource limits
            "--memory".to_string(),
            format!("{}m", limits.memory_mb),
            "--cpu-shares".to_string(),
            limits.cpu_shares.to_string(),
            "--pids-limit".to_string(),
            limits.max_pids.to_string(),
        ];

        // Network isolation
        if !limits.network_enabled {
            args.push("--network".to_string());
            args.push("none".to_string());
        }

        // Read-only root filesystem
        if limits.read_only_root {
            args.push("--read-only".to_string());
            args.push("--tmpfs".to_string());
            args.push("/tmp:rw,noexec,nosuid,size=64m".to_string());
        }

        // Security options
        args.push("--security-opt".to_string());
        args.push("no-new-privileges".to_string());
        args.push("--cap-drop".to_string());
        args.push("ALL".to_string());

        // Mount code file
        args.push("-v".to_string());
        args.push(format!("{}:/tmp/code.{}:ro", code_file.display(), ext));

        // Image
        args.push(image.to_string());

        // Command
        args.extend(run_cmd);

        // Execute with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(limits.timeout_secs),
            Command::new("docker").args(&args).output(),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&code_file).await;

        match result {
            Ok(Ok(output)) => {
                let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();

                // Truncate output if too large
                if stdout.len() > limits.max_output_bytes {
                    stdout.truncate(limits.max_output_bytes);
                    stdout.push_str("\n... [output truncated]");
                }
                if stderr.len() > limits.max_output_bytes {
                    stderr.truncate(limits.max_output_bytes);
                    stderr.push_str("\n... [output truncated]");
                }

                let exit_code = output.status.code().unwrap_or(-1);

                info!(
                    "Sandbox: {} execution completed (exit={}, {}ms)",
                    language, exit_code, duration_ms
                );

                Ok(SandboxResult {
                    stdout,
                    stderr,
                    exit_code,
                    timed_out: false,
                    duration_ms,
                })
            }
            Ok(Err(e)) => {
                warn!("Sandbox: Docker execution failed: {}", e);
                Err(anyhow!("Docker execution failed: {}", e))
            }
            Err(_) => {
                // Timeout — kill the container
                warn!(
                    "Sandbox: execution timed out after {}s, killing container",
                    limits.timeout_secs
                );
                let _ = Command::new("docker")
                    .args(["kill", &container_name])
                    .output()
                    .await;

                Ok(SandboxResult {
                    stdout: String::new(),
                    stderr: format!("Execution timed out after {} seconds", limits.timeout_secs),
                    exit_code: -1,
                    timed_out: true,
                    duration_ms,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.docker_socket, "/var/run/docker.sock");
    }

    #[test]
    fn test_sandbox_result_serialize() {
        let result = SandboxResult {
            stdout: "hello".to_string(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 100,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"exit_code\":0"));
    }

    #[tokio::test]
    async fn test_sandbox_disabled() {
        let sandbox = DockerSandbox::new(SandboxConfig::default());
        let result = sandbox.execute("python", "print('hello')", None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not enabled"));
    }

    #[tokio::test]
    async fn test_sandbox_invalid_language() {
        let mut config = SandboxConfig::default();
        config.enabled = true;
        let sandbox = DockerSandbox::new(config);
        let result = sandbox
            .execute("haskell", "main = putStrLn \"hello\"", None)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_sandbox_code_too_large() {
        let mut config = SandboxConfig::default();
        config.enabled = true;
        let sandbox = DockerSandbox::new(config);
        let large_code = "x".repeat(200_000);
        let result = sandbox.execute("python", &large_code, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }
}
