# Meepo — Unit Test Plan for 100% Coverage

**Current state:** ~694 test functions across 8 crates.
**Goal:** 100% unit test coverage of all public API surface, logic branches, error paths, and edge cases.

---

## Summary by Crate

| Crate | Files | Existing Tests | Estimated Tests Needed | Priority |
|-------|-------|---------------|----------------------|----------|
| `meepo-cli` | 3 | 7 (template only) | ~65 | P1 |
| `meepo-core` | 50+ | ~460 | ~180 | P1 |
| `meepo-channels` | 10 | ~50 | ~35 | P2 |
| `meepo-knowledge` | 7 | ~32 | ~55 | P1 |
| `meepo-scheduler` | 4 | ~18 | ~20 | P2 |
| `meepo-mcp` | 5 | ~22 | ~12 | P3 |
| `meepo-a2a` | 5 | ~10 | ~15 | P3 |
| `meepo-gateway` | 7 | ~77 | ~15 | P3 |

**Total estimated new tests: ~397**

---

## 1. `meepo-cli` (0 tests in config.rs and main.rs)

### 1.1 `config.rs` — **0 tests, 1308 lines, HIGH PRIORITY**

#### `mask_secret()`
- [ ] `test_mask_secret_empty` — returns `"(empty)"`
- [ ] `test_mask_secret_short` — ≤7 chars returns `"***"`
- [ ] `test_mask_secret_long` — >7 chars returns `"abc...wxyz"` pattern
- [ ] `test_mask_secret_exactly_8_chars` — boundary case
- [ ] `test_mask_secret_multibyte_utf8` — emoji/CJK chars don't panic

#### `expand_env_vars()`
- [ ] `test_expand_env_vars_allowed` — `${ANTHROPIC_API_KEY}` expands
- [ ] `test_expand_env_vars_disallowed` — `${PATH}` stays unexpanded
- [ ] `test_expand_env_vars_missing` — allowed var not set → empty string
- [ ] `test_expand_env_vars_multiple` — two vars in same string
- [ ] `test_expand_env_vars_nested` — `${${FOO}}` doesn't crash
- [ ] `test_expand_env_vars_no_closing_brace` — `${FOO` doesn't loop
- [ ] `test_expand_env_vars_no_vars` — plain string returned unchanged
- [ ] `test_expand_env_vars_adjacent` — `${HOME}${USER}` both expand
- [ ] `test_expand_env_vars_empty_name` — `${}` handled gracefully

#### `config_dir()`
- [ ] `test_config_dir_returns_dot_meepo` — ends with `.meepo`

#### `MeepoConfig::load()`
- [ ] `test_load_valid_config` — parse `config/default.toml` (with env vars stubbed)
- [ ] `test_load_missing_file` — returns descriptive error
- [ ] `test_load_invalid_toml` — returns parse error
- [ ] `test_load_permissions_check` — (Unix) rejects 0o644 permissions
- [ ] `test_load_hardcoded_key_warning` — warns on `sk-ant-` prefix

#### Default functions (one test per default fn to verify values)
- [ ] `test_defaults_agent` — `default_system_prompt_file`, `default_memory_file`
- [ ] `test_defaults_providers` — `default_base_url`, `default_openai_*`, `default_google_*`, `default_ollama_*`, `default_compat_max_tokens`
- [ ] `test_defaults_channels` — `default_poll_interval`, `default_email_poll_interval`, `default_subject_prefix`, `default_slack_poll_interval`, `default_alexa_poll_interval`
- [ ] `test_defaults_watchers` — `default_max_concurrent`, `default_min_poll`
- [ ] `test_defaults_code` — `default_coding_agent_path`, `default_gh_path`, `default_workspace`
- [ ] `test_defaults_filesystem` — `default_allowed_directories`
- [ ] `test_defaults_orchestrator` — all 5 orchestrator defaults
- [ ] `test_defaults_autonomy` — all 9 autonomy defaults
- [ ] `test_defaults_mcp` — `McpServerConfig::default()`
- [ ] `test_defaults_a2a` — `A2aConfig::default()`
- [ ] `test_defaults_skills` — `SkillsConfig::default()`
- [ ] `test_defaults_browser` — `BrowserConfig::default()`
- [ ] `test_defaults_gateway` — `GatewayConfig::default()`
- [ ] `test_defaults_voice` — `VoiceConfig::default()`
- [ ] `test_defaults_sandbox` — `SandboxCliConfig::default()`
- [ ] `test_defaults_secrets` — `SecretsCliConfig::default()`
- [ ] `test_defaults_guardrails` — `GuardrailsCliConfig::default()`
- [ ] `test_defaults_usage` — `UsageCliConfig::default()`
- [ ] `test_defaults_notifications` — `NotificationsConfig::default()`, `DigestConfig::default()`
- [ ] `test_defaults_agent_to_agent` — `AgentToAgentCliConfig::default()`
- [ ] `test_defaults_reminders` — `RemindersConfig::default()`
- [ ] `test_defaults_notes` — `NotesConfig::default()`
- [ ] `test_defaults_contacts` — `ContactsConfig::default()`
- [ ] `test_defaults_email` — `EmailConfig::default()`
- [ ] `test_defaults_alexa` — `AlexaConfig::default()`

