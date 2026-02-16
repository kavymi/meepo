//! AppleScript execution helpers with timeout and retry logic

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

/// Run an AppleScript with configurable timeout and retry logic.
/// Retries up to `max_retries` times with exponential backoff (2s, 4s, 8s...).
pub async fn run_applescript(script: &str, timeout_secs: u64, max_retries: u32) -> Result<String> {
    let mut last_err = anyhow::anyhow!("AppleScript execution failed");

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let backoff = Duration::from_secs(2u64.pow(attempt));
            debug!(
                "Retrying AppleScript (attempt {}/{}, backoff {:?})",
                attempt + 1,
                max_retries + 1,
                backoff
            );
            tokio::time::sleep(backoff).await;
        }

        match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            Command::new("osascript").arg("-e").arg(script).output(),
        )
        .await
        {
            Ok(Ok(output)) if output.status.success() => {
                return Ok(String::from_utf8_lossy(&output.stdout).to_string());
            }
            Ok(Ok(output)) => {
                let error = String::from_utf8_lossy(&output.stderr).to_string();
                warn!("AppleScript failed (attempt {}): {}", attempt + 1, error);
                last_err = anyhow::anyhow!("AppleScript failed: {}", error);
            }
            Ok(Err(e)) => {
                warn!("osascript process error (attempt {}): {}", attempt + 1, e);
                last_err = anyhow::anyhow!("Failed to execute osascript: {}", e);
            }
            Err(_) => {
                warn!(
                    "AppleScript timed out after {}s (attempt {})",
                    timeout_secs,
                    attempt + 1
                );
                last_err = anyhow::anyhow!(
                    "AppleScript execution timed out after {} seconds",
                    timeout_secs
                );
            }
        }
    }

    Err(last_err)
}

/// Check if an application is currently running via System Events
pub async fn is_app_running(app_name: &str) -> bool {
    let script = format!(
        r#"tell application "System Events" to (name of processes) contains "{}""#,
        app_name
    );
    match tokio::time::timeout(
        Duration::from_secs(10),
        Command::new("osascript").arg("-e").arg(&script).output(),
    )
    .await
    {
        Ok(Ok(output)) => String::from_utf8_lossy(&output.stdout).trim() == "true",
        _ => false,
    }
}

/// Ensure an app is running before executing a heavy query.
/// If not running, launches it and waits for it to be ready.
pub async fn ensure_app_running(app_name: &str) -> Result<()> {
    if is_app_running(app_name).await {
        return Ok(());
    }

    debug!("{} not running, launching...", app_name);
    let launch_script = format!(r#"tell application "{}" to activate"#, app_name);
    let _ = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new("osascript")
            .arg("-e")
            .arg(&launch_script)
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Timed out launching {}", app_name))?
    .context(format!("Failed to launch {}", app_name))?;

    // Wait for app to finish launching (poll up to 30s)
    for _ in 0..15 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if is_app_running(app_name).await {
            debug!("{} is now running", app_name);
            tokio::time::sleep(Duration::from_secs(3)).await;
            return Ok(());
        }
    }

    warn!(
        "{} may not have fully launched, proceeding anyway",
        app_name
    );
    Ok(())
}

/// Sanitize a string for safe use in AppleScript
pub fn sanitize(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
        .chars()
        .filter(|&c| c >= ' ' || c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_quotes() {
        assert_eq!(sanitize(r#"Hello "world""#), r#"Hello \"world\""#);
    }

    #[test]
    fn test_sanitize_backslash() {
        assert_eq!(sanitize(r"path\to\file"), r"path\\to\\file");
    }

    #[test]
    fn test_sanitize_newlines() {
        assert_eq!(sanitize("line1\nline2\rline3"), "line1 line2 line3");
    }

    #[test]
    fn test_sanitize_control_chars() {
        assert_eq!(sanitize("hello\x01\x02world"), "helloworld");
    }

    #[test]
    fn test_sanitize_tabs_preserved() {
        assert_eq!(sanitize("col1\tcol2"), "col1\tcol2");
    }
}
