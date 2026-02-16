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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecretsProviderType {
    Env,
    File,
    Memory,
}

impl Default for SecretsProviderType {
    fn default() -> Self {
        Self::Env
    }
}

/// Environment variable secrets provider
pub struct EnvSecretsProvider;

#[async_trait]
impl SecretsProvider for EnvSecretsProvider {
    fn name(&self) -> &str {
        "env"
    }

    async fn get(&self, key: &str) -> Result<Option<String>> {
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
        let canonical_dir = self.secrets_dir.canonicalize().unwrap_or_else(|_| self.secrets_dir.clone());
        if let Ok(canonical_path) = path.canonicalize() {
            if !canonical_path.starts_with(&canonical_dir) {
                return Err(anyhow!("Secret key resolves outside secrets directory"));
            }
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
                let dir = config
                    .secrets_dir
                    .as_deref()
                    .unwrap_or("/run/secrets");
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
        // PATH should always exist
        let result = provider.get("PATH").await.unwrap();
        assert!(result.is_some());

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
        let result = mgr
            .expand("key=$secret{MISSING}")
            .await
            .unwrap();
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
}