#### Custom Debug impls (verify secrets are masked)
- [ ] `test_debug_anthropic_config_masks_key`
- [ ] `test_debug_openai_config_masks_key`
- [ ] `test_debug_google_config_masks_key`
- [ ] `test_debug_compat_config_masks_key`
- [ ] `test_debug_tavily_config_masks_key`
- [ ] `test_debug_discord_config_masks_token`
- [ ] `test_debug_slack_config_masks_token`
- [ ] `test_debug_a2a_config_masks_token`
- [ ] `test_debug_a2a_agent_entry_masks_token`
- [ ] `test_debug_gateway_config_masks_token`
- [ ] `test_debug_voice_config_masks_key`

#### Serde round-trip
- [ ] `test_serde_roundtrip_meepo_config` — serialize → deserialize preserves values
- [ ] `test_serde_missing_optional_fields` — missing optional sections get defaults

### 1.2 `main.rs` — **0 tests, 4090 lines**

> Most of `main.rs` is integration/wiring code. Unit-testable helpers:

- [ ] `test_shellexpand_tilde` — `~/foo` → `/Users/<user>/foo`
- [ ] `test_shellexpand_no_tilde` — `/absolute/path` unchanged
- [ ] `test_shellexpand_str` — string variant
- [ ] `test_copy_dir_recursive` — copies nested dirs (use temp dir)
- [ ] `test_detect_terminal_app` — returns a non-empty string
- [ ] `test_detect_shell_rc` — returns Some for common shells

> **Note:** `cmd_start`, `cmd_init`, `cmd_setup`, `cmd_mcp_server` etc. are integration-level and should be covered by integration tests, not unit tests. Consider extracting testable logic into helper functions.

### 1.3 `template.rs` — 7 tests ✅ (adequate)

---

## 2. `meepo-core`

### 2.1 `types.rs` — **0 tests, 84 lines**

- [ ] `test_channel_type_from_string_all_variants` — discord, slack, imessage, email, alexa, reminders, notes, contacts, unknown→Internal
- [ ] `test_channel_type_display_all_variants` — round-trip Display ↔ from_string
- [ ] `test_channel_type_case_insensitive` — "Discord", "DISCORD", "discord" all work
- [ ] `test_message_kind_default` — `MessageKind::default()` == `Response`
- [ ] `test_incoming_message_serde_roundtrip`
- [ ] `test_outgoing_message_serde_roundtrip`
- [ ] `test_channel_type_serde_roundtrip` — all variants

### 2.2 `agent.rs` — 3 tests, 480 lines

**Missing coverage:**
- [ ] `test_handle_message_budget_exceeded` — budget check returns exceeded → error
- [ ] `test_handle_message_budget_warning` — budget check returns warning → continues
- [ ] `test_handle_message_empty_input` — empty string handling
- [ ] `test_handle_message_very_long_input` — input length validation

### 2.3 `api.rs` — 10 tests, 507 lines

**Missing coverage:**
- [ ] `test_tool_definition_serde`
- [ ] `test_run_tool_loop_max_iterations` — verify loop terminates
- [ ] `test_run_tool_loop_no_tool_use` — returns immediately

