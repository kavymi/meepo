//! Agent template system — parse, resolve, activate, and reset templates.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Metadata section from template.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// A goal defined in template.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateGoal {
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: i64,
    pub success_criteria: Option<String>,
}

fn default_priority() -> i32 {
    3
}
fn default_check_interval() -> i64 {
    1800
}

/// Parsed template.toml — metadata + goals + raw TOML overlay
#[derive(Debug, Clone)]
pub struct Template {
    pub metadata: TemplateMetadata,
    pub goals: Vec<TemplateGoal>,
    /// The raw TOML table for config overlay (everything except [template] and [[goals]])
    pub config_overlay: toml::Value,
    /// Directory the template was loaded from (synthetic for built-in)
    pub dir: PathBuf,
}

/// Active template state stored in .active-template
#[derive(Debug, Serialize, Deserialize)]
pub struct ActiveTemplate {
    pub name: String,
    pub source: String,
    pub activated_at: String,
}

// ── Built-in templates ──────────────────────────────────────────

struct BuiltInTemplate {
    name: &'static str,
    template_toml: &'static str,
    soul_md: &'static str,
}

const BUILT_IN_TEMPLATES: &[BuiltInTemplate] = &[
    BuiltInTemplate {
        name: "stock-analyst",
        template_toml: include_str!("../templates/stock-analyst/template.toml"),
        soul_md: include_str!("../templates/stock-analyst/SOUL.md"),
    },
    BuiltInTemplate {
        name: "code-reviewer",
        template_toml: include_str!("../templates/code-reviewer/template.toml"),
        soul_md: include_str!("../templates/code-reviewer/SOUL.md"),
    },
    BuiltInTemplate {
        name: "personal-assistant",
        template_toml: include_str!("../templates/personal-assistant/template.toml"),
        soul_md: include_str!("../templates/personal-assistant/SOUL.md"),
    },
    BuiltInTemplate {
        name: "research-agent",
        template_toml: include_str!("../templates/research-agent/template.toml"),
        soul_md: include_str!("../templates/research-agent/SOUL.md"),
    },
];

// ── Parsing ─────────────────────────────────────────────────────

impl Template {
    /// Parse a template.toml string into a Template
    pub fn parse(content: &str, dir: PathBuf) -> Result<Self> {
        let raw: toml::Value = toml::from_str(content).context("Failed to parse template.toml")?;

        let table = raw
            .as_table()
            .context("template.toml must be a TOML table")?;

        // Extract [template] section
        let template_section = table
            .get("template")
            .context("template.toml must have a [template] section")?;
        let metadata: TemplateMetadata = template_section
            .clone()
            .try_into()
            .context("Invalid [template] section")?;

        // Extract [[goals]] array
        let goals: Vec<TemplateGoal> = if let Some(goals_val) = table.get("goals") {
            goals_val
                .clone()
                .try_into()
                .context("Invalid [[goals]] array")?
        } else {
            vec![]
        };

        // Everything else is config overlay
        let mut overlay = toml::map::Map::new();
        for (key, value) in table {
            if key != "template" && key != "goals" {
                overlay.insert(key.clone(), value.clone());
            }
        }

        Ok(Template {
            metadata,
            goals,
            config_overlay: toml::Value::Table(overlay),
            dir,
        })
    }
}

// ── Resolution ──────────────────────────────────────────────────

