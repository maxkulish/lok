# Plan: CLO-380 — FR-3b Codex `-o/--output-last-message` authoritative result extraction

## Context

- **Design:** docs/designs/clo-380-codex-output-last-message.md
- **Discovery:** docs/discovery/clo-380.md
- **Linear:** https://linear.app/cloud-ai/issue/CLO-380/fr-3b-codex-o-output-last-message-authoritative-result-extraction
- **Branch:** `feat/clo-380-fr-3b-o-output-last-message`
- **Key assumptions carried forward:** `.pi/lessons/clo-373-codex-fxtures-lessons.md` (event semantics), `.pi/lessons/clo-374-sandbox-routing-lessons.md` (flag safety), `.pi/lessons/clo-371-stepcontext-migration-lessons.md` (private helper cohesion)

## Sub-tasks

### ST1 — Add diagnostics parser API to preserve token extraction when agent text is missing

**Files:** `src/backend/codex_event.rs`

Implement a diagnostics structure and helper while keeping `parse_jsonl_stream()` strict behavior for existing FR-3a users.

1. Add:
   ```rust
   #[derive(Debug, Default)]
   pub(crate) struct CodexStreamDiagnostics {
       pub agent_message: Option<String>,
       pub usage: Option<TokenUsage>,
       pub terminal_error: Option<BackendError>,
       pub parse_error: Option<BackendError>,
   }

   pub(crate) fn parse_jsonl_diagnostics(stream: &str) -> CodexStreamDiagnostics;
   ```
2. Move current `parse_jsonl_stream` event loop into `parse_jsonl_diagnostics`, and keep `parse_jsonl_stream` as a strict wrapper that maps:
   - missing/empty output -> `BackendError::Parse`
   - `turn.failed` / top-level `error` -> `BackendError::ExecutionFailed`
   - otherwise returns final `ParsedTurn`.
3. Add/adjust tests in `mod tests`:
   - `diagnostics_preserves_usage_when_agent_message_missing()` using `missing-agent-message.jsonl`.
   - `diagnostics_reports_turn_failed_as_terminal_error()` using `turn-failed.jsonl`.
   - Ensure existing FR-3a parser tests still pass unchanged.

**Acceptance:** `cargo test codex_event::tests::diagnostics_preserves_usage_when_agent_message_missing codex_event::tests::diagnostics_reports_turn_failed_as_terminal_error codex_event::tests::missing_agent_message_returns_parse_error`

**Estimate:** M

---

### ST2 — Wire `-o/--output-last-message` plumbing into Codex query path

**Files:** `src/backend/codex.rs`

1. Extend `build_argv_prefix()`:
   ```rust
   fn build_argv_prefix(
       base_args: &[String],
       sandbox: Option<super::SandboxMode>,
       model: Option<&str>,
       output_last_message_path: Option<&std::path::Path>,
   ) -> Vec<String>
   ```
   and append `-o <path>` when path is provided.
2. Add temp-file helpers:
   - `create_last_message_file() -> Result<NamedTempFile, BackendError>`
     (use `tempfile::Builder` prefix `lok-codex-last-` and return on failure as typed error).
   - `read_last_message(path: &Path) -> Option<String>` that reads trimmed output only from trailing CRLF, returning `None` for missing/empty/whitespace/unreadable files.
3. In `query()`:
   - create one temp file per invocation and keep it alive for full scope;
   - pass path through argv;
   - on success parse diagnostics, then choose text by precedence:
     `terminal_error` -> `Err(ExecutionFailed)`, else `output_last_message` -> fallback JSONL agent message -> parse_error.
   - always attach usage from diagnostics parse result when returning success output.
4. Keep failure precedence unchanged: any `turn.failed` / `error` in JSONL still maps to `ExecutionFailed` even if last-message contains text.

**Acceptance:** `cargo test codex::tests::codex_query_*` (after adding focused `codex` query-path unit tests in ST3)

