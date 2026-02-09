# Sub-Agent System Design

**Date**: 2026-02-08
**Status**: Draft

## Problem

Meepo currently runs a single agent loop per request — one `run_tool_loop` with a 10-iteration cap, no parallelism, no background execution. This limits it to simple request-response interactions and blocks entire categories of use cases that competitors like OpenClaw handle via sub-agent delegation.

## Goals

- Enable parallel task decomposition for multi-part requests (e.g. "plan my weekend")
- Enable long-running background tasks that notify on completion (e.g. "research competitors and write a report")
- Keep API costs predictable with scoped contexts and configurable limits
- Maintain the simplicity of the existing architecture

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Context model | Scoped + summary | Sub-agents get a focused prompt with a context summary from the parent. No full conversation history. Balances cost and coherence. |
| Tool access | Scoped by parent | Parent agent decides which tools each sub-agent can use. Safer and cheaper. |
| Nesting | Single level only | Only the parent agent can spawn sub-agents. Predictable costs, easier to reason about. |
| Notification | Same channel + progress | Results go to originating channel with progress updates so the user knows work is happening. |

## Architecture

### New Components

All new code lives in `meepo-core` — no new crates.

#### `meepo-core/src/orchestrator.rs` — Task Orchestrator

Contains the sub-agent execution machinery:

```rust
/// A single sub-task to be delegated
pub struct SubTask {
    pub task_id: String,
    pub prompt: String,
    pub context_summary: String,
    pub allowed_tools: Vec<String>,
}

/// Result from a completed sub-task
pub struct SubTaskResult {
    pub task_id: String,
    pub status: SubTaskStatus,
    pub output: String,
    pub tokens_used: Usage,
}

pub enum SubTaskStatus {
    Completed,
    Failed,
    TimedOut,
}

/// Tracks a group of sub-tasks
pub struct TaskGroup {
    pub group_id: String,
    pub mode: ExecutionMode,
    pub channel: ChannelType,
    pub reply_to: Option<String>,
    pub tasks: Vec<SubTask>,
    pub results: Vec<SubTaskResult>,
    pub created_at: DateTime<Utc>,
}

pub enum ExecutionMode {
    Parallel,
    Background,
}

/// Wraps a ToolRegistry but only allows specific tools
pub struct FilteredToolExecutor {
    inner: Arc<ToolRegistry>,
    allowed: HashSet<String>,
}
```

`FilteredToolExecutor` implements `ToolExecutor` and delegates to the real `ToolRegistry` but rejects calls to non-allowed tools.

#### `meepo-core/src/tools/delegate.rs` — The `delegate_tasks` Tool

A new `ToolHandler` that the parent agent calls to spawn sub-agents. Takes an array of sub-task definitions and a mode, constructs a `TaskGroup`, and hands it to the orchestrator.

### Modified Components

- **`ToolRegistry`** — New `filter_tools(&self, names: &[String]) -> Vec<ToolDefinition>` method for building scoped tool sets.
- **`Agent`** — Gets an `Arc<MessageBus>` handle so the orchestrator can send progress updates and results back to channels.

## Execution Model

### Parallel Mode

For quick multi-part requests:

1. Parent agent calls `delegate_tasks` with `mode: "parallel"`
2. All sub-tasks spawn concurrently via `tokio::spawn`
3. The `delegate_tasks` tool **blocks** — waits for all sub-tasks to complete (configurable timeout, default 120s)
4. Results are collected and returned as a single tool result back to the parent's tool loop
5. Parent agent synthesizes a final response from all sub-task results
6. One progress message sent to channel: "Working on N tasks..."

### Background Mode

For long-running work:

1. Parent agent calls `delegate_tasks` with `mode: "background"`
2. Sub-tasks spawn concurrently but the tool returns **immediately** with a task group ID
3. Parent agent confirms to user that work has started
4. Each sub-task runs independently; orchestrator sends per-task completion updates to the originating channel via `MessageBus`
5. When all sub-tasks finish, orchestrator sends a final combined summary
6. Failed sub-tasks generate immediate error notifications

### Sub-Agent Execution Detail

Each sub-task runs as an isolated agent loop:

