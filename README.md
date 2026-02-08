# Meepo

A local AI agent for macOS that connects Claude to your digital life through Discord, Slack, and iMessage.

Meepo runs as a daemon on your Mac, monitoring your configured channels for messages. When you message it, it processes your request using Claude's API with access to 20 tools spanning email, calendar, files, code, and a persistent knowledge graph.

## Features

- **Multi-channel messaging** — Discord DMs, Slack DMs, iMessage, or CLI one-shots
- **20 built-in tools** — Read/send emails, manage calendar events, run commands, browse URLs, read/write files, manage code PRs, and more
- **Knowledge graph** — Remembers entities, relationships, and conversations across sessions with Tantivy full-text search
- **Scheduled watchers** — Monitor email, calendar, GitHub events, files, or run cron tasks
- **macOS native** — Uses AppleScript for Mail.app, Calendar.app, and Messages integration
- **Security hardened** — Command allowlists, path traversal protection, SSRF blocking, AppleScript input sanitization, 30s execution timeouts

## Requirements

- macOS (for AppleScript integrations)
- Rust toolchain (`rustup`)
- Anthropic API key
- Optional: Discord bot token, Slack bot token

## Quick Start

```bash
# Build
cargo build --release

# Initialize config
./target/release/meepo init

# Set your API key
export ANTHROPIC_API_KEY="sk-ant-..."

# Edit config
nano ~/.meepo/config.toml

# Start the agent
./target/release/meepo start

# Or ask a one-shot question (no daemon needed)
./target/release/meepo ask "What's on my calendar today?"
```

## Setup Guide

### 1. Build

```bash
git clone https://github.com/kavymi/meepo.git
cd meepo
cargo build --release
```

The binary is at `target/release/meepo` (~19MB).

### 2. Initialize

```bash
meepo init
```

This creates `~/.meepo/` with:
- `config.toml` — Main configuration
- `workspace/SOUL.md` — Agent personality (editable)
- `workspace/MEMORY.md` — Persistent memory (auto-updated)

### 3. Configure API Key

Set your Anthropic API key as an environment variable:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

Or hardcode it in `~/.meepo/config.toml` (not recommended):

```toml
[providers.anthropic]
api_key = "sk-ant-..."
```

### 4. Enable Channels

Edit `~/.meepo/config.toml` to enable the channels you want:

#### Discord

```toml
[channels.discord]
enabled = true
token = "${DISCORD_BOT_TOKEN}"
allowed_users = ["123456789012345678"]  # Your Discord user ID
```

Requires a Discord bot with `MESSAGE_CONTENT` and `DIRECT_MESSAGES` intents enabled. Create one at the [Discord Developer Portal](https://discord.com/developers/applications).

#### Slack

```toml
[channels.slack]
enabled = true
bot_token = "${SLACK_BOT_TOKEN}"
poll_interval_secs = 3
```

Requires a Slack app with `chat:write`, `channels:read`, and `im:history` scopes. Create one at [api.slack.com/apps](https://api.slack.com/apps).

#### iMessage

```toml
[channels.imessage]
enabled = true
trigger_prefix = "/d"
allowed_contacts = ["+15551234567", "user@icloud.com"]
poll_interval_secs = 3
```

No API key needed. Requires macOS with **Full Disk Access** granted to your terminal (System Settings > Privacy & Security > Full Disk Access).

Messages must start with the trigger prefix (default `/d`). Example: `/d What's on my calendar?`

### 5. Run

```bash
# Start the daemon (Ctrl+C to stop)
meepo start

# With debug logging
meepo --debug start

# Stop a backgrounded daemon
meepo stop
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `meepo init` | Create `~/.meepo/` with default config |
| `meepo start` | Start the agent daemon |
| `meepo stop` | Stop a running daemon |
| `meepo ask "..."` | One-shot question (no daemon needed) |
| `meepo config` | Show loaded configuration |
| `meepo --debug <cmd>` | Enable debug logging |
| `meepo --config <path> <cmd>` | Use custom config file |

## Configuration Reference

Full config file: `~/.meepo/config.toml`

```toml
[agent]
default_model = "claude-opus-4-6"     # Claude model to use
max_tokens = 8192                      # Max response tokens

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"       # Required
base_url = "https://api.anthropic.com" # API endpoint

[channels.discord]
enabled = false
token = "${DISCORD_BOT_TOKEN}"
allowed_users = []                     # Discord user IDs (strings)

[channels.slack]
enabled = false
bot_token = "${SLACK_BOT_TOKEN}"
poll_interval_secs = 3                 # How often to check for messages

[channels.imessage]
enabled = false
trigger_prefix = "/d"                  # Messages must start with this
allowed_contacts = []                  # Phone numbers or emails
poll_interval_secs = 3

[knowledge]
db_path = "~/.meepo/knowledge.db"
tantivy_path = "~/.meepo/tantivy_index"

[watchers]
max_concurrent = 50
min_poll_interval_secs = 30
active_hours = { start = "08:00", end = "23:00" }

[code]
claude_code_path = "claude"            # Path to Claude CLI
gh_path = "gh"                         # Path to GitHub CLI
default_workspace = "~/Coding"

[memory]
workspace = "~/.meepo/workspace"       # Contains SOUL.md and MEMORY.md
```

Environment variables are expanded with `${VAR_NAME}` syntax. Paths support `~/` expansion.

## Tools

Meepo registers 20 tools that Claude can use during conversations:

| Category | Tools |
|----------|-------|
| **macOS** | `read_emails`, `send_email`, `read_calendar`, `create_event`, `open_app`, `get_clipboard` |
| **Accessibility** | `read_screen`, `click_element`, `type_text` |
| **Code** | `write_code`, `make_pr`, `review_pr` |
| **Memory** | `remember`, `recall`, `search_knowledge`, `link_entities` |
| **System** | `run_command`, `read_file`, `write_file`, `browse_url` |

## Architecture

See [docs/architecture.md](docs/architecture.md) for detailed architecture documentation with diagrams.

## Project Structure

```
meepo/
├── crates/
│   ├── meepo-core/       # Agent loop, API client, tool system
│   ├── meepo-channels/   # Discord, Slack, iMessage adapters + message bus
│   ├── meepo-knowledge/  # SQLite + Tantivy knowledge graph
│   ├── meepo-scheduler/  # Watcher runner, persistence, polling
│   └── meepo-cli/        # CLI binary, config loading
├── config/
│   └── default.toml      # Default configuration template
├── SOUL.md               # Agent personality template
└── MEMORY.md             # Agent memory template
```

## License

MIT