/// Resolve a template name/path to a Template
pub fn resolve_template(name_or_path: &str) -> Result<Template> {
    // 1. Check if it's a local path
    let path = PathBuf::from(name_or_path);
    if path.exists() && path.join("template.toml").exists() {
        let content = std::fs::read_to_string(path.join("template.toml"))
            .context("Failed to read template.toml")?;
        return Template::parse(&content, path);
    }

    // 2. Check built-in templates
    if let Some(built_in) = BUILT_IN_TEMPLATES.iter().find(|t| t.name == name_or_path) {
        let dir = crate::config::config_dir()
            .join("templates")
            .join(built_in.name);
        return Template::parse(built_in.template_toml, dir);
    }

    // 3. Check ~/.meepo/templates/<name>/
    let local_dir = crate::config::config_dir()
        .join("templates")
        .join(name_or_path);
    if local_dir.join("template.toml").exists() {
        let content = std::fs::read_to_string(local_dir.join("template.toml"))
            .context("Failed to read template.toml")?;
        return Template::parse(&content, local_dir);
    }

    // 4. GitHub — not yet implemented
    if name_or_path.starts_with("gh:") {
        bail!(
            "GitHub template fetching not yet implemented. Download the template locally and use the path instead."
        );
    }

    bail!(
        "Template '{}' not found.\n\nAvailable built-in templates: {}\nOr provide a path to a template directory.",
        name_or_path,
        BUILT_IN_TEMPLATES
            .iter()
            .map(|t| t.name)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// List all available templates (built-in + local)
pub fn list_templates() -> Vec<(String, String, String)> {
    let mut templates = Vec::new();

    // Built-in
    for built_in in BUILT_IN_TEMPLATES {
        if let Ok(t) = Template::parse(built_in.template_toml, PathBuf::new()) {
            templates.push((
                t.metadata.name,
                t.metadata.description,
                "built-in".to_string(),
            ));
        }
    }

    // Local
    let templates_dir = crate::config::config_dir().join("templates");
    if let Ok(entries) = std::fs::read_dir(&templates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("template.toml").exists()
                && let Ok(content) = std::fs::read_to_string(path.join("template.toml"))
                && let Ok(t) = Template::parse(&content, path)
            {
                // Skip if already listed as built-in
                if !templates.iter().any(|(n, _, _)| n == &t.metadata.name) {
                    templates.push((t.metadata.name, t.metadata.description, "local".to_string()));
                }
            }
        }
    }

    templates
}

// ── Deep Merge ──────────────────────────────────────────────────

/// Deep merge template overlay into user config TOML value.
pub fn deep_merge(base: &mut toml::Value, overlay: &toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, overlay_val) in overlay_table {
                if let Some(base_val) = base_table.get_mut(key) {
                    deep_merge(base_val, overlay_val);
                } else {
                    base_table.insert(key.clone(), overlay_val.clone());
                }
            }
        }
        (toml::Value::Array(base_arr), toml::Value::Array(overlay_arr)) => {
            base_arr.extend(overlay_arr.iter().cloned());
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

// ── Template Content Access ─────────────────────────────────────

/// Get the SOUL.md content for a template.
pub fn get_template_soul(template: &Template) -> Result<Option<String>> {
    // Check built-in first
    if let Some(built_in) = BUILT_IN_TEMPLATES
        .iter()
        .find(|t| t.name == template.metadata.name)
    {
        return Ok(Some(built_in.soul_md.to_string()));
    }

    // Check template directory
    let soul_path = template.dir.join("SOUL.md");
    if soul_path.exists() {
        let content =
            std::fs::read_to_string(&soul_path).context("Failed to read template SOUL.md")?;
        return Ok(Some(content));
    }

    Ok(None)
}

/// Get optional MEMORY.md content from the template directory.
pub fn get_template_memory(template: &Template) -> Result<Option<String>> {
    let memory_path = template.dir.join("MEMORY.md");
    if memory_path.exists() {
        let content =
            std::fs::read_to_string(&memory_path).context("Failed to read template MEMORY.md")?;
        return Ok(Some(content));
    }
    Ok(None)
}

// ── Active Template State ───────────────────────────────────────

/// Read the active template state
pub fn get_active_template() -> Option<ActiveTemplate> {
    let path = crate::config::config_dir().join(".active-template");
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// Write the active template state
pub fn set_active_template(name: &str, source: &str) -> Result<()> {
    let state = ActiveTemplate {
        name: name.to_string(),
        source: source.to_string(),
        activated_at: chrono::Utc::now().to_rfc3339(),
    };
    let path = crate::config::config_dir().join(".active-template");
    let content = toml::to_string_pretty(&state)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Clear the active template state
pub fn clear_active_template() -> Result<()> {
    let path = crate::config::config_dir().join(".active-template");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_template() {
        let toml_str = r#"
[template]
name = "test-agent"
description = "A test template"

[[goals]]
description = "Do something"
priority = 4
check_interval_secs = 900

[autonomy]
tick_interval_secs = 10
"#;
        let t = Template::parse(toml_str, PathBuf::from("/tmp/test")).unwrap();
        assert_eq!(t.metadata.name, "test-agent");
        assert_eq!(t.goals.len(), 1);
        assert_eq!(t.goals[0].priority, 4);

        let overlay = t.config_overlay.as_table().unwrap();
        assert!(overlay.contains_key("autonomy"));
        assert!(!overlay.contains_key("template"));
        assert!(!overlay.contains_key("goals"));
    }

    #[test]
    fn test_deep_merge_scalars() {
        let mut base: toml::Value = toml::from_str(
            r#"
[autonomy]
tick_interval_secs = 30
max_goals = 50
"#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
[autonomy]
tick_interval_secs = 10
"#,
        )
        .unwrap();
        deep_merge(&mut base, &overlay);
        let autonomy = base.get("autonomy").unwrap().as_table().unwrap();
        assert_eq!(autonomy["tick_interval_secs"].as_integer(), Some(10));
        assert_eq!(autonomy["max_goals"].as_integer(), Some(50));
    }

    #[test]
    fn test_deep_merge_arrays_append() {
        let mut base: toml::Value = toml::from_str(
            r#"
[filesystem]
allowed_directories = ["~/Coding"]
"#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
[filesystem]
allowed_directories = ["~/Projects"]
"#,
        )
        .unwrap();
        deep_merge(&mut base, &overlay);
        let dirs = base["filesystem"]["allowed_directories"]
            .as_array()
            .unwrap();
        assert_eq!(dirs.len(), 2);
    }

    #[test]
    fn test_list_built_in_templates() {
        let templates = list_templates();
        assert!(templates.len() >= 4);
        let names: Vec<&str> = templates.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"stock-analyst"));
        assert!(names.contains(&"code-reviewer"));
        assert!(names.contains(&"personal-assistant"));
        assert!(names.contains(&"research-agent"));
    }

    #[test]
    fn test_resolve_built_in() {
        let t = resolve_template("stock-analyst").unwrap();
        assert_eq!(t.metadata.name, "stock-analyst");
        assert!(!t.goals.is_empty());
    }

    #[test]
    fn test_resolve_nonexistent() {
        let result = resolve_template("nonexistent-template");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_template_soul_built_in() {
        let t = resolve_template("stock-analyst").unwrap();
        let soul = get_template_soul(&t).unwrap();
        assert!(soul.is_some());
        assert!(soul.unwrap().contains("Stock Analyst"));
    }
}
