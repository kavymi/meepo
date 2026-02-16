//! MEMORY.md and SOUL.md synchronization

use anyhow::{Context, Result};
use std::path::Path;
use tracing::{debug, info, warn};

/// Load MEMORY.md contents
pub fn load_memory<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    debug!("Loading memory from {:?}", path);

    if !path.exists() {
        warn!(
            "Memory file does not exist at {:?}, returning empty string",
            path
        );
        return Ok(String::new());
    }

    let content = std::fs::read_to_string(path)
        .context(format!("Failed to read memory file at {:?}", path))?;

    info!("Loaded {} bytes from memory file", content.len());
    Ok(content)
}

/// Save MEMORY.md contents
pub fn save_memory<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    let path = path.as_ref();
    debug!("Saving memory to {:?}", path);

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context(format!("Failed to create directory {:?}", parent))?;
    }

    std::fs::write(path, content).context(format!("Failed to write memory file at {:?}", path))?;

    info!("Saved {} bytes to memory file", content.len());
    Ok(())
}

/// Load SOUL.md contents (meepo's core identity and purpose)
pub fn load_soul<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    debug!("Loading soul from {:?}", path);

    if !path.exists() {
        warn!(
            "Soul file does not exist at {:?}, returning empty string",
            path
        );
        return Ok(String::new());
    }

    let content =
        std::fs::read_to_string(path).context(format!("Failed to read soul file at {:?}", path))?;

    info!("Loaded {} bytes from soul file", content.len());
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_save_and_load_memory() -> Result<()> {
        let temp_path = env::temp_dir().join("test_memory.md");
        let _ = std::fs::remove_file(&temp_path);

        let content = "# Test Memory\n\nSome content here";
        save_memory(&temp_path, content)?;

        let loaded = load_memory(&temp_path)?;
        assert_eq!(loaded, content);

        let _ = std::fs::remove_file(&temp_path);
        Ok(())
    }

    #[test]
    fn test_load_nonexistent() -> Result<()> {
        let temp_path = env::temp_dir().join("nonexistent_memory.md");
        let _ = std::fs::remove_file(&temp_path);

        let content = load_memory(&temp_path)?;
        assert_eq!(content, "");

        Ok(())
    }

    #[test]
    fn test_load_soul() -> Result<()> {
        let temp_path = env::temp_dir().join("test_soul.md");
        let _ = std::fs::remove_file(&temp_path);

        let soul_content = "# SOUL\n\nI am a helpful meepo.";
        save_memory(&temp_path, soul_content)?;

        let loaded = load_soul(&temp_path)?;
        assert_eq!(loaded, soul_content);

        let _ = std::fs::remove_file(&temp_path);
        Ok(())
    }

    #[test]
    fn test_load_soul_nonexistent() -> Result<()> {
        let temp_path = env::temp_dir().join("nonexistent_soul_xyz.md");
        let _ = std::fs::remove_file(&temp_path);

        let content = load_soul(&temp_path)?;
        assert_eq!(content, "");
        Ok(())
    }

    #[test]
    fn test_save_creates_parent_dirs() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let nested_path = temp.path().join("a").join("b").join("c").join("memory.md");

        save_memory(&nested_path, "nested content")?;
        let loaded = load_memory(&nested_path)?;
        assert_eq!(loaded, "nested content");
        Ok(())
    }

    #[test]
    fn test_save_overwrite() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let path = temp.path().join("overwrite.md");

        save_memory(&path, "first")?;
        save_memory(&path, "second")?;
        let loaded = load_memory(&path)?;
        assert_eq!(loaded, "second");
        Ok(())
    }

    #[test]
    fn test_save_empty_content() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let path = temp.path().join("empty.md");

        save_memory(&path, "")?;
        let loaded = load_memory(&path)?;
        assert_eq!(loaded, "");
        Ok(())
    }

    #[test]
    fn test_save_unicode_content() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let path = temp.path().join("unicode.md");

        let content = "# 日本語テスト\n\n🎉 Emoji and ñ special chars";
        save_memory(&path, content)?;
        let loaded = load_memory(&path)?;
        assert_eq!(loaded, content);
        Ok(())
    }
}
