//! SkillToolHandler — wraps a parsed skill as a ToolHandler

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::ToolHandler;
use super::parser::SkillDefinition;

/// A tool handler that wraps an imported skill
pub struct SkillToolHandler {
    skill: SkillDefinition,
    schema: Value,
}

impl SkillToolHandler {
    pub fn new(skill: SkillDefinition) -> Self {
        // Build JSON Schema from skill inputs
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (name, input) in &skill.inputs {
            let mut prop = serde_json::Map::new();
            prop.insert("type".to_string(), Value::String(input.input_type.clone()));
            if let Some(ref desc) = input.description {
                prop.insert("description".to_string(), Value::String(desc.clone()));
            }
            properties.insert(name.clone(), Value::Object(prop));

            if input.required {
                required.push(Value::String(name.clone()));
            }
        }

        let schema = serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        });

        Self { skill, schema }
    }

    /// Get the skill's allowed commands (for command allowlist validation)
    pub fn allowed_commands(&self) -> &[String] {
        &self.skill.commands
    }
}

#[async_trait]
impl ToolHandler for SkillToolHandler {
    fn name(&self) -> &str {
        &self.skill.name
    }

    fn description(&self) -> &str {
        &self.skill.description
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, input: Value) -> Result<String> {
        // Validate required inputs
        for (name, skill_input) in &self.skill.inputs {
            if skill_input.required {
                if input.get(name).is_none() || input.get(name) == Some(&Value::Null) {
                    return Err(anyhow::anyhow!("Missing required input: {}", name));
                }
            }
        }

        // Build the skill execution prompt
        // The skill doesn't execute commands directly — it returns instructions
        // for the agent's LLM to execute using existing tools (run_command, etc.)
        let mut prompt = format!("## Skill: {}\n\n", self.skill.name);
        prompt.push_str(&self.skill.instructions);
        prompt.push_str("\n\n## User Inputs\n");

        if let Value::Object(map) = &input {
            for (key, value) in map {
                prompt.push_str(&format!("- **{}**: {}\n", key, value));
            }
        }

        if !self.skill.commands.is_empty() {
            prompt.push_str(&format!(
                "\n## Allowed Commands\nYou may use these commands: {}\n",
                self.skill.commands.join(", ")
            ));
        }

        Ok(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::parser::SkillInput;
    use std::collections::HashMap;

    fn make_skill() -> SkillDefinition {
        let mut inputs = HashMap::new();
        inputs.insert("pr_url".to_string(), SkillInput {
            input_type: "string".to_string(),
            required: true,
            description: Some("URL of the PR".to_string()),
        });
        inputs.insert("depth".to_string(), SkillInput {
            input_type: "string".to_string(),
            required: false,
            description: None,
        });

        SkillDefinition {
            name: "review_pr".to_string(),
            description: "Review a pull request".to_string(),
            inputs,
            commands: vec!["gh".to_string()],
            instructions: "Review the PR at the given URL.".to_string(),
        }
    }

    #[test]
    fn test_tool_name_and_description() {
        let skill = make_skill();
        let tool = SkillToolHandler::new(skill);
        assert_eq!(tool.name(), "review_pr");
        assert_eq!(tool.description(), "Review a pull request");
    }

    #[test]
    fn test_schema_generation() {
        let skill = make_skill();
        let tool = SkillToolHandler::new(skill);
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["pr_url"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("pr_url")));
        assert!(!required.contains(&serde_json::json!("depth")));
    }

    #[tokio::test]
    async fn test_execute_returns_instructions() {
        let skill = make_skill();
        let tool = SkillToolHandler::new(skill);
        let result = tool.execute(serde_json::json!({
            "pr_url": "https://github.com/org/repo/pull/123"
        })).await.unwrap();

        assert!(result.contains("Skill: review_pr"));
        assert!(result.contains("Review the PR at the given URL"));
        assert!(result.contains("pr_url"));
        assert!(result.contains("Allowed Commands"));
        assert!(result.contains("gh"));
    }

    #[tokio::test]
    async fn test_execute_missing_required() {
        let skill = make_skill();
        let tool = SkillToolHandler::new(skill);
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pr_url"));
    }

    #[test]
    fn test_allowed_commands() {
        let skill = make_skill();
        let tool = SkillToolHandler::new(skill);
        assert_eq!(tool.allowed_commands(), &["gh".to_string()]);
    }
}
