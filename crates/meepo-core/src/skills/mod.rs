//! Skill system — import OpenClaw-compatible SKILL.md files as tools
//!
//! Skills are markdown files with YAML frontmatter that define:
//! - Name and description
//! - Input parameters
//! - Allowed commands
//! - Agent instructions

pub mod parser;
pub mod skill_tool;

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

use crate::tools::ToolHandler;
pub use parser::{SkillDefinition, SkillInput};
pub use skill_tool::SkillToolHandler;

/// Load all skills from a directory
///
/// Expects structure: `dir/skill_name/SKILL.md`
/// Each subdirectory containing a SKILL.md file becomes a tool.
pub fn load_skills(dir: &Path) -> Result<Vec<Arc<dyn ToolHandler>>> {
    let mut tools: Vec<Arc<dyn ToolHandler>> = Vec::new();

    if !dir.exists() {
        info!(
            "Skills directory does not exist: {} — skipping",
            dir.display()
        );
        return Ok(tools);
    }

    let entries = std::fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let skill_file = path.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }

        match load_single_skill(&skill_file) {
            Ok(tool) => {
                info!(
                    "Loaded skill: {} from {}",
                    tool.name(),
                    skill_file.display()
                );
                tools.push(tool);
            }
            Err(e) => {
                warn!("Failed to load skill from {}: {}", skill_file.display(), e);
            }
        }
    }

    info!("Loaded {} skills from {}", tools.len(), dir.display());
    Ok(tools)
}

/// Load a single SKILL.md file
fn load_single_skill(path: &Path) -> Result<Arc<dyn ToolHandler>> {
    let content = std::fs::read_to_string(path)?;
    let definition = parser::parse_skill(&content)?;
    Ok(Arc::new(SkillToolHandler::new(definition)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_skills_nonexistent_dir() {
        let result = load_skills(Path::new("/nonexistent/dir"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_load_skills_from_temp_dir() {
        let dir = tempfile::tempdir().unwrap();

        // Create a skill
        let skill_dir = dir.path().join("my_skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my_skill\ndescription: A test skill\n---\nDo the thing.\n",
        )
        .unwrap();

        // Create a non-skill directory (no SKILL.md)
        let other_dir = dir.path().join("not_a_skill");
        fs::create_dir(&other_dir).unwrap();
        fs::write(other_dir.join("README.md"), "not a skill").unwrap();

        let tools = load_skills(dir.path()).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "my_skill");
    }

    #[test]
    fn test_load_skills_invalid_skill_skipped() {
        let dir = tempfile::tempdir().unwrap();

        // Create a valid skill
        let good = dir.path().join("good");
        fs::create_dir(&good).unwrap();
        fs::write(
            good.join("SKILL.md"),
            "---\nname: good_skill\ndescription: Works\n---\nDo it.\n",
        )
        .unwrap();

        // Create an invalid skill
        let bad = dir.path().join("bad");
        fs::create_dir(&bad).unwrap();
        fs::write(bad.join("SKILL.md"), "no frontmatter").unwrap();

        let tools = load_skills(dir.path()).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "good_skill");
    }
}
