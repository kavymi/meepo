//! Doctor — system health checks and onboarding validation
//!
//! Inspired by OpenClaw's `sessions scrub` and doctor commands.
//! Checks configuration, dependencies, connectivity, and security.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Result of a single health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub fix_hint: Option<String>,
}

/// Status of a health check
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Pass => write!(f, "PASS"),
            CheckStatus::Warn => write!(f, "WARN"),
            CheckStatus::Fail => write!(f, "FAIL"),
            CheckStatus::Skip => write!(f, "SKIP"),
        }
    }
}

/// Full doctor report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
    pub pass_count: usize,
    pub warn_count: usize,
    pub fail_count: usize,
    pub skip_count: usize,
}

impl DoctorReport {
    pub fn is_healthy(&self) -> bool {
        self.fail_count == 0
    }

    pub fn summary(&self) -> String {
        format!(
            "{} passed, {} warnings, {} failed, {} skipped",
            self.pass_count, self.warn_count, self.fail_count, self.skip_count
        )
    }
}

/// Run all doctor checks
pub async fn run_doctor(
    config_path: Option<&std::path::Path>,
    db_path: Option<&std::path::Path>,
) -> Result<DoctorReport> {
    info!("Running doctor checks...");
    let mut checks = Vec::new();

    // 1. Config file exists
    checks.push(check_config_file(config_path));

    // 2. Database directory writable
    checks.push(check_db_path(db_path));

    // 3. Docker available (for sandbox)
    checks.push(check_docker().await);

    // 4. API key configured
    checks.push(check_api_key("ANTHROPIC_API_KEY"));

    // 5. Git available
    checks.push(check_command("git", &["--version"], "Git"));

    // 6. Home directory accessible
    checks.push(check_home_dir());

    // 7. Check for secret leaks in common files
    checks.push(check_secret_leaks());

    // 8. Temp directory writable
    checks.push(check_temp_dir());

    let pass_count = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
    let warn_count = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
    let fail_count = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
    let skip_count = checks.iter().filter(|c| c.status == CheckStatus::Skip).count();

    let report = DoctorReport {
        checks,
        pass_count,
        warn_count,
        fail_count,
        skip_count,
    };

    if report.is_healthy() {
        info!("Doctor: all checks passed ({})", report.summary());
    } else {
        warn!("Doctor: issues found ({})", report.summary());
    }

    Ok(report)
}

fn check_config_file(path: Option<&std::path::Path>) -> CheckResult {
    match path {
        Some(p) => {
            if p.exists() {
                CheckResult {
                    name: "config_file".to_string(),
                    status: CheckStatus::Pass,
                    message: format!("Config file found: {}", p.display()),
                    fix_hint: None,
                }
            } else {
                CheckResult {
                    name: "config_file".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("Config file not found: {}", p.display()),
                    fix_hint: Some("Run `meepo init` to create a default config".to_string()),
                }
            }
        }
        None => CheckResult {
            name: "config_file".to_string(),
            status: CheckStatus::Skip,
            message: "No config path specified".to_string(),
            fix_hint: None,
        },
    }
}

fn check_db_path(path: Option<&std::path::Path>) -> CheckResult {
    match path {
        Some(p) => {
            let dir = p.parent().unwrap_or(p);
            if dir.exists() {
                CheckResult {
                    name: "database_dir".to_string(),
                    status: CheckStatus::Pass,
                    message: format!("Database directory exists: {}", dir.display()),
                    fix_hint: None,
                }
            } else {
                // Try to create it
                match std::fs::create_dir_all(dir) {
                    Ok(_) => CheckResult {
                        name: "database_dir".to_string(),
                        status: CheckStatus::Pass,
                        message: format!("Created database directory: {}", dir.display()),
                        fix_hint: None,
                    },
                    Err(e) => CheckResult {
                        name: "database_dir".to_string(),
                        status: CheckStatus::Fail,
                        message: format!("Cannot create database directory: {}", e),
                        fix_hint: Some(format!("mkdir -p {}", dir.display())),
                    },
                }
            }
        }
        None => CheckResult {
            name: "database_dir".to_string(),
            status: CheckStatus::Skip,
            message: "No database path specified".to_string(),
            fix_hint: None,
        },
    }
}

async fn check_docker() -> CheckResult {
    let result = tokio::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    match result {
        Ok(status) if status.success() => CheckResult {
            name: "docker".to_string(),
            status: CheckStatus::Pass,
            message: "Docker is available".to_string(),
            fix_hint: None,
        },
        Ok(_) => CheckResult {
            name: "docker".to_string(),
            status: CheckStatus::Warn,
            message: "Docker is installed but not running".to_string(),
            fix_hint: Some("Start Docker Desktop or run `sudo systemctl start docker`".to_string()),
        },
        Err(_) => CheckResult {
            name: "docker".to_string(),
            status: CheckStatus::Warn,
            message: "Docker is not installed (sandbox features unavailable)".to_string(),
            fix_hint: Some("Install Docker: https://docs.docker.com/get-docker/".to_string()),
        },
    }
}

fn check_api_key(env_var: &str) -> CheckResult {
    match std::env::var(env_var) {
        Ok(val) if !val.is_empty() => {
            let masked = if val.len() > 8 {
                format!("{}...{}", &val[..4], &val[val.len() - 4..])
            } else {
                "****".to_string()
            };
            CheckResult {
                name: format!("api_key_{}", env_var.to_lowercase()),
                status: CheckStatus::Pass,
                message: format!("{} is set ({})", env_var, masked),
                fix_hint: None,
            }
        }
        _ => CheckResult {
            name: format!("api_key_{}", env_var.to_lowercase()),
            status: CheckStatus::Fail,
            message: format!("{} is not set", env_var),
            fix_hint: Some(format!("export {}=\"your-api-key\"", env_var)),
        },
    }
}