### 2.4 `usage.rs` — 5 tests, 543 lines, **20 pub fns**

**Missing coverage (15 untested pub fns):**
- [ ] `test_usage_source_parse_all_variants` — anthropic, openai, google, ollama, custom, unknown
- [ ] `test_accumulated_usage_add` — multiple adds accumulate
- [ ] `test_accumulated_usage_record_tool_call` — tool call count increments
- [ ] `test_accumulated_usage_total_tokens` — input + output
- [ ] `test_usage_tracker_new` — construction with config
- [ ] `test_usage_tracker_record` — inserts into DB
- [ ] `test_usage_tracker_estimate_cost` — uses model prices
- [ ] `test_usage_tracker_estimate_cost_unknown_model` — falls back to default
- [ ] `test_check_budget_no_limits` — returns Ok status
- [ ] `test_check_budget_daily_exceeded` — returns exceeded
- [ ] `test_check_budget_monthly_warning` — returns warning at threshold
- [ ] `test_get_daily_summary` — returns today's data
- [ ] `test_get_monthly_summary` — returns current month
- [ ] `test_get_range_summary` — custom date range
- [ ] `test_export_csv` — returns CSV string

### 2.5 `context.rs` — 2 tests, 73 lines

- [ ] `test_context_builder_all_fields` — verify all fields set
- [ ] `test_context_default_values` — verify defaults

### 2.6 `orchestrator.rs` — 11 tests, 637 lines

**Missing coverage:**
- [ ] `test_orchestrator_max_subtasks_limit` — rejects over limit
- [ ] `test_orchestrator_timeout` — parallel tasks timeout
- [ ] `test_orchestrator_background_group_limit` — max background groups enforced

### 2.7 `middleware.rs` — 5 tests, 475 lines

**Missing coverage:**
- [ ] `test_middleware_chain_order` — middlewares execute in order
- [ ] `test_middleware_short_circuit` — middleware can abort chain
- [ ] `test_middleware_empty_chain` — no middlewares → passthrough

### 2.8 `notifications.rs` — 5 tests, 373 lines

**Missing coverage:**
- [ ] `test_notification_quiet_hours` — suppressed during quiet hours
- [ ] `test_notification_quiet_hours_errors_pass` — errors not suppressed
- [ ] `test_notification_event_types` — all event types dispatch correctly

### 2.9 `guardrails.rs` — 10 tests, 415 lines

**Missing coverage:**
- [ ] `test_guardrails_disabled` — when disabled, all input passes
- [ ] `test_guardrails_max_input_length` — rejects over limit
- [ ] `test_guardrails_severity_levels` — low/medium/high blocking

### 2.10 `secrets.rs` — 9 tests, 335 lines

- [ ] `test_secrets_env_provider_missing_var` — returns error
- [ ] `test_secrets_file_provider_missing_file` — returns error

### 2.11 `corrective_rag.rs` — 3 tests, 341 lines

**Missing coverage:**
- [ ] `test_corrective_rag_no_results` — empty search results
- [ ] `test_corrective_rag_low_relevance` — filters low-relevance results
- [ ] `test_corrective_rag_config_defaults`

### 2.12 `summarization.rs` — 3 tests, 240 lines

- [ ] `test_summarize_empty_conversation`
- [ ] `test_summarize_single_message`

### 2.13 `tool_selector.rs` — 6 tests, 408 lines

**Missing coverage:**
- [ ] `test_tool_selector_no_tools` — empty registry
- [ ] `test_tool_selector_exact_match` — tool name in query
- [ ] `test_tool_selector_ambiguous` — multiple matches

### 2.14 `query_router.rs` — 8 tests, 391 lines

- [ ] `test_query_router_empty_query` — handles empty string
- [ ] `test_query_router_all_route_types` — verify each route type

### 2.15 `tavily.rs` — 9 tests, 399 lines

- [ ] `test_tavily_empty_query` — rejects empty
- [ ] `test_tavily_result_parsing` — parse API response

### 2.16 `doctor/mod.rs` — 12 tests, 507 lines ✅ (good coverage)

### 2.17 `providers/` module