**Estimate:** M

---

### ST3 — Add private/query-path unit tests for argv + temp-file lifecycle

**Files:** `src/backend/codex.rs`

Add tests that anchor deterministic behavior:

- `codex_argv_includes_output_last_message_when_path_given()` — path appears once.
- `codex_argv_omits_output_last_message_when_path_none()`.
- `codex_argv_orders_output_last_message_after_sandbox_and_model()`.
- `read_last_message_returns_none_for_missing_file()`.
- `read_last_message_returns_none_for_empty_or_whitespace_file()`.
- `read_last_message_preserves_leading_whitespace_and_trims_only_trailing_newlines()`.
- `named_tempfile_cleanup_removes_last_message_path()` proving RAII removal.

If needed, add helpers to keep tests robust against prompt/model changes and avoid duplicate assertions.

**Acceptance:** `cargo test codex::tests::codex_argv_includes_output_last_message_when_path_given codex::tests::read_last_message_preserves_leading_whitespace_and_trims_only_trailing_newlines codex::tests::named_tempfile_cleanup_removes_last_message_path`

**Estimate:** M

---

### ST4 — Extend fixtures and fixture validation for FR-3b scenarios

**Files:**
- `tests/fixtures/codex/*.jsonl`
- `tests/fixtures/codex/*.last-message.txt`
- `tests/fixtures/codex/README.md`
- `tests/codex_fixtures.rs`
- `tests/codex_parse_output.rs`

1. Add `.last-message.txt` companions:
   - `turn-completed.last-message.txt` (non-empty)
   - `missing-agent-message.last-message.txt` (non-empty)
   - `missing-last-message.last-message.txt` (empty) or dedicated empty companion path
   - `turn-failed.last-message.txt` (non-empty, precedence test)
2. Update fixture README inventory/provenance and explicitly document scrub status for new `.txt` files.
3. Extend `tests/codex_fixtures.rs` to include companion validation:
   - each `.jsonl` fixture used for FR-3b scenarios has expected `.last-message.txt` state (present/absent/empty);
   - preserve current sensitive-data scrub checks and extend guardrail to `.txt` companions.
4. Add/extend tests in `tests/codex_parse_output.rs` to verify precedence and fallback semantics by checking both streams:
   - populated `.last-message.txt` + valid JSONL => selected result = `.last-message.txt`;
   - populated `.last-message.txt` + `missing-agent-message.jsonl` => selected result = `.last-message.txt`, usage still read from JSONL;
   - empty/absent `.last-message` + valid JSONL => selected result = JSONL `item.completed` text;
   - populated `.last-message.txt` + `turn-failed.jsonl` => `BackendError::ExecutionFailed`.

**Acceptance:**
- `cargo test codex_fixtures
`
- `cargo test --test codex_parse_output`

**Estimate:** L (fixtures + dual-stream test matrix)

---

### ST5 — Run pre-merge gate and update follow-up risk notes

**Files:** all

Execute project-level gate and collect any incidental failures (especially async-io warnings and fixture size/scrub constraints).

**Acceptance:**
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Document any follow-up required if FR-3b surfaces cross-version `-o` behavior in real runs.

**Estimate:** S

## Pre-merge gate

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Older Codex binaries reject `-o` and change process behavior. | Low-medium | Keep the approach of returning the existing typed execution error path; add focused follow-up if this appears in CI/corps runs. |
| Missing-agent-message fixtures are fragile to Codex output drift. | Medium | Keep fixtures paired with README provenance, refresh only with verified captures, and retain strict FR-3a parser semantics as fallback signal. |
| Async file read in a hot path regresses latency. | Low | Use direct async read (`tokio::fs::read_to_string`) and keep parse logic minimal and bounded by fixture-size constraints. |
| Sensitive data accidentally ends in `.last-message.txt` companions. | Low | Reuse scrub + manual review pass for both `.jsonl` and `.txt` companions, with automated checks in fixture tests. |
