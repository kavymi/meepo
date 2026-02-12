//! Filesystem access tools for browsing and searching local directories

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::debug;

use super::{ToolHandler, json_schema};

/// Validate that a path is within one of the allowed directories.
/// Uses canonicalize() to resolve symlinks and ".." — the canonical path
/// must start with one of the pre-canonicalized allowed directories.
fn validate_allowed_path(path: &str, allowed_dirs: &[PathBuf]) -> Result<PathBuf> {
    let expanded = shellexpand(path);
    let canonical = expanded
        .canonicalize()
        .with_context(|| format!("Path does not exist: {}", expanded.display()))?;

    for allowed in allowed_dirs {
        if canonical.starts_with(allowed) {
            return Ok(canonical);
        }
    }

    Err(anyhow::anyhow!(
        "Access denied: '{}' is not within allowed directories",
        canonical.display()
    ))
}

fn shellexpand(s: &str) -> PathBuf {
    let mut result = s.to_string();
    if result.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        result = format!("{}{}", home.display(), &result[1..]);
    }
    PathBuf::from(result)
}

/// List directory contents
pub struct ListDirectoryTool {
    allowed_dirs: Vec<PathBuf>,
}

impl ListDirectoryTool {
    pub fn new(allowed_dirs: Vec<String>) -> Self {
        Self {
            allowed_dirs: allowed_dirs
                .iter()
                .map(|d| {
                    let expanded = shellexpand(d);
                    expanded.canonicalize().unwrap_or(expanded)
                })
                .collect(),
        }
    }
}

#[async_trait]
impl ToolHandler for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories at a given path. Only accessible within configured allowed directories."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Directory path to list (supports ~/)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List recursively (default: false, max depth: 3)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Optional glob pattern to filter files (e.g. '*.rs', '*.py')"
                }
            }),
            vec!["path"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path_str = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let recursive = input
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let pattern = input.get("pattern").and_then(|v| v.as_str());

        let validated_path = validate_allowed_path(path_str, &self.allowed_dirs)?;
        debug!("Listing directory: {}", validated_path.display());

        let mut entries = Vec::new();
        list_dir_recursive(
            &validated_path,
            &validated_path,
            recursive,
            0,
            3,
            pattern,
            &mut entries,
        )?;

        if entries.is_empty() {
            return Ok("Directory is empty or no files match the pattern.".to_string());
        }

        Ok(entries.join("\n"))
    }
}

fn list_dir_recursive(
    base: &Path,
    dir: &Path,
    recursive: bool,
    depth: usize,
    max_depth: usize,
    pattern: Option<&str>,
    entries: &mut Vec<String>,
) -> Result<()> {
    let mut dir_entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    dir_entries.sort_by_key(|e| e.file_name());

    for entry in dir_entries {
        let path = entry.path();
        let name = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .display()
            .to_string();

        // Skip hidden files
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }

        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            entries.push(format!("{}/ (dir)", name));
            if recursive && depth < max_depth {
                list_dir_recursive(
                    base,
                    &path,
                    recursive,
                    depth + 1,
                    max_depth,
                    pattern,
                    entries,
                )?;
            }
        } else {
            // Check pattern if provided
            if let Some(pat) = pattern {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if !glob::Pattern::new(pat)
                    .map(|p| p.matches(&file_name))
                    .unwrap_or(false)
                {
                    continue;
                }
            }

            let size = metadata.len();
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            let size_str = if size < 1024 {
                format!("{} B", size)
            } else if size < 1024 * 1024 {
                format!("{:.1} KB", size as f64 / 1024.0)
            } else {
                format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
            };

            entries.push(format!("{} ({}, {})", name, size_str, modified));
        }
    }
    Ok(())
}

/// Search file contents within a directory
pub struct SearchFilesTool {
    allowed_dirs: Vec<PathBuf>,
}

impl SearchFilesTool {
    pub fn new(allowed_dirs: Vec<String>) -> Self {
        Self {
            allowed_dirs: allowed_dirs
                .iter()
                .map(|d| {
                    let expanded = shellexpand(d);
                    expanded.canonicalize().unwrap_or(expanded)
                })
                .collect(),
        }
    }
}

#[async_trait]
impl ToolHandler for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for text patterns within files in a directory. Returns matching lines with file paths, line numbers, and context."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "path": {
                    "type": "string",
                    "description": "Directory to search in (supports ~/)"
                },
                "query": {
                    "type": "string",
                    "description": "Text or pattern to search for (case-insensitive)"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Optional glob pattern to filter files (e.g. '*.rs', '*.py')"
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of matching lines to return (default: 20)"
                }
            }),
            vec!["path", "query"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let path_str = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let file_pattern = input.get("file_pattern").and_then(|v| v.as_str());
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(100) as usize;

        let validated_path = validate_allowed_path(path_str, &self.allowed_dirs)?;
        debug!(
            "Searching files in {} for '{}'",
            validated_path.display(),
            query
        );

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        let mut files_scanned = 0usize;
        let max_files = 1000;

        search_dir_recursive(
            &validated_path,
            &validated_path,
            &query_lower,
            file_pattern,
            max_results,
            max_files,
            &mut files_scanned,
            &mut results,
        )?;

        if results.is_empty() {
            return Ok(format!(
                "No matches found for '{}' in {} ({} files scanned)",
                query,
                validated_path.display(),
                files_scanned
            ));
        }

        let truncated = results.len() >= max_results || files_scanned >= max_files;
        let header = format!(
            "Found {} matches in {} ({} files scanned){}:\n",
            results.len(),
            validated_path.display(),
            files_scanned,
            if truncated {
                " [results truncated]"
            } else {
                ""
            }
        );
        Ok(format!("{}{}", header, results.join("\n")))
    }
}

