# Meepo Architecture

## Overview

Meepo is a 5-crate Rust workspace implementing a local AI agent for macOS. It connects Claude to messaging channels (Discord, Slack, iMessage), gives it access to 20 tools, and maintains a persistent knowledge graph.

## Crate Dependency Graph

```mermaid
graph TD
    CLI[meepo-cli] --> CORE[meepo-core]
    CLI --> CHANNELS[meepo-channels]
    CLI --> KNOWLEDGE[meepo-knowledge]
    CLI --> SCHEDULER[meepo-scheduler]
    CHANNELS --> CORE
    CORE --> KNOWLEDGE
    SCHEDULER --> KNOWLEDGE
```

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `meepo-cli` | Binary entry point, config, subcommands | `Cli`, `MeepoConfig` |
| `meepo-core` | Agent loop, API client, tool system | `Agent`, `ApiClient`, `ToolRegistry` |
| `meepo-channels` | Channel adapters and message routing | `MessageBus`, `MessageChannel` |
| `meepo-knowledge` | SQLite + Tantivy persistence | `KnowledgeDb`, `KnowledgeGraph`, `TantivyIndex` |
| `meepo-scheduler` | Watcher runner and event system | `WatcherRunner`, `Watcher`, `WatcherEvent` |

## Message Flow

```mermaid
sequenceDiagram
    participant User
    participant Channel as Channel Adapter
    participant Bus as MessageBus
    participant Agent
    participant Claude as Claude API
    participant Tools as Tool Registry

    User->>Channel: Send message
    Channel->>Bus: IncomingMessage
    Bus->>Agent: handle_message()
    Agent->>Agent: Store in conversation history
    Agent->>Agent: Load context (history + knowledge)
    Agent->>Claude: API request (message + system prompt + tools)

    loop Tool Use Loop (max 10 iterations)
        Claude-->>Agent: tool_use response
        Agent->>Tools: execute(tool_name, input)
        Tools-->>Agent: tool result
        Agent->>Claude: tool_result + continue
    end

    Claude-->>Agent: Final text response
    Agent->>Agent: Store response in history
    Agent->>Bus: OutgoingMessage
    Bus->>Channel: Route to correct channel
    Channel->>User: Deliver response
```

## System Architecture

```mermaid
graph TB
    subgraph CLI["meepo-cli (Binary)"]
        Config[Config Loader]
        Init[Init Command]
        Start[Start Command]
        Ask[Ask Command]
    end

    subgraph Core["meepo-core"]
        Agent[Agent]
        API[ApiClient]
        ToolReg[ToolRegistry]

        subgraph ToolSystem["Tools (20)"]
            MacOS[macOS Tools]
            A11y[Accessibility Tools]
            Code[Code Tools]
            Mem[Memory Tools]
            Sys[System Tools]
        end
    end

    subgraph Channels["meepo-channels"]
        Bus[MessageBus]
        BusSender[BusSender]
        Discord[DiscordChannel]
        Slack[SlackChannel]
        IMsg[IMessageChannel]
    end

    subgraph Knowledge["meepo-knowledge"]
        DB[(SQLite)]
        Tantivy[(Tantivy Index)]
        Graph[KnowledgeGraph]
        MemSync[Memory Sync]
    end

    subgraph Scheduler["meepo-scheduler"]
        Runner[WatcherRunner]
        Persist[Persistence]
        Watchers["Watchers (7 types)"]
    end

    Start --> Agent
    Start --> Bus
    Start --> Runner
    Ask --> API

    Agent --> API
    Agent --> ToolReg
    ToolReg --> ToolSystem

    Bus --> Discord
    Bus --> Slack
    Bus --> IMsg
    Bus --> BusSender

    Mem --> Graph
    Graph --> DB
    Graph --> Tantivy
    MemSync --> DB

    Runner --> Watchers
    Runner --> Persist
    Persist --> DB

    MacOS -->|AppleScript| Mail[Mail.app]
    MacOS -->|AppleScript| Cal[Calendar.app]
    IMsg -->|SQLite| MsgDB[Messages DB]
    IMsg -->|AppleScript| MsgApp[Messages.app]
    Discord -->|WebSocket| DiscordAPI[Discord API]
    Slack -->|HTTP Polling| SlackAPI[Slack Web API]
```