#### `router.rs` — 7 tests, 336 lines
- [ ] `test_router_failover_all_fail` — all providers fail → error
- [ ] `test_router_failover_second_succeeds` — first fails, second works
- [ ] `test_router_exponential_backoff` — delay increases

#### `anthropic.rs` — 7 tests, 365 lines
- [ ] `test_anthropic_request_serialization` — verify JSON structure
- [ ] `test_anthropic_response_parsing` — parse API response

#### `openai.rs` — 7 tests, 483 lines
- [ ] `test_openai_request_serialization`
- [ ] `test_openai_response_parsing`

#### `google.rs` — 9 tests, 440 lines
- [ ] `test_google_request_serialization`

#### `openai_compat.rs` — 2 tests, 97 lines
- [ ] `test_compat_inherits_openai_format`
- [ ] `test_compat_custom_base_url`

#### `types.rs` — 4 tests, 167 lines
- [ ] `test_chat_message_role_serde`
- [ ] `test_chat_response_fields`

### 2.18 `autonomy/` module

#### `mod.rs` — 5 tests, 732 lines
- [ ] `test_autonomous_loop_rate_limiter` — rate limiting works
- [ ] `test_autonomous_loop_daily_plan` — daily plan triggers at correct hour
- [ ] `test_autonomous_loop_disabled` — does nothing when disabled

#### `action_log.rs` — 7 tests, 225 lines ✅ (good)

#### `goals.rs` — 5 tests, 253 lines
- [ ] `test_goal_evaluator_no_goals` — empty list returns empty
- [ ] `test_goal_evaluator_confidence_gating` — low confidence rejected

#### `planner.rs` — 3 tests, 109 lines
- [ ] `test_confidence_gate_below_threshold` — blocks action
- [ ] `test_confidence_gate_at_threshold` — allows action

#### `user_model.rs` — 4 tests, 218 lines
- [ ] `test_user_model_active_hours_histogram` — records correctly
- [ ] `test_user_model_preferred_channel` — most-used channel

### 2.19 `agents/` module

#### `profile.rs` — 10 tests, 209 lines ✅ (good)

#### `manager.rs` — 8 tests, 218 lines ✅ (good)

### 2.20 `audio/` module

#### `mod.rs` — 7 tests, 196 lines ✅
#### `tts.rs` — 7 tests, 332 lines ✅
#### `stt.rs` — 4 tests, 187 lines
- [ ] `test_stt_empty_audio` — rejects empty input
- [ ] `test_stt_config_defaults`

#### `vad.rs` — 6 tests, 175 lines ✅

### 2.21 `sandbox/` module

#### `policy.rs` — 6 tests, 186 lines ✅
#### `docker.rs` — 5 tests, 296 lines
- [ ] `test_docker_container_name_sanitization`
- [ ] `test_docker_timeout_enforcement`

### 2.22 `registry/mod.rs` — 9 tests, 346 lines ✅

### 2.23 `skills/` module

#### `parser.rs` — 7 tests, 197 lines ✅
#### `skill_tool.rs` — 5 tests, 183 lines ✅
#### `mod.rs` — 3 tests, 134 lines
- [ ] `test_skills_disabled` — returns empty when disabled
- [ ] `test_skills_dir_expansion` — tilde expansion works

### 2.24 `platform/` module

#### `mod.rs` — 4 tests, 696 lines
- [ ] `test_platform_factory_returns_impl` — factory function works
- [ ] `test_platform_trait_methods_exist` — all trait methods callable

#### `macos.rs` — 2 tests, 2583 lines (**severely undertested**)
- [ ] `test_run_applescript_timeout` — timeout works
- [ ] `test_run_applescript_empty_script` — rejects empty
- [ ] `test_run_osascript_sanitization` — input sanitized
- [ ] `test_clipboard_get_set` — round-trip (mock)
- [ ] `test_notification_provider_send` — sends notification (mock)
- [ ] `test_screen_capture_provider` — captures screen (mock)
- [ ] `test_music_provider_methods` — all methods exist
- [ ] `test_browser_provider_methods` — all methods exist

#### `windows.rs` — 4 tests, 365 lines
- [ ] `test_windows_platform_stubs` — all stubs return errors on non-Windows

