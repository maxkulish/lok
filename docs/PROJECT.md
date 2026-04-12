# Project Dashboard - Lok

**Last Updated**: 2026-04-12

## Active Work (WIP Limit: 3)

| Task | Title | Status | Phase | Blocked By |
|------|-------|--------|-------|------------|
| - | - | - | - | - |

## Up Next (Prioritized Backlog)

| Priority | Task | Title | Dependencies |
|----------|------|-------|--------------|
| - | - | - | - |

## Recently Completed

| Task | Title | Completed | Summary |
|------|-------|-----------|---------|
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