#[allow(clippy::too_many_arguments)]
fn search_dir_recursive(
    base: &Path,
    dir: &Path,
    query: &str,
    file_pattern: Option<&str>,
    max_results: usize,
    max_files: usize,
    files_scanned: &mut usize,
    results: &mut Vec<String>,
) -> Result<()> {
    if results.len() >= max_results || *files_scanned >= max_files {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if results.len() >= max_results || *files_scanned >= max_files {
            break;
        }

        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/dirs
        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Skip common large directories
            if matches!(
                name.as_str(),
                "node_modules"
                    | "target"
                    | ".git"
                    | "build"
                    | "dist"
                    | "__pycache__"
                    | ".venv"
                    | "venv"
            ) {
                continue;
            }
            search_dir_recursive(
                base,
                &path,
                query,
                file_pattern,
                max_results,
                max_files,
                files_scanned,
                results,
            )?;
        } else {
            // Check file pattern
            if let Some(pat) = file_pattern
                && !glob::Pattern::new(pat)
                    .map(|p| p.matches(&name))
                    .unwrap_or(false)
            {
                continue;
            }

            // Skip large files (> 10MB) to prevent OOM
            if let Ok(meta) = entry.metadata()
                && meta.len() > 10 * 1024 * 1024
            {
                continue;
            }

            // Skip binary files (check first 512 bytes)
            if let Ok(content) = std::fs::read(&path) {
                *files_scanned += 1;
                let check_len = content.len().min(512);
                if content[..check_len].contains(&0) {
                    continue; // Skip binary
                }

                if let Ok(text) = String::from_utf8(content) {
                    let rel_path = path
                        .strip_prefix(base)
                        .unwrap_or(&path)
                        .display()
                        .to_string();

                    for (line_num, line) in text.lines().enumerate() {
                        if results.len() >= max_results {
                            break;
                        }
                        if line.to_lowercase().contains(query) {
                            results.push(format!(
                                "{}:{}: {}",
                                rel_path,
                                line_num + 1,
                                line.chars().take(200).collect::<String>()
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_directory_tool_schema() {
        let tool = ListDirectoryTool::new(vec!["~/Coding".to_string()]);
        assert_eq!(tool.name(), "list_directory");
        assert!(!tool.description().is_empty());
        let schema = tool.input_schema();
        assert!(schema.get("properties").is_some());
    }

    #[tokio::test]
    async fn test_list_directory_allowed() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("world.txt"), "hello world").unwrap();
        std::fs::create_dir(temp.path().join("subdir")).unwrap();

        let tool = ListDirectoryTool::new(vec![temp_path.clone()]);
        let result = tool
            .execute(serde_json::json!({
                "path": temp_path
            }))
            .await
            .unwrap();

        assert!(result.contains("hello.rs"));
        assert!(result.contains("world.txt"));
        assert!(result.contains("subdir/"));
    }

    #[tokio::test]
    async fn test_list_directory_pattern_filter() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("world.txt"), "hello world").unwrap();

        let tool = ListDirectoryTool::new(vec![temp_path.clone()]);
        let result = tool
            .execute(serde_json::json!({
                "path": temp_path,
                "pattern": "*.rs"
            }))
            .await
            .unwrap();

        assert!(result.contains("hello.rs"));
        assert!(!result.contains("world.txt"));
    }

    #[tokio::test]
    async fn test_list_directory_denied() {
        let tool = ListDirectoryTool::new(vec!["~/Coding".to_string()]);
        let result = tool
            .execute(serde_json::json!({
                "path": "/etc"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_directory_path_traversal_blocked() {
        let tool = ListDirectoryTool::new(vec!["~/Coding".to_string()]);
        let result = tool
            .execute(serde_json::json!({
                "path": "~/Coding/../../etc"
            }))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_search_files_tool_schema() {
        let tool = SearchFilesTool::new(vec!["~/Coding".to_string()]);
        assert_eq!(tool.name(), "search_files");
    }

    #[tokio::test]
    async fn test_search_files_found() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(
            temp.path().join("hello.rs"),
            "fn main() {\n    println!(\"hello world\");\n}\n",
        )
        .unwrap();
        std::fs::write(temp.path().join("other.txt"), "nothing here").unwrap();

        let tool = SearchFilesTool::new(vec![temp_path.clone()]);
        let result = tool
            .execute(serde_json::json!({
                "path": temp_path,
                "query": "println"
            }))
            .await
            .unwrap();

        assert!(result.contains("hello.rs:2"));
        assert!(result.contains("println"));
    }

    #[tokio::test]
    async fn test_search_files_pattern_filter() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("hello.py"), "def main(): pass").unwrap();

        let tool = SearchFilesTool::new(vec![temp_path.clone()]);
        let result = tool
            .execute(serde_json::json!({
                "path": temp_path,
                "query": "main",
                "file_pattern": "*.rs"
            }))
            .await
            .unwrap();

        assert!(result.contains("hello.rs"));
        assert!(!result.contains("hello.py"));
    }

    #[tokio::test]
    async fn test_search_files_no_match() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().to_str().unwrap().to_string();

        std::fs::write(temp.path().join("hello.rs"), "fn main() {}").unwrap();

        let tool = SearchFilesTool::new(vec![temp_path.clone()]);
        let result = tool
            .execute(serde_json::json!({
                "path": temp_path,
                "query": "nonexistent_xyz_pattern"
            }))
            .await
            .unwrap();

        assert!(result.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_search_files_denied() {
        let tool = SearchFilesTool::new(vec!["~/Coding".to_string()]);
        let result = tool
            .execute(serde_json::json!({
                "path": "/etc",
                "query": "test"
            }))
            .await;
        assert!(result.is_err());
    }
}