### 2.25 Tools — per-tool coverage gaps

Most tools have schema + basic tests. Missing coverage patterns:

#### `tools/browser.rs` — 19 tests, 1080 lines
- [ ] `test_browser_scroll_schema`
- [ ] `test_browser_wait_for_element_schema`
- [ ] `test_browser_screenshot_tab_schema`

#### `tools/code.rs` — 10 tests, 929 lines
- [ ] `test_code_tool_workspace_validation` — rejects paths outside workspace
- [ ] `test_code_tool_branch_name_validation` — rejects special chars

#### `tools/filesystem.rs` — 10 tests, 592 lines
- [ ] `test_filesystem_path_traversal_all_patterns` — `../`, symlinks, null bytes
- [ ] `test_filesystem_size_limits` — read/write size limits

#### `tools/memory.rs` — 13 tests, 620 lines ✅ (good)

#### `tools/system.rs` — 26 tests, 1152 lines ✅ (excellent)

#### `tools/macos.rs` — 27 tests, 1238 lines ✅ (excellent)

#### `tools/usage_stats.rs` — 1 test, 169 lines
- [ ] `test_usage_stats_execute_no_tracker` — returns error
- [ ] `test_usage_stats_execute_with_period`
- [ ] `test_usage_stats_execute_csv_format`

#### `tools/search.rs` — 2 tests, 93 lines
- [ ] `test_search_tool_empty_query` — rejects empty
- [ ] `test_search_tool_max_results`

#### `tools/autonomous.rs` — 4 tests, 424 lines
- [ ] `test_autonomous_tool_schemas_all`
- [ ] `test_set_goal_missing_fields`
- [ ] `test_list_goals_empty`

#### `tools/delegate.rs` — 6 tests, 313 lines
- [ ] `test_delegate_missing_task`
- [ ] `test_delegate_invalid_agent`

#### `tools/canvas.rs` — 8 tests, 333 lines ✅

#### `tools/accessibility.rs` — 5 tests, 260 lines
- [ ] `test_accessibility_tool_disabled`

#### `tools/rag.rs` — 5 tests, 420 lines
- [ ] `test_rag_tool_empty_query`
- [ ] `test_rag_tool_no_results`

#### `tools/sandbox_exec.rs` — 4 tests, 138 lines ✅

#### `tools/watchers.rs` — 5 tests, 319 lines ✅

#### Lifestyle tools (all follow same pattern — need error path tests):

| Tool | Tests | Lines | Missing |
|------|-------|-------|---------|
| `calendar.rs` | 5 | 641 | `test_calendar_invalid_date`, `test_calendar_empty_title` |
| `email_intelligence.rs` | 4 | 432 | `test_email_empty_query`, `test_email_invalid_filter` |
| `finance.rs` | 5 | 618 | `test_finance_invalid_amount`, `test_finance_negative_amount` |
| `health.rs` | 5 | 506 | `test_health_invalid_metric`, `test_health_future_date` |
| `news.rs` | 4 | 507 | `test_news_empty_query`, `test_news_max_results` |
| `research.rs` | 4 | 592 | `test_research_empty_topic`, `test_research_max_depth` |
| `sms.rs` | 3 | 385 | `test_sms_invalid_number`, `test_sms_empty_body`, `test_sms_number_format` |
| `social.rs` | 3 | 490 | `test_social_invalid_platform`, `test_social_empty_message`, `test_social_rate_limit` |
| `tasks.rs` | 6 | 698 | `test_tasks_invalid_priority`, `test_tasks_empty_title` |
| `travel.rs` | 4 | 503 | `test_travel_invalid_location`, `test_travel_past_date` |

#### macOS-specific tools:

