//! Secrets manager — pluggable secrets resolution with $secret{NAME} syntax
//!
//! Inspired by OpenClaw PR #11539. Supports environment variables,
//! file-based secrets, and extensible provider backends.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// A secrets provider that can resolve secret names to values
#[async_trait]
pub trait SecretsProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn get(&self, key: &str) -> Result<Option<String>>;
}

/// Configuration for the secrets manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    #[serde(default)]
    pub provider: SecretsProviderType,
    #[serde(default)]
    pub secrets_dir: Option<String>,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            provider: SecretsProviderType::Env,
            secrets_dir: None,
        }
    }
}

/// Type of secrets provider
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecretsProviderType {
    #[default]
    Env,
    File,
    Memory,
}

/// Environment variable secrets provider.
///
/// Only variables in `ALLOWED_SECRET_ENV_VARS` can be resolved to prevent
/// prompt-injection attacks from exfiltrating arbitrary env vars (M-3 fix).
pub struct EnvSecretsProvider;

/// Allowlist of environment variable names that may be resolved as secrets.
/// This mirrors the config-level `ALLOWED_ENV_VARS` and prevents the agent
/// from being tricked into reading arbitrary env vars via `$secret{NAME}`.
const ALLOWED_SECRET_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "GOOGLE_AI_API_KEY",
    "CUSTOM_LLM_API_KEY",
    "TAVILY_API_KEY",
    "DISCORD_BOT_TOKEN",
    "SLACK_BOT_TOKEN",
    "A2A_AUTH_TOKEN",
    "OPENCLAW_A2A_TOKEN",
    "GITHUB_TOKEN",
    "MEEPO_GATEWAY_TOKEN",
    "ELEVENLABS_API_KEY",
    "HOME",
    "USERPROFILE",
    "USER",
];

#[async_trait]
impl SecretsProvider for EnvSecretsProvider {
    fn name(&self) -> &str {
        "env"
    }

    async fn get(&self, key: &str) -> Result<Option<String>> {
        // Only allow known-safe env vars to prevent exfiltration (M-3 fix)
        if !ALLOWED_SECRET_ENV_VARS.contains(&key) {
            tracing::warn!(
                "EnvSecretsProvider: rejected lookup for non-allowlisted env var '{}'",
                key
            );
            return Ok(None);
        }
        Ok(std::env::var(key).ok())
    }
}

/// File-based secrets provider (reads from a secrets directory)
pub struct FileSecretsProvider {
    secrets_dir: std::path::PathBuf,
}

impl FileSecretsProvider {
    pub fn new(secrets_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            secrets_dir: secrets_dir.into(),
        }
    }
}

#[async_trait]
impl SecretsProvider for FileSecretsProvider {
    fn name(&self) -> &str {
        "file"
    }

    async fn get(&self, key: &str) -> Result<Option<String>> {
        // Validate key to prevent path traversal
        if key.contains('/') || key.contains('\\') || key.contains("..") || key.contains('\0') {
            return Err(anyhow!("Invalid secret key: contains path separators"));
        }

        let path = self.secrets_dir.join(key);

        // Verify the resolved path is within secrets_dir
        let canonical_dir = self
            .secrets_dir
            .canonicalize()
            .unwrap_or_else(|_| self.secrets_dir.clone());
        if let Ok(canonical_path) = path.canonicalize()
            && !canonical_path.starts_with(&canonical_dir)
        {
            return Err(anyhow!("Secret key resolves outside secrets directory"));
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => Ok(Some(content.trim().to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(anyhow!("Failed to read secret '{}': {}", key, e)),
        }
    }
}

/// In-memory secrets provider (for testing)
pub struct MemorySecretsProvider {
    secrets: HashMap<String, String>,
}

impl MemorySecretsProvider {
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.secrets.insert(key.into(), value.into());
    }
}

impl Default for MemorySecretsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretsProvider for MemorySecretsProvider {
    fn name(&self) -> &str {
        "memory"
    }

    async fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.secrets.get(key).cloned())
    }
}

/// Secrets manager — resolves $secret{NAME} references in text
pub struct SecretsManager {
    provider: Box<dyn SecretsProvider>,
}

impl SecretsManager {
    pub fn new(provider: Box<dyn SecretsProvider>) -> Self {
        Self { provider }
    }

