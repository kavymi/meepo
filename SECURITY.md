# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability in Meepo, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, please report vulnerabilities by emailing the maintainers or by using [GitHub's private vulnerability reporting](https://github.com/leancoderkavy/meepo/security/advisories/new).

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### What to expect

- Acknowledgment within 48 hours
- A fix or mitigation plan within 7 days for critical issues
- Credit in the release notes (unless you prefer to remain anonymous)

## Security Model

Meepo runs locally on your machine with access to system resources. The security model includes:

- **Command allowlists** — Only pre-approved commands can be executed via `run_command`
- **Path traversal protection** — File access is restricted to home, working, and temp directories
- **SSRF blocking** — Private/internal IP addresses are blocked in URL fetching
- **AppleScript sanitization** — All user input is sanitized before passing to `osascript`
- **Input size limits** — 10MB file cap, 1000-char command limit, 50KB email body limit
- **Execution timeouts** — 30-second timeout on all command execution
- **Tool loop limits** — Maximum 10 tool iterations per request
- **A2A authentication** — Bearer token with constant-time comparison
- **A2A rate limiting** — Maximum 100 concurrent tasks, 1MB request body limit
- **MCP denylist** — `delegate_tasks` is never exposed via MCP
- **Channel access control** — Discord user allowlists, iMessage contact allowlists

## Secrets

API keys are stored as environment variables and referenced in config via `${VAR_NAME}` syntax. They are never logged or included in error messages. Structs holding secrets use custom `Debug` implementations that redact values.