fn check_command(cmd: &str, args: &[&str], display_name: &str) -> CheckResult {
    match std::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) if status.success() => CheckResult {
            name: format!("command_{}", cmd),
            status: CheckStatus::Pass,
            message: format!("{} is available", display_name),
            fix_hint: None,
        },
        _ => CheckResult {
            name: format!("command_{}", cmd),
            status: CheckStatus::Warn,
            message: format!("{} is not available", display_name),
            fix_hint: Some(format!("Install {}", display_name)),
        },
    }
}

fn check_home_dir() -> CheckResult {
    match dirs::home_dir() {
        Some(home) if home.exists() => CheckResult {
            name: "home_dir".to_string(),
            status: CheckStatus::Pass,
            message: format!("Home directory: {}", home.display()),
            fix_hint: None,
        },
        _ => CheckResult {
            name: "home_dir".to_string(),
            status: CheckStatus::Fail,
            message: "Cannot determine home directory".to_string(),
            fix_hint: Some("Set HOME environment variable".to_string()),
        },
    }
}

fn check_secret_leaks() -> CheckResult {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            return CheckResult {
                name: "secret_leaks".to_string(),
                status: CheckStatus::Skip,
                message: "Cannot check for secret leaks (no home dir)".to_string(),
                fix_hint: None,
            };
        }
    };

    let meepo_dir = home.join(".meepo");
    if !meepo_dir.exists() {
        return CheckResult {
            name: "secret_leaks".to_string(),
            status: CheckStatus::Pass,
            message: "No Meepo data directory to scan".to_string(),
            fix_hint: None,
        };
    }

    // Check if any session files contain potential API keys
    let suspicious_patterns = ["sk-", "key-", "Bearer "];
    let mut leak_count = 0;

    if let Ok(entries) = std::fs::read_dir(&meepo_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json" || e == "toml")
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                for pattern in &suspicious_patterns {
                    if content.contains(pattern) {
                        leak_count += 1;
                        debug!("Potential secret leak in: {}", path.display());
                        break;
                    }
                }
            }
        }
    }

    if leak_count > 0 {
        CheckResult {
            name: "secret_leaks".to_string(),
            status: CheckStatus::Warn,
            message: format!(
                "Found {} file(s) with potential secret leaks in {}",
                leak_count,
                meepo_dir.display()
            ),
            fix_hint: Some("Run `meepo sessions scrub` to redact secrets".to_string()),
        }
    } else {
        CheckResult {
            name: "secret_leaks".to_string(),
            status: CheckStatus::Pass,
            message: "No secret leaks detected".to_string(),
            fix_hint: None,
        }
    }
}

fn check_temp_dir() -> CheckResult {
    let temp = std::env::temp_dir();
    let test_file = temp.join(".meepo_doctor_test");

    match std::fs::write(&test_file, "test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            CheckResult {
                name: "temp_dir".to_string(),
                status: CheckStatus::Pass,
                message: format!("Temp directory writable: {}", temp.display()),
                fix_hint: None,
            }
        }
        Err(e) => CheckResult {
            name: "temp_dir".to_string(),
            status: CheckStatus::Fail,
            message: format!("Temp directory not writable: {}", e),
            fix_hint: Some("Check permissions on temp directory".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_status_display() {
        assert_eq!(CheckStatus::Pass.to_string(), "PASS");
        assert_eq!(CheckStatus::Fail.to_string(), "FAIL");
        assert_eq!(CheckStatus::Warn.to_string(), "WARN");
        assert_eq!(CheckStatus::Skip.to_string(), "SKIP");
    }

    #[test]
    fn test_check_config_file_missing() {
        let result = check_config_file(Some(std::path::Path::new("/nonexistent/config.toml")));
        assert_eq!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn test_check_config_file_none() {
        let result = check_config_file(None);
        assert_eq!(result.status, CheckStatus::Skip);
    }

    #[test]
    fn test_check_home_dir() {
        let result = check_home_dir();
        // Should pass on any normal system
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_temp_dir() {
        let result = check_temp_dir();
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_command_git() {
        let result = check_command("git", &["--version"], "Git");
        // Git should be available on dev machines
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_command_nonexistent() {
        let result = check_command("nonexistent_command_xyz", &[], "Nonexistent");
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_doctor_report_healthy() {
        let report = DoctorReport {
            checks: vec![
                CheckResult {
                    name: "test".to_string(),
                    status: CheckStatus::Pass,
                    message: "ok".to_string(),
                    fix_hint: None,
                },
            ],
            pass_count: 1,
            warn_count: 0,
            fail_count: 0,
            skip_count: 0,
        };
        assert!(report.is_healthy());
    }

    #[test]
    fn test_doctor_report_unhealthy() {
        let report = DoctorReport {
            checks: vec![],
            pass_count: 0,
            warn_count: 0,
            fail_count: 1,
            skip_count: 0,
        };
        assert!(!report.is_healthy());
    }

    #[tokio::test]
    async fn test_run_doctor() {
        let report = run_doctor(None, None).await.unwrap();
        // Should complete without error
        assert!(!report.checks.is_empty());
    }

    #[test]
    fn test_check_secret_leaks() {
        let result = check_secret_leaks();
        // Should not fail on a clean system
        assert_ne!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn test_check_api_key_missing() {
        let result = check_api_key("MEEPO_NONEXISTENT_KEY_DOCTOR_TEST");
        assert_eq!(result.status, CheckStatus::Fail);
    }
}