## Event Loop

The main event loop runs in `cmd_start()` using `tokio::select!` across three sources:

```mermaid
graph LR
    subgraph Sources
        RX[incoming_rx.recv]
        WE[watcher_event_rx.recv]
        SIG[Ctrl+C Signal]
    end

    subgraph Select["tokio::select!"]
        RX -->|IncomingMessage| Spawn1[Spawn Task]
        WE -->|WatcherEvent| Spawn2[Spawn Task]
        SIG -->|CancellationToken| Shutdown[Shutdown]
    end

    Spawn1 --> Agent[agent.handle_message]
    Agent --> Send[bus_sender.send]

    Spawn2 --> AgentW[agent.handle_message]
```

The bus is split into a receiver (`mpsc::Receiver<IncomingMessage>`) and an `Arc<BusSender>` to allow concurrent send/receive without borrow conflicts.

## Tool System

Tools implement the `ToolHandler` trait and are registered in a `ToolRegistry` (HashMap-backed). The agent's API client runs a tool loop that executes tools until Claude returns a final text response or hits the 10-iteration limit.

```mermaid
graph TD
    subgraph ToolHandler["ToolHandler Trait"]
        Name["name() -> &str"]
        Desc["description() -> &str"]
        Schema["input_schema() -> Value"]
        Exec["execute(input) -> Result<String>"]
    end

    subgraph Registry["ToolRegistry"]
        HashMap["HashMap<String, Arc<dyn ToolHandler>>"]
    end

    subgraph Categories
        M["macOS (6)"]
        A["Accessibility (3)"]
        C["Code (3)"]
        K["Memory (4)"]
        S["System (4)"]
    end

    Categories --> Registry
    Registry --> |"list_tools()"| API[ApiClient]
    API --> |"tool_use"| Registry
    Registry --> |"execute()"| Result[Tool Result]
    Result --> API
```

### Tool List

| Tool | Description | Implementation |
|------|-------------|----------------|
| `read_emails` | Read recent emails from Mail.app | AppleScript via `osascript` |
| `read_calendar` | Read upcoming calendar events | AppleScript via `osascript` |
| `send_email` | Send email via Mail.app | AppleScript (sanitized input) |
| `create_event` | Create calendar event | AppleScript (sanitized input) |
| `open_app` | Open macOS application | `open -a` command |
| `get_clipboard` | Read clipboard contents | `pbpaste` command |
| `read_screen` | Read focused app/window info | AppleScript accessibility |
| `click_element` | Click UI element by name | AppleScript accessibility |
| `type_text` | Type text into focused app | AppleScript keystroke |
| `write_code` | Delegate coding to Claude CLI | `claude` CLI subprocess |
| `make_pr` | Create GitHub pull request | `git` + `gh` CLI |
| `review_pr` | Analyze PR diff for issues | `gh pr view` + diff analysis |
| `remember` | Store entity in knowledge graph | SQLite + Tantivy insert |
| `recall` | Search entities by name/type | SQLite query |
| `search_knowledge` | Full-text search knowledge graph | Tantivy search |
| `link_entities` | Create relationship between entities | SQLite insert |
| `run_command` | Execute shell command (allowlisted) | `sh -c` with 30s timeout |
| `read_file` | Read file contents | `tokio::fs::read_to_string` |
| `write_file` | Write file contents | `tokio::fs::write` |
| `browse_url` | Fetch URL content | `reqwest` with SSRF protection |

## Knowledge Graph

```mermaid
erDiagram
    ENTITIES {
        string id PK
        string name
        string entity_type
        string metadata
        datetime created_at
    }
    RELATIONSHIPS {
        string id PK
        string source_id FK
        string target_id FK
        string relationship_type
        string metadata
        datetime created_at
    }
    CONVERSATIONS {
        integer id PK
        string channel
        string sender
        string content
        string metadata
        datetime created_at
    }
    WATCHERS {
        string id PK
        string kind_json
        string action
        string reply_channel
        boolean active
        datetime created_at
    }
    WATCHER_EVENTS {
        integer id PK
        string watcher_id FK
        string kind
        string payload
        datetime created_at
    }

    ENTITIES ||--o{ RELATIONSHIPS : "source"
    ENTITIES ||--o{ RELATIONSHIPS : "target"
    WATCHERS ||--o{ WATCHER_EVENTS : "emits"
```

