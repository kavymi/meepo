# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- MCP server support — expose Meepo's tools via STDIO for Claude Desktop, Cursor, etc. (`meepo mcp-server`)
- MCP client — connect to external MCP servers and import their tools
- A2A (Agent-to-Agent) protocol — HTTP server and client for multi-agent task delegation
- Autonomous agent loop — observe/think/act cycle with goal tracking and proactive actions
- Proactive notification service — iMessage/Discord/Slack alerts for task progress, watcher triggers, errors
- Agent template system — swap personalities, goals, and config overlays (`meepo template use`)
- Skills system — import OpenClaw-compatible SKILL.md files as additional tools
- Platform abstraction layer — trait-based OS abstraction for all system integrations
- New tools: `list_reminders`, `create_reminder`, `list_notes`, `create_note`, `search_contacts`, `get_current_track`, `music_control`, `send_notification`, `screen_capture`, `list_directory`, `search_files`, `spawn_background_task`, `agent_status`, `stop_task`, `spawn_coding_agent`
- Browser automation tools: `browser_list_tabs`, `browser_open_tab`, `browser_close_tab`, `browser_switch_tab`, `browser_get_page_content`, `browser_execute_js`, `browser_click`, `browser_fill_form`, `browser_navigate`, `browser_get_url`, `browser_screenshot`
- Email channel — poll Mail.app for incoming emails with subject prefix filtering
- Filesystem access tools with configurable allowed directories and sandboxing
- Quiet hours support for notifications
- Daily digest notifications (morning briefing, evening recap)

## [0.1.1] - 2026-02-09

### Added
- Homebrew formula (`brew install leancoderkavy/tap/meepo`)
- GitHub Actions release workflow with cross-compilation
- Install scripts for macOS/Linux (curl) and Windows (PowerShell)

## [0.1.0] - 2026-02-08

### Added
- Initial release
- Agent loop with Claude API integration and tool use
- 25 built-in tools: email, calendar, clipboard, app launching, UI automation, code (write_code, make_pr, review_pr), web search (Tavily), browse URL, knowledge graph (remember, recall, search, link), system (run_command, read_file, write_file), watchers (create, list, cancel), delegation (delegate_tasks)
- Channel adapters: Discord (WebSocket), Slack (HTTP polling), iMessage (SQLite + AppleScript)
- Knowledge graph with SQLite + Tantivy full-text search
- Watcher system with 7 types: email, calendar, GitHub, file, message, scheduled/cron, one-shot
- Sub-agent orchestrator with parallel and background execution modes
- Cross-platform support: macOS (AppleScript) and Windows (PowerShell/COM)
- Security: command allowlists, path traversal protection, SSRF blocking, input sanitization, execution timeouts
- Configuration via TOML with environment variable expansion
- Background service support: launchd (macOS) and Task Scheduler (Windows)

[Unreleased]: https://github.com/leancoderkavy/meepo/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/leancoderkavy/meepo/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/leancoderkavy/meepo/releases/tag/v0.1.0
