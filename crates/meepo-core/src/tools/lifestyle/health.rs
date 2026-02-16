//! Health & Habit Tracker tools
//!
//! Track habits, workouts, water intake, sleep, or anything else via natural language.
//! Maintain streaks, send check-in reminders, and generate progress reports.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::tools::{ToolHandler, json_schema};
use meepo_knowledge::KnowledgeDb;

/// Log a habit entry
pub struct LogHabitTool {
    db: Arc<KnowledgeDb>,
}

impl LogHabitTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for LogHabitTool {
    fn name(&self) -> &str {
        "log_habit"
    }

    fn description(&self) -> &str {
        "Log a habit entry. Track workouts, water intake, sleep, meditation, reading, or any \
         custom habit. Entries are timestamped and stored in the knowledge graph for streak \
         tracking and reporting."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "habit": {
                    "type": "string",
                    "description": "Habit name (e.g., 'exercise', 'water', 'sleep', 'meditation', 'reading')"
                },
                "value": {
                    "type": "string",
                    "description": "Value/amount (e.g., '30 minutes', '8 glasses', '7.5 hours', '3 miles')"
                },
                "notes": {
                    "type": "string",
                    "description": "Optional notes (e.g., 'ran in the park', 'felt great')"
                },
                "date": {
                    "type": "string",
                    "description": "Date of the entry (default: today). Format: YYYY-MM-DD"
                }
            }),
            vec!["habit"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let habit = input
            .get("habit")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'habit' parameter"))?;
        let value = input
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("completed");
        let notes = input.get("notes").and_then(|v| v.as_str());
        let date = input
            .get("date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

        if habit.len() > 200 {
            return Err(anyhow::anyhow!("Habit name too long (max 200 characters)"));
        }

        debug!("Logging habit: {} = {}", habit, value);

        let entry_name = format!("habit:{}:{}", habit, date);
        let entry_id = self
            .db
            .insert_entity(
                &entry_name,
                "habit_entry",
                Some(serde_json::json!({
                    "habit": habit,
                    "value": value,
                    "notes": notes,
                    "date": date,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                })),
            )
            .await?;

        // Ensure habit definition entity exists
        let habits = self
            .db
            .search_entities(&format!("habit_def:{}", habit), Some("habit_definition"))
            .await
            .unwrap_or_default();
        let habit_def_id = if let Some(existing) = habits.first() {
            existing.id.clone()
        } else {
            self.db
                .insert_entity(
                    &format!("habit_def:{}", habit),
                    "habit_definition",
                    Some(serde_json::json!({
                        "name": habit,
                        "created_at": chrono::Utc::now().to_rfc3339(),
                    })),
                )
                .await?
        };

        // Link entry to habit definition
        let _ = self
            .db
            .insert_relationship(&entry_id, &habit_def_id, "instance_of", None)
            .await;

        // Calculate current streak
        let all_entries = self
            .db
            .search_entities(&format!("habit:{}", habit), Some("habit_entry"))
            .await
            .unwrap_or_default();

        let mut dates: Vec<String> = all_entries
            .iter()
            .filter_map(|e| {
                e.metadata
                    .as_ref()
                    .and_then(|m| m.get("date"))
                    .and_then(|d| d.as_str())
                    .map(String::from)
            })
            .collect();
        dates.sort();
        dates.dedup();

        let streak = calculate_streak(&dates);

        Ok(format!(
            "Habit logged:\n\
             - Habit: {}\n\
             - Value: {}\n\
             - Date: {}\n\
             - Current streak: {} days\n\
             - Total entries: {}{}",
            habit,
            value,
            date,
            streak,
            dates.len(),
            notes
                .map(|n| format!("\n- Notes: {}", n))
                .unwrap_or_default()
        ))
    }
}