The knowledge layer has two backends:
- **SQLite** (`KnowledgeDb`) — Stores entities, relationships, conversations, and watchers with indexed queries
- **Tantivy** (`TantivyIndex`) — Full-text search index over entity content, returning relevance-ranked results

`KnowledgeGraph` combines both, indexing entities in Tantivy on insert and delegating searches to the appropriate backend.

## Watcher System

```mermaid
graph TD
    subgraph WatcherKind["7 Watcher Types"]
        Email[EmailWatch]
        Calendar[CalendarWatch]
        GitHub[GitHubWatch]
        File[FileWatch]
        Message[MessageWatch]
        Scheduled[Scheduled / Cron]
        OneShot[OneShot]
    end

    subgraph Runner["WatcherRunner"]
        ActiveTasks["active_tasks: RwLock<HashMap>"]
        Cancel["CancellationToken per watcher"]
    end

    subgraph Execution
        Polling["Polling Loop"]
        PollState["PollState (dedup)"]
        Notify["notify::Watcher"]
        Cron["cron::Schedule"]
    end

    Email --> Polling
    Calendar --> Polling
    GitHub --> Polling
    Polling --> PollState

    File --> Notify
    Message --> Polling
    Scheduled --> Cron
    OneShot --> |"tokio::time::sleep_until"| Once[Execute Once]

    Polling --> |"WatcherEvent"| EventTX[mpsc channel]
    Notify --> EventTX
    Cron --> EventTX
    Once --> EventTX
    EventTX --> Agent[Agent handles event]
```

Watchers run as independent tokio tasks managed by `WatcherRunner`. Each has a `CancellationToken` for graceful shutdown. Polling watchers use `PollState` with `HashSet<u64>` for deduplication across cycles.

## Channel Adapters

```mermaid
graph TB
    subgraph MessageChannel["MessageChannel Trait"]
        Start["start(tx) -> Result"]
        Send["send(msg) -> Result"]
        Type["channel_type() -> ChannelType"]
    end

    subgraph Discord
        Serenity[Serenity Client]
        DHandler[EventHandler]
        LRU1["LRU<msg_id, channel_id>"]
    end

    subgraph Slack
        ReqwestS[reqwest Client]
        PollS[Polling Task]
        DashMapS["DashMap<user_id, channel_id>"]
    end

    subgraph IMessage
        SQLiteI[SQLite Read-Only]
        PollI[Polling Task]
        LRU2["LRU<msg_id, sender>"]
        AppleScript[osascript Send]
    end

    Discord --> |WebSocket| DiscordAPI[Discord Gateway]
    Slack --> |HTTP| SlackAPI[Slack Web API]
    IMessage --> |File| ChatDB["~/Library/Messages/chat.db"]
    IMessage --> |AppleScript| Messages[Messages.app]
```

| Channel | Connection | Receive | Send | Reply Tracking |
|---------|-----------|---------|------|----------------|
| Discord | WebSocket via Serenity | EventHandler callback | HTTP via `channel_id.say()` | LRU cache (1000 entries) |
| Slack | HTTP polling (configurable interval) | `conversations.history` | `chat.postMessage` | DashMap user->channel |
| iMessage | SQLite polling of chat.db | Read-only query by ROWID | AppleScript `send` command | LRU cache (1000 entries) |

## Security Model

```mermaid
graph LR
    subgraph Input["Input Validation"]
        CMD["Command Allowlist (57 safe commands)"]
        PATH["Path Traversal Protection"]
        SSRF["SSRF Blocking (private IPs)"]
        AS["AppleScript Sanitization"]
        CRLF["HTTP Header CRLF Check"]
    end

    subgraph Limits["Resource Limits"]
        TIMEOUT["30s Execution Timeout"]
        FILESIZE["10MB File Size Cap"]
        CMDLEN["1000 char Command Limit"]
        MAXITER["10 Tool Loop Iterations"]
    end

    subgraph Access["Access Control"]
        DISCORD_ACL["Discord User Allowlist"]
        IMSG_ACL["iMessage Contact Allowlist"]
        TRIGGER["iMessage Trigger Prefix"]
    end

    UserInput --> Input
    Input --> Limits
    Access --> Channel[Channel Adapters]
```
