# PRD: Port llm-mux Patterns to Lok

**Date:** 2026-03-30
**Source:** https://github.com/ducks/llm-mux
**Status:** Draft

## Background

llm-mux is a clean-room redesign of lok's core concepts with more mature abstractions. This document captures the specific patterns and components worth porting back to lok, prioritized by impact and effort.

## Phase 1: Typed Backend Errors + Retry Decorator

**Impact:** High | **Effort:** Medium
**Relates to:** CLO-180 (QueryOutput struct)

### Problem

lok uses `anyhow::Result` everywhere - errors are opaque strings. No retry logic at the backend level. The 365-day "no timeout" magic value is fragile.

### What to port

- `BackendError` enum with 7 variants: `Timeout`, `RateLimit`, `Auth`, `Network`, `Parse`, `ExecutionFailed`, `Unavailable`, `Config`
- `is_retryable()` method - only `Timeout`, `RateLimit`, `Network` are retryable
- `RetryPolicy` struct with exponential backoff, jitter, max delay, and server-provided `retry_after` respect
- `RetryExecutor<T: BackendExecutor>` - generic decorator pattern wrapping any backend with retry

### Changes

- Add `BackendError` enum and `is_retryable()` to `src/backend/mod.rs`
- Create `src/backend/retry.rs` with `RetryPolicy` + `RetryExecutor`
- Change `Backend::query()` return type from `Result<QueryOutput>` to `Result<QueryOutput, BackendError>`

---

## Phase 2: Richer QueryOutput

**Impact:** Medium | **Effort:** Low
**Relates to:** CLO-180 (QueryOutput struct)

### Problem

lok's `QueryOutput` only has `stdout`, `stderr`, `exit_code`. No model info, no token usage, no duration, no structured JSON extraction.

### What to port

Extend `QueryOutput` with fields from llm-mux's `BackendResponse`:

```rust
struct QueryOutput {
    stdout: String,
    stderr: Option<String>,
    exit_code: Option<i32>,
    // New fields:
    model: Option<String>,
    duration: Duration,
    usage: Option<TokenUsage>,        // prompt_tokens, completion_tokens, total_tokens
    structured: Option<serde_json::Value>,
    backend: String,
}
```

### Changes

- Add fields to `QueryOutput` in `src/backend/mod.rs`
- Update each backend implementation to populate new fields where available

---

## Phase 3: Config Merging

**Impact:** Medium | **Effort:** Low

### Problem

lok's config loader picks the first file found (project OR user). No merging - user-level defaults can't be overridden by project-level settings.

### What to port

- Three-layer merge: built-in defaults -> `~/.config/lok/lok.toml` -> `./lok.toml`
- Field-level merge where later layers override earlier
- Map-level merge (backends, tasks) at key granularity
- `deny_unknown_fields` for strict TOML validation to catch typos

### Changes

- Add `merge(&mut self, other: Config)` method to `Config` in `src/config.rs`
- Update `load_config()` to load both user and project configs, merge them
- Add `#[serde(deny_unknown_fields)]` to config structs

---

## Phase 4: MiniJinja Templates

**Impact:** Medium | **Effort:** Medium

### Problem

lok has 8 separate regex patterns for interpolation (`INTERPOLATE_RE`, `FIELD_RE`, `ENV_RE`, `ARG_RE`, `ITEM_RE`, `ITEM_FIELD_RE`, `INDEX_RE`, `WORKFLOW_BACKENDS_RE`) plus an `UNKNOWN_VAR_RE` catch-all. No filters, no defaults, no conditionals in expressions.

### What to port

- MiniJinja 2.0 integration with a `TemplateContext` object
- Filters: `shell_escape`, `json`, `join`, `first`, `last`, `default`, `trim`, `lines`, `strftime`
- Full Jinja2 syntax: `{% if %}`, `{% for %}`, `{{ value | filter }}`
- Lazy `{{ env.VAR }}` lookup via custom MiniJinja Object trait

### Changes

- Add `minijinja = "2.0"` to `Cargo.toml`
- Create `src/template/mod.rs`, `src/template/context.rs`, `src/template/filters.rs`
- Replace regex-based interpolation functions in `src/workflow.rs` with MiniJinja rendering
- Backward compatible: existing `{{ steps.X.output }}` syntax is already Jinja-compatible

### Security note

The `shell_escape` filter prevents injection when step outputs are used in shell commands.

---

## Phase 5: Apply-and-Verify Pipeline

**Impact:** High | **Effort:** High

### Problem

lok shells out to an external `git-agent` tool for code modifications. String-based edits fail on ambiguous matches. No built-in diff parsing, verification, or rollback.

### What to port

- `EditParser` - auto-detects 3 formats: unified diff, old/new JSON pairs, full file content. Extracts JSON from markdown code blocks.
- `DiffApplier` - applies parsed edits to files
- `Verification` - runs `sh -c {command}` with timeout, captures stdout/stderr/exit_code
- `Rollback` - reverts files on verification failure
- `RetryLoop` - retries the full apply-verify cycle

### Changes

- Create `src/apply_verify/mod.rs` with submodules:
  - `edit_parser.rs` - three-format parser with auto-detection
  - `diff_applier.rs` - file modification logic
  - `verification.rs` - post-apply command runner
  - `rollback.rs` - revert on failure
  - `retry_loop.rs` - retry the full cycle
- Update workflow step execution to use the new pipeline for `apply_edits` steps
- Keep `git-agent` integration as optional for event logging, remove it as edit dependency

---

## Phase 6: Configurable Role Routing

**Impact:** Low | **Effort:** Medium

### Problem

lok's delegation system uses hardcoded keyword-to-backend mappings in `src/delegation.rs`. Backend profiles can't be changed without code changes.

### What to port

- Roles defined in config TOML instead of code
- Two-tier resolution: team-specific override then global fallback
- Execution strategies per role: `First`, `Parallel`, `Fallback`
- `min_success` threshold for parallel execution

### Changes

- Add `[roles]` and `[teams]` sections to `lok.toml` schema
- Create `src/role/mod.rs` with `role_resolver.rs`
- Move hardcoded profiles from `delegation.rs` into default config
- Keep keyword classification as fallback for unconfigured roles

---

## Phase 7: SQLite Knowledge Store (Future)

**Impact:** Low (currently) | **Effort:** High

### Problem

lok uses file-based JSON cache with simple TTL. No persistent memory across workflow runs beyond caching.

### What llm-mux has

- SQLite with 6 tables: facts, relationships, findings, workflow runs, entities, entity properties
- Temporal tracking (SCD Type 2) on entity properties with `valid_from`/`valid_to`
- Per-ecosystem isolation

### Assessment

Defer until lok grows multi-repo/ecosystem awareness. The file-based cache is sufficient for current use cases. Revisit when Phase 5 (apply-and-verify) lands, since workflow runs and findings tracking become valuable with a built-in code modification pipeline.

---

## Implementation Order Rationale

| Phase | Dependency | Why this order |
|-------|-----------|----------------|
| 1 - Typed errors | None | Unblocks CLO-180, foundation for retry |
| 2 - QueryOutput | Phase 1 | Natural CLO-180 extension, small diff |
| 3 - Config merge | None | Small diff, quality-of-life, no blockers |
| 4 - MiniJinja | None | Replaces fragile regex, security win |
| 5 - Apply-verify | Phase 1 (error types) | Biggest effort, biggest payoff |
| 6 - Role routing | Phase 3 (config merge) | Needs config layer working first |
| 7 - Knowledge store | Phase 5 (apply-verify) | Only valuable with code modification pipeline |