1. **Build scoped tool set** — `registry.filter_tools(&task.allowed_tools)` creates a `FilteredToolExecutor` wrapper
2. **Build system prompt** — Lightweight, no SOUL.md/MEMORY.md:
   ```
   You are a focused sub-agent working on a specific task.

   ## Context
   {context_summary}

   ## Your Task
   {prompt}

   Respond with your findings/results directly. Be concise.
   ```
3. **Run the loop** — `api_client.run_tool_loop()` with scoped prompt, filtered tools, 10-iteration limit
4. **Timeout** — `tokio::time::timeout` wraps each sub-task (120s parallel, 600s background)
5. **Error isolation** — Panics and errors don't affect sibling sub-tasks

Sub-agents reuse the same `ApiClient` instance — `reqwest::Client` handles concurrent requests natively.

## Tool Interface

```json
{
  "name": "delegate_tasks",
  "description": "Delegate work to sub-agents that execute independently. Use 'parallel' mode when you need multiple results combined into one response. Use 'background' mode for long-running work the user can come back to later.",
  "input_schema": {
    "type": "object",
    "properties": {
      "mode": {
        "type": "string",
        "enum": ["parallel", "background"]
      },
      "tasks": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "task_id": { "type": "string" },
            "prompt": { "type": "string" },
            "context_summary": { "type": "string" },
            "tools": {
              "type": "array",
              "items": { "type": "string" }
            }
          },
          "required": ["task_id", "prompt", "tools"]
        }
      }
    },
    "required": ["mode", "tasks"]
  }
}
```

### Return Values

**Parallel mode** (blocks, returns combined results):
```
## Results

### search_events (completed)
Found 3 events this weekend: ...

### check_weather (completed)
Saturday: 72°F sunny. Sunday: 68°F partly cloudy.

### check_calendar (failed)
Error: Calendar access timed out
```

**Background mode** (returns immediately):
```
Started task group bg-a1b2c3 with 3 tasks. The user will be notified on the original channel as tasks complete.
```

## Progress Updates

- **Parallel mode**: One message on start ("Working on 4 tasks..."), then the final combined result. No per-task noise.
- **Background mode**: Message on start, per-task completion update, final summary when all done. Immediate notification on failures.

Progress messages are sent as `OutgoingMessage` through the existing `MessageBus`.

## Cost Controls

| Control | Default | Configurable |
|---------|---------|-------------|
| Max concurrent sub-tasks per group | 5 | Yes |
| Max total sub-tasks per request | 10 | Yes |
| Per-sub-task iteration limit | 10 | No (matches parent) |
| Per-sub-task timeout | 120s parallel / 600s background | Yes |
| Max concurrent background groups | 3 | Yes |

### Configuration

```toml
[orchestrator]
max_concurrent_subtasks = 5
max_subtasks_per_request = 10
parallel_timeout_secs = 120
background_timeout_secs = 600
max_background_groups = 3
```

## Scope Boundaries

**In scope (v1)**:
- `delegate_tasks` tool with parallel and background modes
- `FilteredToolExecutor` for scoped tool access
- `TaskOrchestrator` with progress updates via MessageBus
- Configuration and cost controls
- In-memory task group tracking

**Out of scope (future)**:
- Recursive sub-agent spawning (depth > 1)
- SQLite persistence for background tasks (survives daemon restart)
- Global token budget tracking per hour
- Sub-agent-to-sub-agent communication
- Per-sub-agent model selection (e.g. use Haiku for simple lookups)

## Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `crates/meepo-core/src/orchestrator.rs` | Create | TaskOrchestrator, TaskGroup, SubTask, FilteredToolExecutor |
| `crates/meepo-core/src/tools/delegate.rs` | Create | delegate_tasks ToolHandler |
| `crates/meepo-core/src/tools/mod.rs` | Modify | Add `pub mod delegate;` |
| `crates/meepo-core/src/lib.rs` | Modify | Add `pub mod orchestrator;` |
| `crates/meepo-core/src/agent.rs` | Modify | Add Arc<MessageBus> field, pass to orchestrator |
| `crates/meepo-cli/src/config.rs` | Modify | Add `[orchestrator]` config section |
| `crates/meepo-cli/src/main.rs` | Modify | Wire orchestrator config, pass MessageBus to Agent |
