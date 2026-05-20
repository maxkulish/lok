# Plan: CLO-383 — FR-22: `apply_edits=true` defaults Codex sandbox to `workspace-write`

## Context
- **Design:** `docs/designs/clo-383-apply-edits-sandbox-default.md`
- **Discovery:** `docs/discovery/clo-383.md`
- **Linear:** https://linear.app/cloud-ai/issue/CLO-383/fr-22-apply_editstrue-defaults-codex-sandbox-to-workspace-write
- **Depends on:** CLO-374 (FR-21 per-step sandbox routing)

## Sub-tasks

### ST1 Add `apply_edits` field to `StepContext`
**Files:** `src/backend/context.rs`
**What:** Add `pub apply_edits: bool` to `StepContext`, default `false` in `from_prompt`. Update any struct-literal test in the same file.
**Acceptance:** `cargo test test_step_context_from_prompt_defaults` (or all context.rs tests) pass.
**Estimate:** S

### ST2 Wire `apply_edits` through `workflow::step_context()`
**Files:** `src/workflow.rs`
**What:** Set `apply_edits: step.apply_edits` in the `StepContext` spread literal inside `step_context()`.
**Acceptance:** `cargo test step_context_tests` (or all workflow.rs tests) pass.
**Estimate:** S

### ST3 Implement Codex backend effective-sandbox resolution with unit tests
**Files:** `src/backend/codex.rs`
**What:**
- Update `build_argv_prefix` signature to accept `apply_edits: bool`.
- Implement resolution rule: `(true, None) => WorkspaceWrite`; `(true, Some(ReadOnly)) => warn + preserve`; else passthrough.
- Call site in `query()` passes `ctx.apply_edits`.
- Add 7 unit tests covering the 8-row matrix (excluding the ReadOnly + warn variant which is tested alongside the resolution).
**Acceptance:** `cargo test codex_apply_edits` (all `codex_apply_edits_*` tests) pass.
**Estimate:** M

### ST4 Implement Gemini backend effective-sandbox resolution with unit tests
**Files:** `src/backend/gemini.rs`
**What:**
- Update `build_shell_cmd` signature to accept `apply_edits: bool`.
- Implement same resolution rule as Codex (maps `WorkspaceWrite -> auto_edit`, `ReadOnly -> plan`, etc.).
- Call site in `query()` passes `ctx.apply_edits`.
- Add 6 unit tests mirroring the Codex matrix.
**Acceptance:** `cargo test gemini_apply_edits` (all `gemini_apply_edits_*` tests) pass.
**Estimate:** M

### ST5 Compilation regression guard + pre-merge gate
**Files:** `src/`, `tests/`
**What:**
- Run `rg 'StepContext \{' src/ tests/` to confirm no struct literal missed the new field.
- Run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- Verify no warnings about `apply_edits` being unused or dead.
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` passes clean.
**Estimate:** S

## Pre-merge gate
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Risks
- **Struct-literal breakage:** Any `StepContext { ... }` literal outside `from_prompt` will break compilation. Mitigated by ST5 compilation guard.
- **Test duplication:** The Codex and Gemini matrix tests are ~90% identical logic. Mitigated by keeping them backend-local; helper extraction is deferred per design open question.
- **Warning emission point uncertainty:** Deciding during ST3/ST4 whether to warn inside the builder or in `query()`. Either choice is safe per design.