| Tool | Tests | Lines | Missing |
|------|-------|-------|---------|
| `macos_finder.rs` | 5 | 397 | `test_finder_path_validation` |
| `macos_keychain.rs` | 3 | 175 | `test_keychain_empty_service`, `test_keychain_empty_account` |
| `macos_media.rs` | 5 | 343 | `test_media_invalid_action` |
| `macos_messages.rs` | 4 | 227 | `test_messages_empty_recipient` |
| `macos_productivity.rs` | 3 | 136 | `test_productivity_invalid_app` |
| `macos_shortcuts.rs` | 3 | 141 | `test_shortcuts_empty_name` |
| `macos_spotlight.rs` | 3 | 153 | `test_spotlight_empty_query` |
| `macos_system.rs` | 8 | 650 | ✅ (good) |
| `macos_terminal.rs` | 4 | 192 | `test_terminal_empty_command` |
| `macos_windows.rs` | 4 | 328 | `test_windows_invalid_app` |

---

## 3. `meepo-channels`

### 3.1 `bus.rs` — 8 tests, 293 lines ✅

### 3.2 `rate_limit.rs` — 5 tests, 130 lines ✅

### 3.3 `discord.rs` — 9 tests, 533 lines
- [ ] `test_discord_allowed_users_filter` — rejects non-allowed users
- [ ] `test_discord_empty_message` — handles empty content

### 3.4 `slack.rs` — 3 tests, 503 lines
- [ ] `test_slack_poll_interval` — respects config
- [ ] `test_slack_allowed_users_filter`
- [ ] `test_slack_empty_message`
- [ ] `test_slack_message_formatting`

### 3.5 `imessage.rs` — 8 tests, 504 lines
- [ ] `test_imessage_allowed_contacts_filter`
- [ ] `test_imessage_empty_message`

### 3.6 `email.rs` — 4 tests, 396 lines
- [ ] `test_email_subject_prefix` — prefix applied
- [ ] `test_email_disabled` — no-op when disabled
- [ ] `test_email_poll_interval`

### 3.7 `alexa.rs` — 1 test, 123 lines
- [ ] `test_alexa_disabled` — no-op when disabled
- [ ] `test_alexa_skill_id_validation`
- [ ] `test_alexa_message_format`

### 3.8 `notes.rs` — 3 tests, 360 lines
- [ ] `test_notes_folder_name` — uses config folder
- [ ] `test_notes_tag_prefix` — applies prefix
- [ ] `test_notes_disabled`

### 3.9 `reminders.rs` — 3 tests, 329 lines
- [ ] `test_reminders_list_name` — uses config list
- [ ] `test_reminders_disabled`
- [ ] `test_reminders_poll_interval`

### 3.10 `contacts.rs` — 6 tests, 525 lines
- [ ] `test_contacts_group_name` — uses config group
- [ ] `test_contacts_disabled`

### 3.11 `lib.rs` — 0 tests, 37 lines
- [ ] `test_channel_trait_object_safety` — trait is object-safe

---

## 4. `meepo-knowledge`

### 4.1 `sqlite.rs` — 6 tests, 2043 lines, **37 pub fns** (**severely undertested**)

**Tested:** entity ops, relationship ops, goal ops, preference ops, action log ops, background task ops

**Missing (31 untested functions):**
- [ ] `test_new_creates_tables` — verify all tables created
- [ ] `test_insert_conversation` — insert and verify
- [ ] `test_get_recent_conversations` — returns in order
- [ ] `test_get_recent_conversations_limit` — respects limit
- [ ] `test_insert_watcher` — insert and verify
- [ ] `test_get_active_watchers` — only active returned
- [ ] `test_get_watcher_by_id` — found and not-found
- [ ] `test_update_watcher_active` — toggle active flag
- [ ] `test_delete_watcher` — removes from DB
- [ ] `test_get_due_goals` — only due goals returned
- [ ] `test_get_active_goals` — only active goals
- [ ] `test_update_goal_status` — status changes
- [ ] `test_update_goal_checked` — checked_at updates
- [ ] `test_delete_goals_by_source` — deletes matching, returns count
- [ ] `test_insert_approval` — insert and verify
- [ ] `test_get_pending_approvals` — only pending returned
- [ ] `test_decide_approval` — approved/rejected status
- [ ] `test_cleanup_old_conversations` — removes old, keeps recent
- [ ] `test_insert_background_task` — insert and verify
- [ ] `test_update_background_task` — status/result update
- [ ] `test_get_active_background_tasks` — only running tasks
- [ ] `test_get_recent_background_tasks` — respects limit
- [ ] `test_insert_usage_log` — insert and verify
- [ ] `test_get_usage_cost_for_date` — correct sum
- [ ] `test_get_usage_cost_for_range` — correct sum over range
- [ ] `test_get_usage_summary` — all fields populated
- [ ] `test_export_usage_csv` — valid CSV format
- [ ] `test_search_entities_empty_query` — returns empty
- [ ] `test_get_all_entities` — returns all
- [ ] `test_get_relationships_for_nonexistent` — returns empty

