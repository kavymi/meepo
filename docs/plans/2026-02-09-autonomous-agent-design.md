# Autonomous Agent Architecture

## Overview

Replace Meepo's reactive message handler with a continuous autonomous loop that pursues goals, learns user preferences, and takes proactive action. The agent runs continuously, processing user messages as one input among many — alongside watcher events, goal deadlines, and self-generated tasks.

## Core Loop: Observe / Think / Act / Reflect

The autonomous loop runs in a tokio task, ticking every ~30s when idle and immediately when inputs arrive.

```
┌─────────────────────────────────────────────┐
│              AUTONOMOUS LOOP                │
│                                             │
│  1. OBSERVE  — drain pending inputs:        │
│     • user messages (from channels)         │
│     • watcher events                        │
│     • completed background tasks            │
│     • goals due for evaluation              │
│                                             │
│  2. THINK   — single Claude API call:       │
│     • situation report (inputs + due goals  │
│       + recent outcomes + preferences)      │
│     • returns structured JSON decisions     │
│                                             │
│  3. ACT     — execute decisions:            │
│     • respond to user on their channel      │
│     • run tools (send email, create PR...)  │
│     • create/update/complete goals          │
│     • store learned preferences             │
│                                             │
│  4. REFLECT — log outcomes:                 │
│     • record actions + results              │
│     • update preference confidence scores   │
│     • adjust goal strategies                │
│                                             │
└─────────────────────────────────────────────┘
```

**Token efficiency:** Skip the API call entirely when there are zero inputs AND zero goals due for checking. Situation reports are kept compact — goal summaries, not full histories. A quiet tick is free.

**Latency:** When a user message arrives, it triggers an immediate tick. Response time is comparable to the current reactive flow.

## Data Model

Three new SQLite tables in knowledge.db alongside existing entities/relationships/conversations.

### Goals

```sql
CREATE TABLE goals (
    id INTEGER PRIMARY KEY,
    description TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',  -- active|paused|completed|failed
    priority INTEGER NOT NULL DEFAULT 3,    -- 1 (low) to 5 (critical)
    success_criteria TEXT,
    strategy TEXT,                          -- current plan, updated by agent
    check_interval_secs INTEGER NOT NULL,  -- how often to re-evaluate
    last_checked_at TEXT,                   -- ISO 8601
    source_channel TEXT,                   -- channel where user set this
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### User Preferences

```sql
CREATE TABLE user_preferences (
    id INTEGER PRIMARY KEY,
    category TEXT NOT NULL,     -- communication|schedule|code|workflow
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,        -- JSON
    confidence REAL NOT NULL DEFAULT 0.3,
    learned_from TEXT,          -- what interaction taught this
    last_confirmed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### Action Log

```sql
CREATE TABLE action_log (
    id INTEGER PRIMARY KEY,
    goal_id INTEGER,           -- nullable, not all actions are goal-driven
    action_type TEXT NOT NULL,
    description TEXT NOT NULL,
    outcome TEXT NOT NULL DEFAULT 'pending',  -- success|failed|pending|unknown
    user_feedback TEXT,        -- nullable, did user approve/correct?
    created_at TEXT NOT NULL,
    FOREIGN KEY (goal_id) REFERENCES goals(id)
);
```

## Goal Lifecycle

Goals come from three sources:

**Explicit** — user tells the agent: "remind me to review PRs every morning." Agent creates goal with check_interval, strategy, and success criteria.

**Inferred** — agent observes repeated user behavior (e.g., asking for calendar summary 3+ times in a week). Creates a preference first (low confidence), then promotes to a standing goal once confidence exceeds 0.7.

**Self-generated** — agent notices a watcher event that needs follow-up (e.g., PR assigned to user, no response in 2 hours) and creates a short-lived goal to ensure user awareness.

Lifecycle: `active → completed | failed → archived`. Standing goals (e.g., "keep inbox clean") never complete — they're checked continuously. Time-bound goals (e.g., "ship feature by Friday") complete or fail.

Agent can also pause goals (user on vacation) and split goals (too large, decompose into sub-goals).

## Preference Learning

Passive observation — agent does not interrogate the user.

- **Schedule patterns**: active hours, channel preferences, daily routines
- **Communication style**: brevity preference, code examples, formality
- **Priorities**: which repos, contacts, topics matter most
- **Corrections**: "don't do that" → negative preference, high confidence

Confidence starts at 0.3, increases with repeated observations. Below 0.5: tentative use, agent may ask for confirmation. Above 0.7: used automatically. At 0.9+: high conviction.

**Decay**: unconfirmed preferences lose 0.05 confidence per 30 days. Prevents stale assumptions from driving behavior.

## Channel Routing

Each user has a `preferred_channel` in their user model — set to whatever channel they last interacted on. Proactive messages route there.

Fallback chain: preferred channel → next enabled channel → log and retry on next tick.

This fixes the current bug where watcher event responses go to `ChannelType::Internal` and never reach the user.

## Structured Decision Output

The Think phase returns structured JSON, not free-text:

```json
{
  "decisions": [
    {"type": "respond", "message": "Your PR got approved, merging now."},
    {"type": "tool_call", "tool": "run_command", "args": {"command": "gh pr merge 42"}},
    {"type": "update_goal", "goal_id": 3, "status": "completed"},
    {"type": "learn_preference", "category": "code", "key": "auto_merge_approved_prs", "value": true},
    {"type": "no_action", "reason": "nothing needs attention"}
  ]
}
```

Multiple decisions per tick are allowed — the agent can respond to a user AND advance a goal in the same cycle.

## Module Structure

```
meepo-core/src/autonomy/
  mod.rs          — AutonomousLoop struct, tick logic, input draining
  goals.rs        — Goal CRUD, due-for-check queries, lifecycle transitions
  user_model.rs   — Preference CRUD, confidence updates, decay
  action_log.rs   — Outcome logging, feedback recording
  planner.rs      — Situation report builder, decision parser
```

## Implementation Order

1. **Autonomous loop skeleton** — tick-based cycle replacing reactive handler. New SQLite tables. Foundation for everything else.

2. **Goal system** — explicit goal creation from user messages, evaluation on tick, tool execution. Agent becomes goal-driven.

3. **Channel routing fix** — proactive messages reach the user's preferred channel. Without this, autonomous actions are invisible.

4. **Preference learning** — passive observation, confidence scoring, decay. Agent starts adapting to user.

5. **Inferred goals** — promote high-confidence preferences to standing goals. Full autonomy emerges.

## Configuration

```toml
[autonomy]
enabled = true
tick_interval_secs = 30       # idle tick rate
max_goals = 50                # prevent runaway goal creation
preference_decay_days = 30    # confidence decay period
min_confidence_to_act = 0.5   # below this, ask user first
max_tokens_per_tick = 4096    # budget per think phase
```