    /// Create from config
    pub fn from_config(config: &SecretsConfig) -> Self {
        let provider: Box<dyn SecretsProvider> = match config.provider {
            SecretsProviderType::Env => Box::new(EnvSecretsProvider),
            SecretsProviderType::File => {
                let dir = config.secrets_dir.as_deref().unwrap_or("/run/secrets");
                Box::new(FileSecretsProvider::new(dir))
            }
            SecretsProviderType::Memory => Box::new(MemorySecretsProvider::new()),
        };
        Self { provider }
    }

    /// Resolve a single secret by name
    pub async fn resolve(&self, key: &str) -> Result<Option<String>> {
        debug!("Resolving secret: {}", key);
        self.provider.get(key).await
    }

    /// Expand all $secret{NAME} references in a string
    pub async fn expand(&self, text: &str) -> Result<String> {
        let re: regex::Regex = regex::Regex::new(r"\$secret\{([^}]+)\}")
            .map_err(|e| anyhow!("Invalid regex: {}", e))?;

        // Collect all matches first (can't await inside captures_iter)
        let matches: Vec<(usize, usize, String)> = re
            .captures_iter(text)
            .filter_map(|cap| {
                let full = cap.get(0)?;
                let key = cap.get(1)?.as_str().to_string();
                Some((full.start(), full.end(), key))
            })
            .collect();

        let mut result = text.to_string();
        let mut offset: i64 = 0;

        for (start, end, key) in &matches {
            match self.provider.get(key).await? {
                Some(value) => {
                    let adj_start = (*start as i64 + offset) as usize;
                    let adj_end = (*end as i64 + offset) as usize;
                    let match_len = adj_end - adj_start;
                    result.replace_range(adj_start..adj_end, &value);
                    offset += value.len() as i64 - match_len as i64;
                    debug!("Expanded secret: {} ({} chars)", key, value.len());
                }
                None => {
                    warn!("Secret '{}' not found", key);
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_env_provider() {
        let provider = EnvSecretsProvider;
        // Use a platform-appropriate env var that should always exist
        #[cfg(not(target_os = "windows"))]
        let result = provider.get("HOME").await.unwrap();
        #[cfg(target_os = "windows")]
        let result = provider.get("USERPROFILE").await.unwrap();
        assert!(result.is_some());

        // Non-allowlisted env var should return None even if it exists
        let blocked = provider.get("PATH").await.unwrap();
        assert!(blocked.is_none());

        let missing = provider.get("MEEPO_NONEXISTENT_KEY_12345").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_memory_provider() {
        let mut provider = MemorySecretsProvider::new();
        provider.set("API_KEY", "sk-test-123");

        let result = provider.get("API_KEY").await.unwrap();
        assert_eq!(result, Some("sk-test-123".to_string()));

        let missing = provider.get("MISSING").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_file_provider_path_traversal() {
        let provider = FileSecretsProvider::new("/tmp/secrets");
        let result = provider.get("../etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_provider_missing() {
        let provider = FileSecretsProvider::new("/tmp/meepo_test_secrets_nonexistent");
        let result = provider.get("missing_key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_secrets_manager_expand() {
        let mut provider = MemorySecretsProvider::new();
        provider.set("DB_HOST", "localhost");
        provider.set("DB_PORT", "5432");

        let mgr = SecretsManager::new(Box::new(provider));
        let result = mgr
            .expand("host=$secret{DB_HOST} port=$secret{DB_PORT}")
            .await
            .unwrap();
        assert_eq!(result, "host=localhost port=5432");
    }

    #[tokio::test]
    async fn test_secrets_manager_expand_missing() {
        let provider = MemorySecretsProvider::new();
        let mgr = SecretsManager::new(Box::new(provider));
        let result = mgr.expand("key=$secret{MISSING}").await.unwrap();
        // Missing secrets are left as-is
        assert_eq!(result, "key=$secret{MISSING}");
    }

    #[tokio::test]
    async fn test_secrets_manager_expand_no_secrets() {
        let provider = MemorySecretsProvider::new();
        let mgr = SecretsManager::new(Box::new(provider));
        let result = mgr.expand("no secrets here").await.unwrap();
        assert_eq!(result, "no secrets here");
    }

    #[tokio::test]
    async fn test_secrets_manager_resolve() {
        let mut provider = MemorySecretsProvider::new();
        provider.set("KEY", "value");
        let mgr = SecretsManager::new(Box::new(provider));

        assert_eq!(mgr.resolve("KEY").await.unwrap(), Some("value".to_string()));
        assert_eq!(mgr.resolve("MISSING").await.unwrap(), None);
    }

    #[test]
    fn test_secrets_config_default() {
        let config = SecretsConfig::default();
        assert_eq!(config.provider, SecretsProviderType::Env);
        assert!(config.secrets_dir.is_none());
    }

    #[test]
    fn test_secrets_config_serde_roundtrip() {
        let config = SecretsConfig {
            provider: SecretsProviderType::File,
            secrets_dir: Some("/run/secrets".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SecretsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, SecretsProviderType::File);
        assert_eq!(parsed.secrets_dir.as_deref(), Some("/run/secrets"));
    }

    #[test]
    fn test_secrets_provider_type_serde() {
        let json = serde_json::to_string(&SecretsProviderType::Memory).unwrap();
        assert_eq!(json, "\"memory\"");
        let parsed: SecretsProviderType = serde_json::from_str("\"env\"").unwrap();
        assert_eq!(parsed, SecretsProviderType::Env);
        let parsed: SecretsProviderType = serde_json::from_str("\"file\"").unwrap();
        assert_eq!(parsed, SecretsProviderType::File);
    }

    #[test]
    fn test_from_config_env() {
        let config = SecretsConfig::default();
        let mgr = SecretsManager::from_config(&config);
        // Just verify it constructs without panic
        let _ = mgr;
    }

    #[test]
    fn test_from_config_file() {
        let config = SecretsConfig {
            provider: SecretsProviderType::File,
            secrets_dir: Some("/tmp/test_secrets".to_string()),
        };
        let mgr = SecretsManager::from_config(&config);
        let _ = mgr;
    }

    #[test]
    fn test_from_config_file_default_dir() {
        let config = SecretsConfig {
            provider: SecretsProviderType::File,
            secrets_dir: None,
        };
        let mgr = SecretsManager::from_config(&config);
        let _ = mgr;
    }

    #[test]
    fn test_from_config_memory() {
        let config = SecretsConfig {
            provider: SecretsProviderType::Memory,
            secrets_dir: None,
        };
        let mgr = SecretsManager::from_config(&config);
        let _ = mgr;
    }

    #[tokio::test]
    async fn test_file_provider_path_traversal_backslash() {
        let provider = FileSecretsProvider::new("/tmp/secrets");
        let result = provider.get("secret\\file").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_provider_path_traversal_null() {
        let provider = FileSecretsProvider::new("/tmp/secrets");
        let result = provider.get("secret\0file").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_provider_reads_real_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("my_key"), "  secret_value  \n").unwrap();

        let provider = FileSecretsProvider::new(dir.path());
        let result = provider.get("my_key").await.unwrap();
        assert_eq!(result, Some("secret_value".to_string()));
    }

    #[tokio::test]
    async fn test_env_provider_allowlist_enforcement() {
        let provider = EnvSecretsProvider;
        // These should all return None (not on allowlist) even if they exist
        for var in &["PATH", "SHELL", "TERM", "LANG", "PWD"] {
            let result = provider.get(var).await.unwrap();
            assert!(
                result.is_none(),
                "Expected None for non-allowlisted var {}",
                var
            );
        }
    }

    #[tokio::test]
    async fn test_secrets_manager_expand_multiple_same_key() {
        let mut provider = MemorySecretsProvider::new();
        provider.set("TOKEN", "abc123");

        let mgr = SecretsManager::new(Box::new(provider));
        let result = mgr
            .expand("first=$secret{TOKEN} second=$secret{TOKEN}")
            .await
            .unwrap();
        assert_eq!(result, "first=abc123 second=abc123");
    }

    #[tokio::test]
    async fn test_secrets_manager_expand_adjacent() {
        let mut provider = MemorySecretsProvider::new();
        provider.set("A", "x");
        provider.set("B", "y");

        let mgr = SecretsManager::new(Box::new(provider));
        let result = mgr.expand("$secret{A}$secret{B}").await.unwrap();
        assert_eq!(result, "xy");
    }

    #[test]
    fn test_memory_provider_default() {
        let provider = MemorySecretsProvider::default();
        assert_eq!(provider.name(), "memory");
    }

    #[test]
    fn test_env_provider_name() {
        let provider = EnvSecretsProvider;
        assert_eq!(provider.name(), "env");
    }

    #[test]
    fn test_file_provider_name() {
        let provider = FileSecretsProvider::new("/tmp");
        assert_eq!(provider.name(), "file");
    }
}