### 4.2 `graph.rs` — 4 tests, 437 lines, **20 pub fns**

**Missing:**
- [ ] `test_get_context_for` — returns entity + relationships
- [ ] `test_recall_empty` — no results for unknown query
- [ ] `test_get_entity_nonexistent` — returns None
- [ ] `test_search_entities_by_type` — type filter works
- [ ] `test_store_and_get_conversations` — round-trip
- [ ] `test_create_and_get_watchers` — round-trip
- [ ] `test_update_watcher` — toggle active
- [ ] `test_delete_watcher` — removes
- [ ] `test_reindex` — doesn't error
- [ ] `test_get_all_entities` — returns all
- [ ] `test_db_accessor` — `db()` returns Arc
- [ ] `test_cleanup_old_conversations` — delegates to sqlite

### 4.3 `embeddings.rs` — 8 tests, 440 lines

**Missing:**
- [ ] `test_vector_index_remove` — removes entry
- [ ] `test_vector_index_persist_and_load` — round-trip to DB
- [ ] `test_vector_index_len_is_empty` — len/is_empty correct
- [ ] `test_noop_provider_dimensions` — returns correct dims

### 4.4 `chunking.rs` — 5 tests, 322 lines ✅

### 4.5 `graph_rag.rs` — 3 tests, 308 lines
- [ ] `test_graph_expand_multiple_hops` — multi-hop expansion
- [ ] `test_graph_expand_no_relationships` — single entity only

### 4.6 `memory_sync.rs` — 3 tests, 109 lines ✅

### 4.7 `tantivy.rs` — 2 tests, 302 lines
- [ ] `test_tantivy_update_document` — update existing
- [ ] `test_tantivy_search_empty_index` — returns empty
- [ ] `test_tantivy_search_pagination` — limit works

---

## 5. `meepo-scheduler`

### 5.1 `persistence.rs` — 8 tests, 646 lines, **12 pub fns**

**Missing:**
- [ ] `test_get_watcher_by_id_found` — returns Some
- [ ] `test_get_watcher_by_id_not_found` — returns None
- [ ] `test_get_last_run` — returns correct timestamp
- [ ] `test_get_last_run_never_run` — returns None
- [ ] `test_cleanup_old_events` — removes old, keeps recent
- [ ] `test_save_watcher_event` — insert and retrieve
- [ ] `test_get_watcher_events_limit` — respects limit

### 5.2 `watcher.rs` — 4 tests, 357 lines, **12 pub fns**

**Missing:**
- [ ] `test_watcher_description_all_kinds` — each kind has description
- [ ] `test_watcher_is_polling` — true for polling kinds
- [ ] `test_watcher_is_event_driven` — true for event kinds
- [ ] `test_watcher_is_scheduled` — true for cron kinds
- [ ] `test_watcher_event_email` — factory method
- [ ] `test_watcher_event_calendar` — factory method
- [ ] `test_watcher_event_file_changed` — factory method
- [ ] `test_watcher_event_github` — factory method
- [ ] `test_watcher_event_task` — factory method

### 5.3 `runner.rs` — 5 tests, 1019 lines ✅

### 5.4 `lib.rs` — 1 test, 47 lines ✅

---

## 6. `meepo-mcp`

### 6.1 `protocol.rs` — 5 tests, 188 lines ✅
### 6.2 `server.rs` — 6 tests, 254 lines ✅
### 6.3 `adapter.rs` — 4 tests, 117 lines ✅

### 6.4 `client.rs` — 2 tests, 351 lines
- [ ] `test_client_connect_timeout` — timeout on connect
- [ ] `test_client_tool_call_response_parsing` — parse response
- [ ] `test_client_reconnect` — reconnect after disconnect
- [ ] `test_client_env_vars_passed` — env vars forwarded to subprocess

