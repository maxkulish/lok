# Project Dashboard - Lok

**Last Updated**: 2026-05-24 (CLO-395 complete)

## Active Work (WIP Limit: 3)

| Task | Title | Status | Phase | Blocked By |
|------|-------|--------|-------|------------|
| [CLO-392](https://linear.app/cloud-ai/issue/CLO-392) | FR-13: Codex health probe + version-aware unusable-flag matrix | In Progress | Phase 10 | - |

## Up Next (Prioritized Backlog)

| Priority | Task | Title | Dependencies |
|----------|------|-------|--------------|
| - | - | - | - |

## Recently Completed

| Task | Title | Completed | Summary |
|------|-------|-----------|---------|
| [CLO-395](https://linear.app/cloud-ai/issue/CLO-395/fr-12b-opencode-health-probe-google-auth-detection) | FR-12b: opencode health probe + Google auth detection | 2026-05-24 | Replaced which-only stub with multi-step probe: version (--version, 1s), auth (auth.json → env → CLI), models (models google). 13 unit tests. PR #50. |
| [CLO-391](https://linear.app/cloud-ai/issue/CLO-391) | FR-13a: Claude dual-mode health probe (Api vs Cli) | 2026-05-24 | Implemented `mode` and `diagnostic` fields in `HealthStatus`; added `probe_api()` (offline key/model check) and `probe_cli()` (binary + version + `--help` + JSON support); 12 unit tests; merged in PR #43. |
| [CLO-389](https://linear.app/cloud-ai/issue/CLO-389) | FR-11 + FR-11a: Ollama health probe (/api/version + /api/tags) + ModelInfo + workflow model validation | 2026-05-23 | Implemented active Ollama health check probing version/tags, cached HealthStatus.models, and added workflow step model validation with strict matching and helpful pull suggestions. |
| [CLO-388](https://linear.app/cloud-ai/issue/CLO-388) | FR-9a + FR-10 + FR-15: Engine warmup + HealthCache + sync is_available cache-only | 2026-05-21 | Implemented parallel async warmup, std OnceLock-based HealthCache, and made backend is_available checks cache-only with zero syscalls. |
| [CLO-382](https://linear.app/cloud-ai/issue/CLO-382/fr-26-gemini-backend-extracts-token-counts-from-json-envelope) | FR-26: Gemini backend extracts token counts from JSON envelope | 2026-05-20 | Gemini JSON envelope schema parsed; handles nested stats.models.*.tokens shapes with flat fallback; captured real CLI v0.42.0 envelope fixture and integrated validation tests |
| [CLO-384](https://linear.app/cloud-ai/issue/CLO-384/fr-23-per-step-timeout-layered-override-step-backend-global) | FR-23: per-step `timeout` layered override (step > backend > global) | 2026-05-20 | Implemented layered timeout resolution with humantime config deserialization and unified effective timeout use across LLM, shell, format, and verify paths |
| [CLO-383](https://linear.app/cloud-ai/issue/CLO-383/fr-22-apply_editstrue-defaults-codex-sandbox-to-workspace-write) | FR-22: `apply_edits=true` defaults Codex sandbox to `workspace-write` | 2026-05-20 | Resolves effective sandbox: apply_edits=true + no sandbox → workspace-write/auto-edit; explicit sandbox always wins; threads sandbox+apply_edits through all StepContext call sites (single/many/for_each/retry); 15 backend + 1 workflow tests |
| [CLO-380](https://linear.app/cloud-ai/issue/CLO-380/fr-3b-codex-o-output-last-message-authoritative-result-extraction) | FR-3b: Codex `-o`/`--output-last-message` authoritative result extraction | 2026-05-20 | Added authoritative last-message success-path with JSONL fallback and fixture-backed precedence tests |
| [CLO-381](https://linear.app/cloud-ai/issue/CLO-381/fr-25-codex-backend-extracts-usage-from-turncompleted-events) | FR-25: Codex backend extracts `usage` from `turn.completed` events | 2026-05-20 | Wired cached_input_tokens and reasoning_output_tokens from Codex turn.completed usage |
| [CLO-378](https://linear.app/cloud-ai/issue/CLO-378/fr-25b-extend-tokenusage-with-cached-tokens-reasoning-tokens) | FR-25b: extend TokenUsage with cached_tokens + reasoning_tokens | 2026-05-20 | Extended TokenUsage with cached_tokens + reasoning_tokens fields |
| [CLO-373](https://linear.app/cloud-ai/issue/CLO-373/capture-codex-jsonl-fixtures-for-parser-test-corpus-fr-40) | Capture Codex JSONL fixtures for parser test corpus (FR-40) | 2026-05-19 | Added scrubbed Codex JSONL corpus, capture README/version metadata, LF normalization, and fixture validation tests for FR-3a parser work. |
| [CLO-372](https://linear.app/cloud-ai/issue/CLO-372) | Thread `StepContext` through non-Step `Backend::query` call sites (FR-20b) | 2026-05-18 | Threaded StepContext through non-Step Backend::query call sites, propagating model and timeout context to backend, conductor, spawn, team, and debate paths. |
| [CLO-371](https://linear.app/cloud-ai/issue/CLO-371) | Migrate `Backend::query` to `StepContext` + add async `health_check` + sweep Step call sites (FR-19a/19b/20a) | 2026-05-18 | StepContext carrying struct; Backend::query migrated across all backends and call sites; async health_check added; timeout context populated; validation fix iteration applied; PR #18 merged |
| [CLO-370](https://linear.app/cloud-ai/issue/CLO-370) | Add `usage` field to `StepResult` for end-to-end token observability (FR-25a) | 2026-05-18 | StepResult.usage: Option<TokenUsage> wired through all 4 LLM paths (single-backend, consensus+synthesis, for_each with aggregation, shell=None); BackendResponse extended for consensus; aggregate_usage helper; 2 new unit tests; 468 unit tests pass |
| [CLO-212](https://linear.app/cloud-ai/issue/CLO-212) | Configurable role routing with [roles]/[teams] config | 2026-04-12 | RoleResolver with two-tier lookup (team->global), RoutingStrategy enum (First/Parallel/Fallback), --team/--role/--explain CLI flags, Delegator fallback, deny_unknown_fields, 20 role tests |
| [CLO-207](https://linear.app/cloud-ai/issue/CLO-207) | Extend QueryOutput with model, duration, usage, structured, backend | 2026-04-12 | QueryOutput extended with model/duration/usage/structured/backend; TokenUsage struct; 5 backends updated; 19 new tests |
| [CLO-211](https://linear.app/cloud-ai/issue/CLO-211) | Wire apply-verify pipeline into workflow step execution | 2026-04-11 | RetryLoop replaces legacy fix_loop with shell-composed (format) \|\| true && (verify); command_wrapper applied to composed cmd; apply_once helper for verify=None; ST-3 spec error format strings with 4KB raw/1KB stderr truncation; Mutex poison recovery; 14 new tests, 429 total |
| [CLO-210](https://linear.app/cloud-ai/issue/CLO-210) | Implement DiffApplier, Rollback, Verification, RetryLoop | 2026-04-09 | 4 modules in apply_verify: DiffApplier (3-format apply), Rollback (reverse-order restore), Verification (bounded shell + process-group), RetryLoop (parse-apply-verify-rollback cycle); 45 new tests |
| [CLO-216](https://linear.app/cloud-ai/issue/CLO-216) | Support validate.mode = "lenient" for noise-cleanup validators | 2026-04-08 | mode="lenient" bypasses parser; any non-empty response passes; reconciled with tests |
| [CLO-215](https://linear.app/cloud-ai/issue/CLO-215) | Add --explain-validation CLI flag for raw validator response | 2026-04-08 | raw_response field on ValidationResult + CLI flag dump on parse failure |
| [CLO-214](https://linear.app/cloud-ai/issue/CLO-214) | Add validate.on_parse_error config (pass/skip/fail) | 2026-04-08 | on_parse_error policy independent of on_error; reconciled with helper extraction + tests |
| [CLO-208](https://linear.app/cloud-ai/issue/CLO-208) | Add RetryExecutor decorator wrapping Backend trait | 2026-04-08 | Transparent decorator, retries only on is_retryable(), honors RateLimit retry_after_ms |
| [CLO-206](https://linear.app/cloud-ai/issue/CLO-206) | Add RetryPolicy with exponential backoff, jitter, retry_after | 2026-04-08 | Exponential backoff (base * 2^n) with ±10% jitter, clamped at max_delay; spec divergences logged in Linear |
| [CLO-209](https://linear.app/cloud-ai/issue/CLO-209) | Replace regex interpolation in workflow.rs with MiniJinja rendering | 2026-04-07 | MiniJinja replaces 14 regexes + 4 interpolation/condition functions; backward-compat legacy translator; 348 tests |
| [CLO-205](https://linear.app/cloud-ai/issue/CLO-205) | Implement EditParser with 3-format auto-detection | 2026-04-07 | EditParser auto-detects unified diff/JSON/full-file formats with markdown extraction, CRLF normalization, 1MB limit, 32 tests |
| [CLO-204](https://linear.app/cloud-ai/issue/CLO-204) | Add MiniJinja integration with TemplateContext and custom filters | 2026-04-04 | MiniJinja 2.0 template engine with TemplateContext, LazyEnv, 8 custom filters, TemplateError enum, 48 tests |
| [CLO-203](https://linear.app/cloud-ai/issue/CLO-203) | Implement three-layer config merge with deny_unknown_fields | 2026-04-04 | Three-layer config merge (defaults -> user -> project), toml::Value deep merge, deny_unknown_fields on all structs |
| [CLO-202](https://linear.app/cloud-ai/issue/CLO-202) | Add BackendError enum with typed variants and is_retryable() | 2026-04-04 | Added BackendError enum (8 variants), is_retryable(), typed errors on Backend::query(), QueryResult.error field |
| [CLO-185](https://linear.app/cloud-ai/issue/CLO-185) | Implement structured failure data for step errors | 2026-04-03 | Added StepFailure struct with StepFailureKind enum (6 variants) for execution-level failure classification |
| [CLO-184](https://linear.app/cloud-ai/issue/CLO-184) | Implement LLM-based step validation (validate.backend + prompt) | 2026-04-03 | Added LLM validation via validate.backend + validate.prompt with JSON structured output, fail-closed parsing, on_error policy |
| [CLO-183](https://linear.app/cloud-ai/issue/CLO-183) | Implement heuristic validators (check field) for step validation | 2026-04-03 | Added not_empty, min_length, contains validators via [steps.validate] TOML config |
| [CLO-182](https://linear.app/cloud-ai/issue/CLO-182) | Extend StepResult with stderr, exit_code, validation fields | 2026-03-31 | Added raw_output, stderr, exit_code, and ValidationResult fields to StepResult |
| [CLO-181](https://linear.app/cloud-ai/issue/CLO-181) | - | - | - |
| [CLO-180](https://linear.app/cloud-ai/issue/CLO-180) | - | - | - |

## Blocked

| Task | Title | Blocked By | Notes |
|------|-------|------------|-------|
| - | - | - | - |
