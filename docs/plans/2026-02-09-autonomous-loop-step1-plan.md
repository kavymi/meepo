# Autonomous Loop Step 1 — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the reactive message handler with a tick-based autonomous loop, add SQLite tables for goals/preferences/action log, and wire everything into the daemon.

**Architecture:** New `autonomy/` module in meepo-core owns the loop. New tables extend KnowledgeDb. The loop drains user messages + watcher events each tick, delegates to Agent::handle_message for now, and checks for due goals. Main.rs swaps the current `select!` loop for `AutonomousLoop::run()`.

**Tech Stack:** Rust, tokio (select!, mpsc, Notify), rusqlite, serde_json, chrono

---

### Task 1: Add Goals Table to KnowledgeDb

**Files:**
- Modify: `crates/meepo-knowledge/src/sqlite.rs`

**Step 1: Add Goal struct after the Watcher struct (line ~59)**

```rust
/// Autonomous goal tracked by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub status: String,          // active|paused|completed|failed
    pub priority: i32,           // 1 (low) to 5 (critical)
    pub success_criteria: Option<String>,
    pub strategy: Option<String>,
    pub check_interval_secs: i64,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub source_channel: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Step 2: Add CREATE TABLE in `KnowledgeDb::new()` after the watchers table (after line ~130)**

```rust
// Create goals table
conn.execute(
    "CREATE TABLE IF NOT EXISTS goals (
        id TEXT PRIMARY KEY,
        description TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        priority INTEGER NOT NULL DEFAULT 3,
        success_criteria TEXT,
        strategy TEXT,
        check_interval_secs INTEGER NOT NULL DEFAULT 1800,
        last_checked_at TEXT,
        source_channel TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    [],
)?;
conn.execute(
    "CREATE INDEX IF NOT EXISTS idx_goals_status ON goals(status)",
    [],
)?;
```

**Step 3: Add CRUD methods for goals**

Add these methods to `impl KnowledgeDb` after the watcher methods:

```rust
/// Insert a new goal
pub async fn insert_goal(
    &self,
    description: &str,
    priority: i32,
    check_interval_secs: i64,
    success_criteria: Option<&str>,
    source_channel: Option<&str>,
) -> Result<String> {
    let conn = Arc::clone(&self.conn);
    let description = description.to_owned();
    let success_criteria = success_criteria.map(|s| s.to_owned());
    let source_channel = source_channel.map(|s| s.to_owned());

    tokio::task::spawn_blocking(move || {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO goals (id, description, status, priority, success_criteria, check_interval_secs, source_channel, created_at, updated_at)
             VALUES (?1, ?2, 'active', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![&id, &description, priority, success_criteria, check_interval_secs, source_channel, now.to_rfc3339(), now.to_rfc3339()],
        )?;
        debug!("Inserted goal: {} ({})", description, id);
        Ok(id)
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Get active goals that are due for checking
pub async fn get_due_goals(&self) -> Result<Vec<Goal>> {
    let conn = Arc::clone(&self.conn);

    tokio::task::spawn_blocking(move || {
        let now = Utc::now();
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, description, status, priority, success_criteria, strategy,
                    check_interval_secs, last_checked_at, source_channel, created_at, updated_at
             FROM goals
             WHERE status = 'active'
               AND (last_checked_at IS NULL
                    OR strftime('%s', 'now') - strftime('%s', last_checked_at) >= check_interval_secs)
             ORDER BY priority DESC, created_at ASC",
        )?;

        let goals = stmt
            .query_map([], Self::row_to_goal)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(goals)
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Get all active goals
pub async fn get_active_goals(&self) -> Result<Vec<Goal>> {
    let conn = Arc::clone(&self.conn);

    tokio::task::spawn_blocking(move || {
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, description, status, priority, success_criteria, strategy,
                    check_interval_secs, last_checked_at, source_channel, created_at, updated_at
             FROM goals WHERE status = 'active'
             ORDER BY priority DESC, created_at ASC",
        )?;
        let goals = stmt
            .query_map([], Self::row_to_goal)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(goals)
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Update goal status
pub async fn update_goal_status(&self, id: &str, status: &str) -> Result<()> {
    let conn = Arc::clone(&self.conn);
    let id = id.to_owned();
    let status = status.to_owned();

    tokio::task::spawn_blocking(move || {
        let now = Utc::now();
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE goals SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![&status, now.to_rfc3339(), &id],
        )?;
        Ok(())
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Update goal strategy and mark as checked
pub async fn update_goal_checked(&self, id: &str, strategy: Option<&str>) -> Result<()> {
    let conn = Arc::clone(&self.conn);
    let id = id.to_owned();
    let strategy = strategy.map(|s| s.to_owned());

    tokio::task::spawn_blocking(move || {
        let now = Utc::now();
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE goals SET last_checked_at = ?1, strategy = COALESCE(?2, strategy), updated_at = ?3 WHERE id = ?4",
            params![now.to_rfc3339(), strategy, now.to_rfc3339(), &id],
        )?;
        Ok(())
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Helper to convert row to Goal
fn row_to_goal(row: &rusqlite::Row) -> rusqlite::Result<Goal> {
    Ok(Goal {
        id: row.get(0)?,
        description: row.get(1)?,
        status: row.get(2)?,
        priority: row.get(3)?,
        success_criteria: row.get(4)?,
        strategy: row.get(5)?,
        check_interval_secs: row.get(6)?,
        last_checked_at: row.get::<_, Option<String>>(7)?
            .and_then(|s| s.parse().ok()),
        source_channel: row.get(8)?,
        created_at: row.get::<_, String>(9)?.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: row.get::<_, String>(10)?.parse().unwrap_or_else(|_| Utc::now()),
    })
}
```

**Step 4: Add Goal to the re-exports in `crates/meepo-knowledge/src/lib.rs`**

Change line 15 from:
```rust
pub use sqlite::{KnowledgeDb, Entity, Relationship, Conversation, Watcher};
```
to:
```rust
pub use sqlite::{KnowledgeDb, Entity, Relationship, Conversation, Watcher, Goal};
```

**Step 5: Add test for goal operations**

Add to the `#[cfg(test)] mod tests` block at the bottom of sqlite.rs:

```rust
#[tokio::test]
async fn test_goal_operations() -> Result<()> {
    let temp_path = env::temp_dir().join("test_goals.db");
    let _ = std::fs::remove_file(&temp_path);
    let db = KnowledgeDb::new(&temp_path)?;

    // Insert goal
    let id = db.insert_goal("Review PRs daily", 3, 3600, Some("All PRs reviewed"), Some("discord")).await?;
    assert!(!id.is_empty());

    // Get active goals
    let goals = db.get_active_goals().await?;
    assert_eq!(goals.len(), 1);
    assert_eq!(goals[0].description, "Review PRs daily");

    // Get due goals (should be due immediately since last_checked_at is NULL)
    let due = db.get_due_goals().await?;
    assert_eq!(due.len(), 1);

    // Mark as checked
    db.update_goal_checked(&id, Some("Check GitHub PRs tool")).await?;

    // Should no longer be due (just checked, interval is 3600s)
    let due = db.get_due_goals().await?;
    assert_eq!(due.len(), 0);

    // Update status
    db.update_goal_status(&id, "completed").await?;
    let active = db.get_active_goals().await?;
    assert_eq!(active.len(), 0);

    let _ = std::fs::remove_file(&temp_path);
    Ok(())
}
```

**Step 6: Run test**

Run: `cargo test -p meepo-knowledge test_goal_operations -- --nocapture`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/meepo-knowledge/src/sqlite.rs crates/meepo-knowledge/src/lib.rs
git commit -m "feat: add goals table and CRUD to KnowledgeDb"
```

---

### Task 2: Add User Preferences and Action Log Tables

**Files:**
- Modify: `crates/meepo-knowledge/src/sqlite.rs`
- Modify: `crates/meepo-knowledge/src/lib.rs`

**Step 1: Add UserPreference and ActionLog structs after Goal**

```rust
/// Learned user preference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreference {
    pub id: String,
    pub category: String,        // communication|schedule|code|workflow
    pub key: String,
    pub value: JsonValue,
    pub confidence: f64,         // 0.0 to 1.0
    pub learned_from: Option<String>,
    pub last_confirmed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Log of autonomous actions taken
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLogEntry {
    pub id: String,
    pub goal_id: Option<String>,
    pub action_type: String,
    pub description: String,
    pub outcome: String,         // success|failed|pending|unknown
    pub user_feedback: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

**Step 2: Add CREATE TABLEs in `KnowledgeDb::new()` after goals table**

```rust
// Create user_preferences table
conn.execute(
    "CREATE TABLE IF NOT EXISTS user_preferences (
        id TEXT PRIMARY KEY,
        category TEXT NOT NULL,
        key TEXT NOT NULL UNIQUE,
        value TEXT NOT NULL,
        confidence REAL NOT NULL DEFAULT 0.3,
        learned_from TEXT,
        last_confirmed_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    [],
)?;
conn.execute(
    "CREATE INDEX IF NOT EXISTS idx_preferences_category ON user_preferences(category)",
    [],
)?;

// Create action_log table
conn.execute(
    "CREATE TABLE IF NOT EXISTS action_log (
        id TEXT PRIMARY KEY,
        goal_id TEXT,
        action_type TEXT NOT NULL,
        description TEXT NOT NULL,
        outcome TEXT NOT NULL DEFAULT 'pending',
        user_feedback TEXT,
        created_at TEXT NOT NULL,
        FOREIGN KEY (goal_id) REFERENCES goals(id)
    )",
    [],
)?;
conn.execute(
    "CREATE INDEX IF NOT EXISTS idx_action_log_goal ON action_log(goal_id)",
    [],
)?;
```

**Step 3: Add CRUD methods for preferences**

```rust
/// Upsert a user preference (insert or update by key)
pub async fn upsert_preference(
    &self,
    category: &str,
    key: &str,
    value: JsonValue,
    confidence: f64,
    learned_from: Option<&str>,
) -> Result<String> {
    let conn = Arc::clone(&self.conn);
    let category = category.to_owned();
    let key = key.to_owned();
    let learned_from = learned_from.map(|s| s.to_owned());

    tokio::task::spawn_blocking(move || {
        let now = Utc::now();
        let value_str = serde_json::to_string(&value)?;
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());

        // Try update first
        let updated = conn.execute(
            "UPDATE user_preferences SET value = ?1, confidence = ?2, learned_from = COALESCE(?3, learned_from), updated_at = ?4 WHERE key = ?5",
            params![&value_str, confidence, learned_from, now.to_rfc3339(), &key],
        )?;

        if updated > 0 {
            // Return existing id
            let id: String = conn.query_row(
                "SELECT id FROM user_preferences WHERE key = ?1",
                params![&key],
                |row| row.get(0),
            )?;
            return Ok(id);
        }

        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO user_preferences (id, category, key, value, confidence, learned_from, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![&id, &category, &key, &value_str, confidence, learned_from, now.to_rfc3339(), now.to_rfc3339()],
        )?;
        Ok(id)
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Get all preferences, optionally filtered by category
pub async fn get_preferences(&self, category: Option<&str>) -> Result<Vec<UserPreference>> {
    let conn = Arc::clone(&self.conn);
    let category = category.map(|s| s.to_owned());

    tokio::task::spawn_blocking(move || {
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        let (sql, params_vec): (&str, Vec<String>) = if let Some(ref cat) = category {
            ("SELECT id, category, key, value, confidence, learned_from, last_confirmed_at, created_at, updated_at
              FROM user_preferences WHERE category = ?1 ORDER BY confidence DESC",
             vec![cat.clone()])
        } else {
            ("SELECT id, category, key, value, confidence, learned_from, last_confirmed_at, created_at, updated_at
              FROM user_preferences ORDER BY confidence DESC",
             vec![])
        };

        let mut stmt = conn.prepare(sql)?;
        let prefs = if category.is_some() {
            stmt.query_map(params![&params_vec[0]], Self::row_to_preference)?
        } else {
            stmt.query_map([], Self::row_to_preference)?
        }
        .collect::<Result<Vec<_>, _>>()?;
        Ok(prefs)
    })
    .await
    .context("spawn_blocking task panicked")?
}

fn row_to_preference(row: &rusqlite::Row) -> rusqlite::Result<UserPreference> {
    let value_str: String = row.get(3)?;
    let value = serde_json::from_str(&value_str)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;

    Ok(UserPreference {
        id: row.get(0)?,
        category: row.get(1)?,
        key: row.get(2)?,
        value,
        confidence: row.get(4)?,
        learned_from: row.get(5)?,
        last_confirmed_at: row.get::<_, Option<String>>(6)?.and_then(|s| s.parse().ok()),
        created_at: row.get::<_, String>(7)?.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: row.get::<_, String>(8)?.parse().unwrap_or_else(|_| Utc::now()),
    })
}
```

**Step 4: Add methods for action log**

```rust
/// Insert an action log entry
pub async fn insert_action_log(
    &self,
    goal_id: Option<&str>,
    action_type: &str,
    description: &str,
    outcome: &str,
) -> Result<String> {
    let conn = Arc::clone(&self.conn);
    let goal_id = goal_id.map(|s| s.to_owned());
    let action_type = action_type.to_owned();
    let description = description.to_owned();
    let outcome = outcome.to_owned();

    tokio::task::spawn_blocking(move || {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO action_log (id, goal_id, action_type, description, outcome, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&id, goal_id, &action_type, &description, &outcome, now.to_rfc3339()],
        )?;
        Ok(id)
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Get recent action log entries
pub async fn get_recent_actions(&self, limit: usize) -> Result<Vec<ActionLogEntry>> {
    let conn = Arc::clone(&self.conn);

    tokio::task::spawn_blocking(move || {
        let conn = conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, goal_id, action_type, description, outcome, user_feedback, created_at
             FROM action_log ORDER BY created_at DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(ActionLogEntry {
                    id: row.get(0)?,
                    goal_id: row.get(1)?,
                    action_type: row.get(2)?,
                    description: row.get(3)?,
                    outcome: row.get(4)?,
                    user_feedback: row.get(5)?,
                    created_at: row.get::<_, String>(6)?.parse().unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    })
    .await
    .context("spawn_blocking task panicked")?
}
```

**Step 5: Update re-exports in lib.rs**

```rust
pub use sqlite::{KnowledgeDb, Entity, Relationship, Conversation, Watcher, Goal, UserPreference, ActionLogEntry};
```

**Step 6: Add tests**

```rust
#[tokio::test]
async fn test_preference_operations() -> Result<()> {
    let temp_path = env::temp_dir().join("test_prefs.db");
    let _ = std::fs::remove_file(&temp_path);
    let db = KnowledgeDb::new(&temp_path)?;

    // Upsert preference
    let id = db.upsert_preference("schedule", "morning_summary", serde_json::json!(true), 0.5, Some("user asked 3 times")).await?;
    assert!(!id.is_empty());

    // Get preferences
    let prefs = db.get_preferences(Some("schedule")).await?;
    assert_eq!(prefs.len(), 1);
    assert_eq!(prefs[0].key, "morning_summary");

    // Upsert same key updates
    let id2 = db.upsert_preference("schedule", "morning_summary", serde_json::json!(true), 0.8, None).await?;
    assert_eq!(id, id2);
    let prefs = db.get_preferences(None).await?;
    assert_eq!(prefs.len(), 1);
    assert!((prefs[0].confidence - 0.8).abs() < 0.01);

    let _ = std::fs::remove_file(&temp_path);
    Ok(())
}

#[tokio::test]
async fn test_action_log_operations() -> Result<()> {
    let temp_path = env::temp_dir().join("test_actions.db");
    let _ = std::fs::remove_file(&temp_path);
    let db = KnowledgeDb::new(&temp_path)?;

    let id = db.insert_action_log(None, "sent_email", "Sent morning summary", "success").await?;
    assert!(!id.is_empty());

    let actions = db.get_recent_actions(10).await?;
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_type, "sent_email");

    let _ = std::fs::remove_file(&temp_path);
    Ok(())
}
```

**Step 7: Run tests**

Run: `cargo test -p meepo-knowledge -- --nocapture`
Expected: All PASS

**Step 8: Commit**

```bash
git add crates/meepo-knowledge/src/sqlite.rs crates/meepo-knowledge/src/lib.rs
git commit -m "feat: add user_preferences and action_log tables to KnowledgeDb"
```

---

### Task 3: Add Autonomy Config Section

**Files:**
- Modify: `crates/meepo-cli/src/config.rs`
- Modify: `config/default.toml`

**Step 1: Add AutonomyConfig struct in config.rs after OrchestratorConfig (after line ~230)**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyConfig {
    #[serde(default = "default_autonomy_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tick_interval")]
    pub tick_interval_secs: u64,
    #[serde(default = "default_max_goals")]
    pub max_goals: usize,
    #[serde(default = "default_preference_decay_days")]
    pub preference_decay_days: u32,
    #[serde(default = "default_min_confidence")]
    pub min_confidence_to_act: f64,
    #[serde(default = "default_max_tokens_per_tick")]
    pub max_tokens_per_tick: u32,
}

fn default_autonomy_enabled() -> bool { true }
fn default_tick_interval() -> u64 { 30 }
fn default_max_goals() -> usize { 50 }
fn default_preference_decay_days() -> u32 { 30 }
fn default_min_confidence() -> f64 { 0.5 }
fn default_max_tokens_per_tick() -> u32 { 4096 }

fn default_autonomy_config() -> AutonomyConfig {
    AutonomyConfig {
        enabled: default_autonomy_enabled(),
        tick_interval_secs: default_tick_interval(),
        max_goals: default_max_goals(),
        preference_decay_days: default_preference_decay_days(),
        min_confidence_to_act: default_min_confidence(),
        max_tokens_per_tick: default_max_tokens_per_tick(),
    }
}
```

**Step 2: Add to MeepoConfig struct**

Add after the `orchestrator` field (line ~18):
```rust
    #[serde(default = "default_autonomy_config")]
    pub autonomy: AutonomyConfig,
```

**Step 3: Add `[autonomy]` section to default.toml**

Append after the `[orchestrator]` section:

```toml

# ── Autonomous Agent ─────────────────────────────────────────────
# Continuous loop that pursues goals and learns preferences.

[autonomy]
enabled = true
tick_interval_secs = 30       # idle tick rate
max_goals = 50                # prevent runaway goal creation
preference_decay_days = 30    # confidence decay period
min_confidence_to_act = 0.5   # below this, ask user first
max_tokens_per_tick = 4096    # budget per think phase
```

**Step 4: Verify config parses**

Run: `python3 -c "import tomllib; tomllib.load(open('config/default.toml','rb')); print('OK')"`
Expected: OK

Run: `cargo check -p meepo-cli`
Expected: compiles clean

**Step 5: Commit**

```bash
git add crates/meepo-cli/src/config.rs config/default.toml
git commit -m "feat: add [autonomy] config section"
```

---

### Task 4: Create Autonomy Module Skeleton

**Files:**
- Create: `crates/meepo-core/src/autonomy/mod.rs`
- Create: `crates/meepo-core/src/autonomy/goals.rs`
- Create: `crates/meepo-core/src/autonomy/user_model.rs`
- Create: `crates/meepo-core/src/autonomy/action_log.rs`
- Create: `crates/meepo-core/src/autonomy/planner.rs`
- Modify: `crates/meepo-core/src/lib.rs`

**Step 1: Create the module files with minimal content**

`crates/meepo-core/src/autonomy/goals.rs`:
```rust
//! Goal management for the autonomous agent

// Goal evaluation and lifecycle will be implemented in Step 2
```

`crates/meepo-core/src/autonomy/user_model.rs`:
```rust
//! User preference learning for the autonomous agent

// Preference learning will be implemented in Step 4
```

`crates/meepo-core/src/autonomy/action_log.rs`:
```rust
//! Action logging for the autonomous agent

// Action outcome tracking will be implemented in Step 2
```

`crates/meepo-core/src/autonomy/planner.rs`:
```rust
//! Situation report builder and decision parser

// Planning and decision parsing will be implemented in Step 2
```

`crates/meepo-core/src/autonomy/mod.rs`:
```rust
//! Autonomous agent loop — observe/think/act/reflect cycle
//!
//! Replaces the reactive message handler with a continuous tick-based loop.
//! User messages are just one input among many — the agent also processes
//! watcher events, evaluates goals, and takes proactive actions.

pub mod goals;
pub mod user_model;
pub mod action_log;
pub mod planner;

use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use tokio::sync::{mpsc, Notify};
use tracing::{info, error, debug};

use crate::agent::Agent;
use crate::types::{IncomingMessage, OutgoingMessage, ChannelType};
use meepo_knowledge::KnowledgeDb;
use meepo_scheduler::runner::WatcherEvent;

/// Configuration for the autonomous loop
#[derive(Debug, Clone)]
pub struct AutonomyConfig {
    pub enabled: bool,
    pub tick_interval_secs: u64,
    pub max_goals: usize,
}

/// Input that the autonomous loop processes each tick
#[derive(Debug)]
enum LoopInput {
    UserMessage(IncomingMessage),
    WatcherEvent(WatcherEvent),
}

/// The autonomous loop that drives the agent
pub struct AutonomousLoop {
    agent: Arc<Agent>,
    db: Arc<KnowledgeDb>,
    config: AutonomyConfig,

    /// Receives user messages from channels
    message_rx: mpsc::Receiver<IncomingMessage>,

    /// Receives watcher events from the scheduler
    watcher_rx: mpsc::UnboundedReceiver<WatcherEvent>,

    /// Sends responses back to channels
    response_tx: mpsc::Sender<OutgoingMessage>,

    /// Notified when a new input arrives (to wake the loop immediately)
    wake: Arc<Notify>,
}

impl AutonomousLoop {
    pub fn new(
        agent: Arc<Agent>,
        db: Arc<KnowledgeDb>,
        config: AutonomyConfig,
        message_rx: mpsc::Receiver<IncomingMessage>,
        watcher_rx: mpsc::UnboundedReceiver<WatcherEvent>,
        response_tx: mpsc::Sender<OutgoingMessage>,
        wake: Arc<Notify>,
    ) -> Self {
        Self {
            agent,
            db,
            config,
            message_rx,
            watcher_rx,
            response_tx,
            wake,
        }
    }

    /// Create a Notify handle that can be shared with message producers
    /// to wake the loop immediately when new inputs arrive.
    pub fn create_wake_handle() -> Arc<Notify> {
        Arc::new(Notify::new())
    }

    /// Run the autonomous loop until cancelled
    pub async fn run(mut self, cancel: tokio_util::sync::CancellationToken) {
        info!("Autonomous loop started (tick interval: {}s)", self.config.tick_interval_secs);

        let tick_duration = Duration::from_secs(self.config.tick_interval_secs);

        loop {
            // Wait for: cancellation, tick timer, or wake signal
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Autonomous loop shutting down");
                    break;
                }
                _ = tokio::time::sleep(tick_duration) => {
                    // Periodic tick — check for due goals and process any pending inputs
                }
                _ = self.wake.notified() => {
                    // Immediate wake — new input arrived
                    debug!("Autonomous loop woken by new input");
                }
            }

            // OBSERVE: drain all pending inputs
            let inputs = self.drain_inputs();

            // Check for due goals
            let due_goals = match self.db.get_due_goals().await {
                Ok(goals) => goals,
                Err(e) => {
                    error!("Failed to get due goals: {}", e);
                    vec![]
                }
            };

            // Skip tick if nothing to do
            if inputs.is_empty() && due_goals.is_empty() {
                continue;
            }

            debug!(
                "Tick: {} inputs, {} due goals",
                inputs.len(),
                due_goals.len()
            );

            // THINK + ACT: process inputs
            // For now, handle user messages via the existing Agent::handle_message path.
            // Goal evaluation will be added in Step 2.
            for input in inputs {
                match input {
                    LoopInput::UserMessage(msg) => {
                        self.handle_user_message(msg).await;
                    }
                    LoopInput::WatcherEvent(event) => {
                        self.handle_watcher_event(event).await;
                    }
                }
            }

            // Mark due goals as checked (placeholder — real evaluation in Step 2)
            for goal in &due_goals {
                if let Err(e) = self.db.update_goal_checked(&goal.id, None).await {
                    error!("Failed to mark goal {} as checked: {}", goal.id, e);
                }
            }
        }
    }

    /// Drain all pending inputs from channels without blocking
    fn drain_inputs(&mut self) -> Vec<LoopInput> {
        let mut inputs = Vec::new();

        // Drain user messages
        while let Ok(msg) = self.message_rx.try_recv() {
            inputs.push(LoopInput::UserMessage(msg));
        }

        // Drain watcher events
        while let Ok(event) = self.watcher_rx.try_recv() {
            inputs.push(LoopInput::WatcherEvent(event));
        }

        inputs
    }

    /// Handle a user message through the existing agent path
    async fn handle_user_message(&self, msg: IncomingMessage) {
        let channel = msg.channel.clone();
        info!("Processing user message from {} on {}", msg.sender, channel);

        match self.agent.handle_message(msg).await {
            Ok(response) => {
                if let Err(e) = self.response_tx.send(response).await {
                    error!("Failed to send response: {}", e);
                }
            }
            Err(e) => error!("Agent error: {}", e),
        }
    }

    /// Handle a watcher event
    async fn handle_watcher_event(&self, event: WatcherEvent) {
        info!("Processing watcher event: {} from {}", event.kind, event.watcher_id);

        // Convert watcher event to IncomingMessage (same as current behavior)
        let msg = IncomingMessage {
            id: uuid::Uuid::new_v4().to_string(),
            sender: "watcher".to_string(),
            content: format!("Watcher {} triggered: {}", event.watcher_id, event.payload),
            channel: ChannelType::Internal,
            timestamp: chrono::Utc::now(),
        };

        match self.agent.handle_message(msg).await {
            Ok(response) => {
                // For now, log internal responses (channel routing fix in Step 3)
                info!("Watcher {} response: {}", event.watcher_id, &response.content[..response.content.len().min(200)]);
            }
            Err(e) => error!("Failed to handle watcher event: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiClient;
    use crate::tools::ToolRegistry;
    use tempfile::TempDir;

    fn setup() -> (Arc<Agent>, Arc<KnowledgeDb>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(KnowledgeDb::new(&db_path).unwrap());
        let api = ApiClient::new("test-key".to_string(), None);
        let tools = Arc::new(ToolRegistry::new());
        let agent = Arc::new(Agent::new(api, tools, "test soul".into(), "test memory".into(), db.clone()));
        (agent, db, temp_dir)
    }

    #[tokio::test]
    async fn test_drain_inputs_empty() {
        let (agent, db, _tmp) = setup();
        let (_, msg_rx) = mpsc::channel(16);
        let (_, watcher_rx) = mpsc::unbounded_channel();
        let (resp_tx, _) = mpsc::channel(16);
        let wake = AutonomousLoop::create_wake_handle();

        let mut loop_ = AutonomousLoop::new(
            agent, db,
            AutonomyConfig { enabled: true, tick_interval_secs: 30, max_goals: 50 },
            msg_rx, watcher_rx, resp_tx, wake,
        );

        let inputs = loop_.drain_inputs();
        assert!(inputs.is_empty());
    }

    #[tokio::test]
    async fn test_drain_inputs_with_messages() {
        let (agent, db, _tmp) = setup();
        let (msg_tx, msg_rx) = mpsc::channel(16);
        let (_, watcher_rx) = mpsc::unbounded_channel();
        let (resp_tx, _) = mpsc::channel(16);
        let wake = AutonomousLoop::create_wake_handle();

        // Send a message before creating the loop
        msg_tx.send(IncomingMessage {
            id: "test-1".into(),
            sender: "user".into(),
            content: "hello".into(),
            channel: ChannelType::Discord,
            timestamp: chrono::Utc::now(),
        }).await.unwrap();

        let mut loop_ = AutonomousLoop::new(
            agent, db,
            AutonomyConfig { enabled: true, tick_interval_secs: 30, max_goals: 50 },
            msg_rx, watcher_rx, resp_tx, wake,
        );

        let inputs = loop_.drain_inputs();
        assert_eq!(inputs.len(), 1);
    }
}
```

**Step 2: Register the module in lib.rs**

Add after `pub mod types;` (line 17):
```rust
pub mod autonomy;
```

Add to re-exports:
```rust
pub use autonomy::{AutonomousLoop, AutonomyConfig};
```

**Step 3: Run tests**

Run: `cargo test -p meepo-core autonomy -- --nocapture`
Expected: PASS

Run: `cargo check`
Expected: compiles clean

**Step 4: Commit**

```bash
git add crates/meepo-core/src/autonomy/ crates/meepo-core/src/lib.rs
git commit -m "feat: add autonomy module with AutonomousLoop skeleton"
```

---

### Task 5: Wire Autonomous Loop into Main Daemon

**Files:**
- Modify: `crates/meepo-cli/src/main.rs`

This is the biggest integration task. We replace the `tokio::select!` main loop with `AutonomousLoop::run()`. The key insight: the bus receiver feeds into the loop's message_rx, and the loop's response_tx feeds back to the bus sender.

**Step 1: Add a forwarding task from bus → loop input channel**

In `cmd_start()`, after the bus is split (line ~518), replace everything from the `// Semaphore to limit concurrent message processing` comment (line 520) through the main_loop spawn (ending at line ~663) with the new autonomous loop wiring:

```rust
    // ── Autonomous Loop ─────────────────────────────────────────
    let (loop_msg_tx, loop_msg_rx) = tokio::sync::mpsc::channel::<meepo_core::types::IncomingMessage>(256);
    let (loop_resp_tx, mut loop_resp_rx) = tokio::sync::mpsc::channel::<meepo_core::types::OutgoingMessage>(256);
    let wake = meepo_core::autonomy::AutonomousLoop::create_wake_handle();

    // Forward incoming bus messages to the autonomous loop
    let wake_clone = wake.clone();
    let cancel_clone = cancel.clone();
    let bus_to_loop = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => break,
                msg = incoming_rx.recv() => {
                    match msg {
                        Some(incoming) => {
                            info!("Message from {} via {}: {}",
                                incoming.sender,
                                incoming.channel,
                                &incoming.content[..incoming.content.len().min(100)]);
                            if loop_msg_tx.send(incoming).await.is_err() {
                                break;
                            }
                            wake_clone.notify_one();
                        }
                        None => break,
                    }
                }
            }
        }
    });

    // Forward watcher events to the autonomous loop (replaces the old select! branch)
    let (loop_watcher_tx, loop_watcher_rx) = tokio::sync::mpsc::unbounded_channel();
    let cancel_clone2 = cancel.clone();
    let wake_clone2 = wake.clone();
    let watcher_to_loop = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone2.cancelled() => break,
                event = watcher_event_rx.recv() => {
                    match event {
                        Some(ev) => {
                            info!("Watcher event: {} from {}", ev.kind, ev.watcher_id);
                            let _ = loop_watcher_tx.send(ev);
                            wake_clone2.notify_one();
                        }
                        None => break,
                    }
                }
            }
        }
    });

    // Forward loop responses to the bus sender
    let cancel_clone3 = cancel.clone();
    let resp_to_bus = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone3.cancelled() => break,
                resp = loop_resp_rx.recv() => {
                    match resp {
                        Some(msg) => {
                            let channel = msg.channel.clone();
                            if let Err(e) = bus_sender.send(msg).await {
                                // Internal channel has no handler — this is expected
                                if channel != meepo_core::types::ChannelType::Internal {
                                    error!("Failed to route response to {}: {}", channel, e);
                                }
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    // Handle watcher commands (keep this — it's independent of the loop)
    let cancel_clone4 = cancel.clone();
    let watcher_cmd_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone4.cancelled() => break,
                cmd = watcher_command_rx.recv() => {
                    if let Some(command) = cmd {
                        let runner = watcher_runner_clone.clone();
                        let sched_db = sched_db.clone();
                        tokio::spawn(async move {
                            use meepo_core::tools::watchers::WatcherCommand;
                            match command {
                                WatcherCommand::Create { id, kind: _, config, action, reply_channel } => {
                                    let watcher_kind = match serde_json::from_value(config) {
                                        Ok(k) => k,
                                        Err(e) => {
                                            error!("Failed to deserialize watcher kind: {}", e);
                                            return;
                                        }
                                    };
                                    let watcher = meepo_scheduler::watcher::Watcher {
                                        id,
                                        kind: watcher_kind,
                                        action,
                                        reply_channel,
                                        active: true,
                                        created_at: chrono::Utc::now(),
                                    };
                                    if let Ok(conn) = sched_db.lock() {
                                        if let Err(e) = meepo_scheduler::persistence::save_watcher(&conn, &watcher) {
                                            error!("Failed to persist watcher {}: {}", watcher.id, e);
                                        }
                                    }
                                    if let Err(e) = runner.lock().await.start_watcher(watcher).await {
                                        error!("Failed to start watcher: {}", e);
                                    }
                                }
                                WatcherCommand::Cancel { id } => {
                                    if let Ok(conn) = sched_db.lock() {
                                        if let Err(e) = meepo_scheduler::persistence::deactivate_watcher(&conn, &id) {
                                            error!("Failed to deactivate watcher {} in scheduler DB: {}", id, e);
                                        }
                                    }
                                    if let Err(e) = runner.lock().await.stop_watcher(&id).await {
                                        error!("Failed to stop watcher {}: {}", id, e);
                                    }
                                }
                                WatcherCommand::List => {}
                            }
                        });
                    }
                }
            }
        }
    });

    // Handle sub-agent progress (keep this too)
    let cancel_clone5 = cancel.clone();
    let progress_bus = bus_sender_for_progress.clone(); // need a second Arc<BusSender> — see step 2
    let progress_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone5.cancelled() => break,
                progress = progress_rx.recv() => {
                    if let Some(msg) = progress {
                        info!("Sub-agent progress for {}: {}", msg.channel, &msg.content[..msg.content.len().min(100)]);
                        let _ = progress_bus.send(msg).await;
                    }
                }
            }
        }
    });

    // Start the autonomous loop
    let autonomy_config = meepo_core::autonomy::AutonomyConfig {
        enabled: cfg.autonomy.enabled,
        tick_interval_secs: cfg.autonomy.tick_interval_secs,
        max_goals: cfg.autonomy.max_goals,
    };

    let auto_loop = meepo_core::autonomy::AutonomousLoop::new(
        agent,
        db.clone(),
        autonomy_config,
        loop_msg_rx,
        loop_watcher_rx,
        loop_resp_tx,
        wake,
    );

    let cancel_clone6 = cancel.clone();
    let loop_task = tokio::spawn(async move {
        auto_loop.run(cancel_clone6).await;
    });
```

**Step 2: Fix the bus_sender sharing**

The current code has one `Arc<BusSender>`. We now need it in two places: the response forwarder and the progress forwarder. Before the forwarding tasks, clone it:

```rust
    let bus_sender_for_progress = bus_sender.clone();
```

This replaces the single `bus_sender` usage. The `bus_sender` (owned) goes into `resp_to_bus`, and `bus_sender_for_progress` (cloned) goes into `progress_task`.

**Step 3: Update the shutdown sequence**

Replace the current shutdown block (lines ~665-677) with:

```rust
    // Wait for shutdown signal
    signal::ctrl_c().await?;
    info!("Received Ctrl+C, shutting down...");
    cancel.cancel();

    // Wait for all tasks
    let _ = tokio::join!(loop_task, bus_to_loop, watcher_to_loop, resp_to_bus, watcher_cmd_task, progress_task);

    // Stop all watchers
    watcher_runner.lock().await.stop_all().await;

    println!("Meepo stopped.");
    Ok(())
```

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: compiles clean (warnings OK)

**Step 5: Commit**

```bash
git add crates/meepo-cli/src/main.rs
git commit -m "feat: wire autonomous loop into daemon, replacing reactive select loop"
```

---

### Task 6: Run Full Test Suite and Verify

**Step 1: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass (same as before plus new autonomy tests)

**Step 2: Build release binary**

Run: `cargo build --release`
Expected: compiles successfully

**Step 3: Smoke test**

Run: `./target/release/meepo ask "Say hello"`
Expected: Agent responds (this path doesn't use the autonomous loop)

**Step 4: Commit if any fixes were needed**

```bash
git add -A && git commit -m "fix: address test/build issues from autonomous loop integration"
```

(Only if fixes were needed — skip if all clean.)

---

### Summary of Changes

| File | Change |
|------|--------|
| `crates/meepo-knowledge/src/sqlite.rs` | +3 tables (goals, user_preferences, action_log), +Goal/UserPreference/ActionLogEntry structs, +CRUD methods, +tests |
| `crates/meepo-knowledge/src/lib.rs` | +re-exports for new types |
| `crates/meepo-core/src/autonomy/mod.rs` | NEW — AutonomousLoop struct with tick-based run(), drain_inputs(), message/event handlers |
| `crates/meepo-core/src/autonomy/goals.rs` | NEW — placeholder for Step 2 |
| `crates/meepo-core/src/autonomy/user_model.rs` | NEW — placeholder for Step 4 |
| `crates/meepo-core/src/autonomy/action_log.rs` | NEW — placeholder for Step 2 |
| `crates/meepo-core/src/autonomy/planner.rs` | NEW — placeholder for Step 2 |
| `crates/meepo-core/src/lib.rs` | +autonomy module, +re-exports |
| `crates/meepo-cli/src/config.rs` | +AutonomyConfig struct, +field on MeepoConfig |
| `config/default.toml` | +[autonomy] section |
| `crates/meepo-cli/src/main.rs` | Replace select! loop with forwarding tasks + AutonomousLoop::run() |