---

## 7. `meepo-a2a`

### 7.1 `protocol.rs` — 4 tests, 122 lines ✅

### 7.2 `server.rs` — 2 tests, 335 lines
- [ ] `test_server_auth_required` — rejects unauthenticated
- [ ] `test_server_auth_valid` — accepts valid token
- [ ] `test_server_tool_call_routing` — routes to correct tool
- [ ] `test_server_unknown_tool` — returns error
- [ ] `test_server_agent_card` — returns valid card

### 7.3 `client.rs` — 2 tests, 222 lines
- [ ] `test_client_send_task` — sends and receives
- [ ] `test_client_auth_header` — includes bearer token
- [ ] `test_client_timeout` — respects timeout

### 7.4 `tool.rs` — 2 tests, 181 lines
- [ ] `test_a2a_tool_schema` — valid schema
- [ ] `test_a2a_tool_missing_agent` — returns error
- [ ] `test_a2a_tool_missing_message` — returns error

---

## 8. `meepo-gateway`

### 8.1 `auth.rs` — 5 tests ✅
### 8.2 `events.rs` — 3 tests ✅
### 8.3 `protocol.rs` — 5 tests ✅
### 8.4 `session.rs` — 27 tests ✅ (excellent)
### 8.5 `session_tools.rs` — 24 tests ✅ (excellent)
### 8.6 `server.rs` — 11 tests, 538 lines

**Missing:**
- [ ] `test_handle_request_session_history` — session.history method
- [ ] `test_handle_request_session_delete` — session.delete method
- [ ] `test_handle_request_status_fields` — verify all status fields

### 8.7 `webchat.rs` — 2 tests, 73 lines ✅

---

## Implementation Priority

### Phase 1 — Critical gaps (highest ROI)
1. **`config.rs`** — 0 tests, 1308 lines of config parsing, env var expansion, secret masking
2. **`sqlite.rs`** — 6 tests for 37 pub fns (database layer)
3. **`types.rs`** — 0 tests, core types used everywhere
4. **`usage.rs`** — 5 tests for 20 pub fns (billing/budget)

### Phase 2 — Important gaps
5. **`graph.rs`** — 4 tests for 20 pub fns (knowledge graph)
6. **`watcher.rs`** — 4 tests for 12 pub fns (scheduler)
7. **`persistence.rs`** — 8 tests for 12 pub fns (scheduler DB)
8. **`platform/macos.rs`** — 2 tests for 2583 lines

### Phase 3 — Fill remaining gaps
9. All lifestyle tools error paths
10. All macOS-specific tools missing tests
11. Channel adapters edge cases
12. Provider request/response serialization
13. MCP client, A2A client/server

### Phase 4 — Polish
14. `main.rs` helper functions
15. Integration-style tests for wiring code
16. Serde round-trip tests for all config structs

---

## Testing Patterns to Follow

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 1. Schema tests — verify tool input_schema has correct fields
    #[test]
    fn test_tool_schema() {
        let tool = MyTool::new(/* deps */);
        let schema = tool.input_schema();
        assert!(schema["properties"]["param"].is_object());
        assert!(schema["required"].as_array().unwrap().contains(&json!("param")));
    }

    // 2. Error path tests — verify missing/invalid input returns Err
    #[tokio::test]
    async fn test_tool_missing_param() {
        let tool = MyTool::new(/* deps */);
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
    }

    // 3. DB tests — use temp dirs
    #[tokio::test]
    async fn test_db_operation() {
        let dir = tempfile::tempdir().unwrap();
        let db = KnowledgeDb::new(dir.path().join("test.db")).unwrap();
        // ... test operations ...
    }

    // 4. Serde round-trip tests
    #[test]
    fn test_serde_roundtrip() {
        let original = MyStruct { /* ... */ };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MyStruct = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }
}
```

---

## Verification

After implementing all tests:

```bash
cargo test --workspace                    # All tests pass
cargo clippy --workspace                  # No warnings
cargo llvm-cov --workspace --html         # Generate coverage report
```

Install `cargo-llvm-cov` for actual coverage measurement:
```bash
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --html --open  # View in browser
```