/// Calculate streak from sorted date strings
fn calculate_streak(dates: &[String]) -> usize {
    if dates.is_empty() {
        return 0;
    }

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    // Check if the most recent entry is today or yesterday
    let last = dates.last().unwrap();
    if last != &today && last != &yesterday {
        return 0;
    }

    let mut streak = 1;
    for i in (0..dates.len() - 1).rev() {
        // Simple consecutive day check using string comparison
        // This is a heuristic — proper date parsing would be more accurate
        let current = &dates[i + 1];
        let prev = &dates[i];

        // Parse dates for proper comparison
        if let (Ok(curr_date), Ok(prev_date)) = (
            chrono::NaiveDate::parse_from_str(current, "%Y-%m-%d"),
            chrono::NaiveDate::parse_from_str(prev, "%Y-%m-%d"),
        ) {
            if curr_date - prev_date == chrono::Duration::days(1) {
                streak += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    streak
}

/// Get habit streak information
pub struct HabitStreakTool {
    db: Arc<KnowledgeDb>,
}

impl HabitStreakTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for HabitStreakTool {
    fn name(&self) -> &str {
        "habit_streak"
    }

    fn description(&self) -> &str {
        "Get streak information for a habit or all habits. Shows current streak, longest streak, \
         total entries, and recent activity."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "habit": {
                    "type": "string",
                    "description": "Habit name (omit for all habits overview)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let habit = input.get("habit").and_then(|v| v.as_str());

        debug!("Getting habit streak: {}", habit.unwrap_or("all habits"));

        // Get all habit definitions
        let definitions = self
            .db
            .search_entities("habit_def:", Some("habit_definition"))
            .await
            .unwrap_or_default();

        if definitions.is_empty() {
            return Ok("No habits tracked yet. Use log_habit to start tracking.".to_string());
        }

        let mut output = String::from("# Habit Streaks\n\n");

        for def in &definitions {
            let habit_name = def
                .metadata
                .as_ref()
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(&def.name);

            if let Some(filter) = habit {
                if !habit_name.contains(filter) {
                    continue;
                }
            }

            let entries = self
                .db
                .search_entities(&format!("habit:{}", habit_name), Some("habit_entry"))
                .await
                .unwrap_or_default();

            let mut dates: Vec<String> = entries
                .iter()
                .filter_map(|e| {
                    e.metadata
                        .as_ref()
                        .and_then(|m| m.get("date"))
                        .and_then(|d| d.as_str())
                        .map(String::from)
                })
                .collect();
            dates.sort();
            dates.dedup();

            let current_streak = calculate_streak(&dates);
            let last_entry = dates.last().cloned().unwrap_or_else(|| "never".to_string());

            output.push_str(&format!(
                "## {}\n\
                 - Current streak: {} days\n\
                 - Total entries: {}\n\
                 - Last entry: {}\n\n",
                habit_name,
                current_streak,
                dates.len(),
                last_entry
            ));
        }

        Ok(output)
    }
}

/// Generate a habit report
pub struct HabitReportTool {
    db: Arc<KnowledgeDb>,
}

impl HabitReportTool {
    pub fn new(db: Arc<KnowledgeDb>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ToolHandler for HabitReportTool {
    fn name(&self) -> &str {
        "habit_report"
    }

    fn description(&self) -> &str {
        "Generate a comprehensive habit report for a time period. Shows completion rates, \
         streaks, trends, and insights across all tracked habits."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "period": {
                    "type": "string",
                    "description": "Report period: this_week, this_month, last_month, all_time (default: this_week)"
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let period = input
            .get("period")
            .and_then(|v| v.as_str())
            .unwrap_or("this_week");

        debug!("Generating habit report for: {}", period);

        let definitions = self
            .db
            .search_entities("habit_def:", Some("habit_definition"))
            .await
            .unwrap_or_default();

        if definitions.is_empty() {
            return Ok("No habits tracked yet. Use log_habit to start tracking.".to_string());
        }

        let days_in_period = match period {
            "this_week" => 7,
            "this_month" => 30,
            "last_month" => 30,
            "all_time" => 365,
            _ => 7,
        };

        let mut output = format!("# Habit Report ({})\n\n", period);

        for def in &definitions {
            let habit_name = def
                .metadata
                .as_ref()
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(&def.name);

            let entries = self
                .db
                .search_entities(&format!("habit:{}", habit_name), Some("habit_entry"))
                .await
                .unwrap_or_default();

            let mut dates: Vec<String> = entries
                .iter()
                .filter_map(|e| {
                    e.metadata
                        .as_ref()
                        .and_then(|m| m.get("date"))
                        .and_then(|d| d.as_str())
                        .map(String::from)
                })
                .collect();
            dates.sort();
            dates.dedup();

            let completion_rate = if days_in_period > 0 {
                (dates.len() as f64 / days_in_period as f64 * 100.0).min(100.0) as u32
            } else {
                0
            };

            let current_streak = calculate_streak(&dates);

            // Get recent values
            let recent_values: Vec<String> = entries
                .iter()
                .rev()
                .take(5)
                .filter_map(|e| {
                    let meta = e.metadata.as_ref()?;
                    let date = meta.get("date")?.as_str()?;
                    let value = meta.get("value")?.as_str()?;
                    Some(format!("  {} — {}", date, value))
                })
                .collect();

            output.push_str(&format!(
                "## {}\n\
                 - Completion: {}% ({}/{} days)\n\
                 - Current streak: {} days\n\
                 - Recent:\n{}\n\n",
                habit_name,
                completion_rate,
                dates.len(),
                days_in_period,
                current_streak,
                if recent_values.is_empty() {
                    "  No entries".to_string()
                } else {
                    recent_values.join("\n")
                }
            ));
        }

        output.push_str(
            "\nPlease analyze the data above and provide:\n\
             1. **Highlights** — best performing habits\n\
             2. **Needs Attention** — habits with declining or low completion\n\
             3. **Suggestions** — actionable tips to improve consistency",
        );

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<KnowledgeDb> {
        Arc::new(KnowledgeDb::new(&std::env::temp_dir().join("test_health.db")).unwrap())
    }

    #[test]
    fn test_log_habit_schema() {
        let tool = LogHabitTool::new(test_db());
        assert_eq!(tool.name(), "log_habit");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema
                .get("required")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"habit".to_string()));
    }

    #[test]
    fn test_habit_streak_schema() {
        let tool = HabitStreakTool::new(test_db());
        assert_eq!(tool.name(), "habit_streak");
    }

    #[test]
    fn test_habit_report_schema() {
        let tool = HabitReportTool::new(test_db());
        assert_eq!(tool.name(), "habit_report");
    }

    #[test]
    fn test_calculate_streak_empty() {
        assert_eq!(calculate_streak(&[]), 0);
    }

    #[test]
    fn test_calculate_streak_consecutive() {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let day_before = (chrono::Local::now() - chrono::Duration::days(2))
            .format("%Y-%m-%d")
            .to_string();

        let dates = vec![day_before, yesterday, today];
        assert_eq!(calculate_streak(&dates), 3);
    }
}
