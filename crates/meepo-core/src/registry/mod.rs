//! Skills Registry — discover, install, and manage skills/plugins
//!
//! MeepoHub-compatible registry for sharing and discovering skills.
//! Skills are YAML-defined tool bundles that extend Meepo's capabilities.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// A skill package in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackage {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
}

/// Status of an installed skill
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillStatus {
    Available,
    Installed,
    UpdateAvailable,
    Disabled,
}

impl std::fmt::Display for SkillStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillStatus::Available => write!(f, "available"),
            SkillStatus::Installed => write!(f, "installed"),
            SkillStatus::UpdateAvailable => write!(f, "update_available"),
            SkillStatus::Disabled => write!(f, "disabled"),
        }
    }
}

/// An installed skill with local metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub package: SkillPackage,
    pub status: SkillStatus,
    pub installed_at: chrono::DateTime<chrono::Utc>,
    pub skill_dir: PathBuf,
}

/// Local skills registry — manages installed skills
pub struct SkillsRegistry {
    skills_dir: PathBuf,
    installed: HashMap<String, InstalledSkill>,
}

impl SkillsRegistry {
    /// Create a new registry with the given skills directory
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills_dir,
            installed: HashMap::new(),
        }
    }

    /// Load installed skills from the skills directory
    pub fn load(&mut self) -> Result<()> {
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir)
                .context("Failed to create skills directory")?;
            return Ok(());
        }

        let manifest_path = self.skills_dir.join("registry.json");
        if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path)
                .context("Failed to read registry manifest")?;
            self.installed = serde_json::from_str(&content)
                .context("Failed to parse registry manifest")?;
            info!("Loaded {} installed skills", self.installed.len());
        }

        Ok(())
    }

    /// Save the registry manifest
    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(&self.skills_dir)
            .context("Failed to create skills directory")?;

        let manifest_path = self.skills_dir.join("registry.json");
        let content = serde_json::to_string_pretty(&self.installed)
            .context("Failed to serialize registry")?;
        std::fs::write(&manifest_path, content)
            .context("Failed to write registry manifest")?;

        debug!("Saved registry with {} skills", self.installed.len());
        Ok(())
    }

    /// Install a skill package
    pub fn install(&mut self, package: SkillPackage) -> Result<()> {
        // Validate package name
        if package.name.is_empty() || package.name.len() > 128 {
            return Err(anyhow!("Invalid package name"));
        }
        if package.name.contains('/') || package.name.contains('\\') || package.name.contains("..") {
            return Err(anyhow!("Package name contains invalid characters"));
        }

        let skill_dir = self.skills_dir.join(&package.name);
        std::fs::create_dir_all(&skill_dir)
            .context("Failed to create skill directory")?;

        let installed = InstalledSkill {
            package: package.clone(),
            status: SkillStatus::Installed,
            installed_at: chrono::Utc::now(),
            skill_dir,
        };

        info!("Installed skill: {} v{}", package.name, package.version);
        self.installed.insert(package.name.clone(), installed);
        self.save()?;

        Ok(())
    }

    /// Uninstall a skill
    pub fn uninstall(&mut self, name: &str) -> Result<()> {
        let skill = self.installed.remove(name)
            .ok_or_else(|| anyhow!("Skill '{}' is not installed", name))?;

        // Remove skill directory
        if skill.skill_dir.exists() {
            std::fs::remove_dir_all(&skill.skill_dir)
                .context("Failed to remove skill directory")?;
        }

        info!("Uninstalled skill: {}", name);
        self.save()?;
        Ok(())
    }

    /// Disable a skill without uninstalling
    pub fn disable(&mut self, name: &str) -> Result<()> {
        let skill = self.installed.get_mut(name)
            .ok_or_else(|| anyhow!("Skill '{}' is not installed", name))?;
        skill.status = SkillStatus::Disabled;
        warn!("Disabled skill: {}", name);
        self.save()?;
        Ok(())
    }

    /// Enable a disabled skill
    pub fn enable(&mut self, name: &str) -> Result<()> {
        let skill = self.installed.get_mut(name)
            .ok_or_else(|| anyhow!("Skill '{}' is not installed", name))?;
        skill.status = SkillStatus::Installed;
        info!("Enabled skill: {}", name);
        self.save()?;
        Ok(())
    }

    /// List all installed skills
    pub fn list(&self) -> Vec<&InstalledSkill> {
        self.installed.values().collect()
    }

    /// List active (non-disabled) skills
    pub fn list_active(&self) -> Vec<&InstalledSkill> {
        self.installed
            .values()
            .filter(|s| s.status != SkillStatus::Disabled)
            .collect()
    }

    /// Get an installed skill by name
    pub fn get(&self, name: &str) -> Option<&InstalledSkill> {
        self.installed.get(name)
    }

    /// Number of installed skills
    pub fn count(&self) -> usize {
        self.installed.len()
    }

    /// Search installed skills by tag
    pub fn search_by_tag(&self, tag: &str) -> Vec<&InstalledSkill> {
        self.installed
            .values()
            .filter(|s| s.package.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_package(name: &str) -> SkillPackage {
        SkillPackage {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("Test skill: {}", name),
            author: "test".to_string(),
            tags: vec!["test".to_string()],
            dependencies: Vec::new(),
            tools: vec!["tool_a".to_string()],
            source_url: None,
            checksum: None,
        }
    }

    #[test]
    fn test_registry_new() {
        let dir = TempDir::new().unwrap();
        let registry = SkillsRegistry::new(dir.path().to_path_buf());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_install_and_list() {
        let dir = TempDir::new().unwrap();
        let mut registry = SkillsRegistry::new(dir.path().to_path_buf());

        registry.install(test_package("weather")).unwrap();
        registry.install(test_package("calendar")).unwrap();

        assert_eq!(registry.count(), 2);
        assert!(registry.get("weather").is_some());
        assert!(registry.get("calendar").is_some());
    }

    #[test]
    fn test_uninstall() {
        let dir = TempDir::new().unwrap();
        let mut registry = SkillsRegistry::new(dir.path().to_path_buf());

        registry.install(test_package("weather")).unwrap();
        assert_eq!(registry.count(), 1);

        registry.uninstall("weather").unwrap();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_uninstall_nonexistent() {
        let dir = TempDir::new().unwrap();
        let mut registry = SkillsRegistry::new(dir.path().to_path_buf());
        assert!(registry.uninstall("nonexistent").is_err());
    }

    #[test]
    fn test_disable_enable() {
        let dir = TempDir::new().unwrap();
        let mut registry = SkillsRegistry::new(dir.path().to_path_buf());

        registry.install(test_package("weather")).unwrap();
        assert_eq!(registry.list_active().len(), 1);

        registry.disable("weather").unwrap();
        assert_eq!(registry.list_active().len(), 0);
        assert_eq!(registry.get("weather").unwrap().status, SkillStatus::Disabled);

        registry.enable("weather").unwrap();
        assert_eq!(registry.list_active().len(), 1);
    }

    #[test]
    fn test_search_by_tag() {
        let dir = TempDir::new().unwrap();
        let mut registry = SkillsRegistry::new(dir.path().to_path_buf());

        let mut pkg = test_package("weather");
        pkg.tags = vec!["utility".to_string(), "api".to_string()];
        registry.install(pkg).unwrap();

        let mut pkg2 = test_package("calendar");
        pkg2.tags = vec!["productivity".to_string()];
        registry.install(pkg2).unwrap();

        assert_eq!(registry.search_by_tag("utility").len(), 1);
        assert_eq!(registry.search_by_tag("productivity").len(), 1);
        assert_eq!(registry.search_by_tag("nonexistent").len(), 0);
    }

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();

        // Install and save
        {
            let mut registry = SkillsRegistry::new(dir.path().to_path_buf());
            registry.install(test_package("weather")).unwrap();
            registry.install(test_package("calendar")).unwrap();
        }

        // Load in new instance
        {
            let mut registry = SkillsRegistry::new(dir.path().to_path_buf());
            registry.load().unwrap();
            assert_eq!(registry.count(), 2);
            assert!(registry.get("weather").is_some());
        }
    }

    #[test]
    fn test_install_invalid_name() {
        let dir = TempDir::new().unwrap();
        let mut registry = SkillsRegistry::new(dir.path().to_path_buf());

        let mut pkg = test_package("test");
        pkg.name = "../etc/passwd".to_string();
        assert!(registry.install(pkg).is_err());

        let mut pkg2 = test_package("test");
        pkg2.name = String::new();
        assert!(registry.install(pkg2).is_err());
    }

    #[test]
    fn test_skill_status_display() {
        assert_eq!(SkillStatus::Available.to_string(), "available");
        assert_eq!(SkillStatus::Installed.to_string(), "installed");
        assert_eq!(SkillStatus::Disabled.to_string(), "disabled");
    }
}
