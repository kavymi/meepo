//! Sandbox execution tool — run code in Docker containers

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::sandbox::{DockerSandbox, SandboxConfig};
use crate::tools::{ToolHandler, json_schema};

/// Tool for executing code in a sandboxed Docker container
pub struct SandboxExecTool {
    sandbox: Arc<DockerSandbox>,
}

impl SandboxExecTool {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            sandbox: Arc::new(DockerSandbox::new(config)),
        }
    }
}

#[async_trait]
impl ToolHandler for SandboxExecTool {
    fn name(&self) -> &str {
        "sandbox_exec"
    }

    fn description(&self) -> &str {
        "Execute code in a sandboxed Docker container. Supports Python, JavaScript, TypeScript, Rust, Go, and Bash. Code runs in an isolated environment with resource limits and no network access by default."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "language": {
                    "type": "string",
                    "description": "Programming language: python, javascript, typescript, rust, go, bash",
                    "enum": ["python", "javascript", "typescript", "rust", "go", "bash"]
                },
                "code": {
                    "type": "string",
                    "description": "The code to execute"
                }
            }),
            vec!["language", "code"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let language = input
            .get("language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'language' parameter"))?;

        let code = input
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'code' parameter"))?;

        if code.trim().is_empty() {
            return Err(anyhow::anyhow!("Code cannot be empty"));
        }

        let result = self.sandbox.execute(language, code, None).await?;

        let mut output = String::new();

        if !result.stdout.is_empty() {
            output.push_str("=== STDOUT ===\n");
            output.push_str(&result.stdout);
            output.push('\n');
        }

        if !result.stderr.is_empty() {
            output.push_str("=== STDERR ===\n");
            output.push_str(&result.stderr);
            output.push('\n');
        }

        if result.timed_out {
            output.push_str("⚠ Execution timed out\n");
        }

        output.push_str(&format!(
            "\nExit code: {} | Duration: {}ms",
            result.exit_code, result.duration_ms
        ));

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_exec_tool_schema() {
        let tool = SandboxExecTool::new(SandboxConfig::default());
        assert_eq!(tool.name(), "sandbox_exec");

        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("language")));
        assert!(required.iter().any(|v| v.as_str() == Some("code")));
    }

    #[tokio::test]
    async fn test_sandbox_exec_missing_language() {
        let tool = SandboxExecTool::new(SandboxConfig::default());
        let result = tool
            .execute(serde_json::json!({"code": "print('hello')"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sandbox_exec_empty_code() {
        let tool = SandboxExecTool::new(SandboxConfig::default());
        let result = tool
            .execute(serde_json::json!({"language": "python", "code": "  "}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sandbox_exec_disabled() {
        let tool = SandboxExecTool::new(SandboxConfig::default());
        let result = tool
            .execute(serde_json::json!({"language": "python", "code": "print('hello')"}))
            .await;
        // Should fail because sandbox is disabled by default
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not enabled"));
    }
}
