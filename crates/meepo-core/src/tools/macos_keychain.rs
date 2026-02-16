//! macOS Keychain tools — get and store passwords

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::KeychainProvider;

pub struct KeychainGetPasswordTool {
    provider: Box<dyn KeychainProvider>,
}

impl KeychainGetPasswordTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_keychain_provider()
                .expect("Keychain provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for KeychainGetPasswordTool {
    fn name(&self) -> &str {
        "keychain_get_password"
    }

    fn description(&self) -> &str {
        "Retrieve a password from the macOS Keychain by service and account name. macOS may prompt the user for approval."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "service": {
                    "type": "string",
                    "description": "Service name (e.g., 'my-app', 'github.com')"
                },
                "account": {
                    "type": "string",
                    "description": "Account name (e.g., username or email)"
                }
            }),
            vec!["service", "account"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let service = input
            .get("service")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'service' parameter"))?;
        let account = input
            .get("account")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'account' parameter"))?;
        debug!("Getting keychain password for service: {}", service);
        self.provider.get_password(service, account).await
    }
}

pub struct KeychainStorePasswordTool {
    provider: Box<dyn KeychainProvider>,
}

impl KeychainStorePasswordTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_keychain_provider()
                .expect("Keychain provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for KeychainStorePasswordTool {
    fn name(&self) -> &str {
        "keychain_store_password"
    }

    fn description(&self) -> &str {
        "Store a password in the macOS Keychain under a service and account name."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "service": {
                    "type": "string",
                    "description": "Service name (e.g., 'my-app', 'github.com')"
                },
                "account": {
                    "type": "string",
                    "description": "Account name (e.g., username or email)"
                },
                "password": {
                    "type": "string",
                    "description": "The password to store"
                }
            }),
            vec!["service", "account", "password"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let service = input
            .get("service")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'service' parameter"))?;
        let account = input
            .get("account")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'account' parameter"))?;
        let password = input
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'password' parameter"))?;
        debug!("Storing keychain password for service: {}", service);
        self.provider.store_password(service, account, password).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_keychain_get_schema() {
        let tool = KeychainGetPasswordTool::new();
        assert_eq!(tool.name(), "keychain_get_password");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema.get("required").cloned().unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"service".to_string()));
        assert!(required.contains(&"account".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_keychain_get_missing_params() {
        let tool = KeychainGetPasswordTool::new();
        let result = tool.execute(serde_json::json!({"service": "test"})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_keychain_store_schema() {
        let tool = KeychainStorePasswordTool::new();
        assert_eq!(tool.name(), "keychain_store_password");
    }
}
