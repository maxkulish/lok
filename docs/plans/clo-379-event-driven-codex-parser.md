# Plan: CLO-379 FR-3a: event-driven Codex JSONL parser (replace substring match)

## Context
- Design: docs/designs/clo-379-event-driven-codex-parser.md
- Discovery: docs/discovery/clo-379.md
- Linear: https://linear.app/cloud-ai/issue/CLO-379/fr-3a-event-driven-codex-jsonl-parser-replace-substring-match

## Sub-tasks

### ST1 Create `codex_event.rs` with serde types and parser state machine
**Files:** `src/backend/codex_event.rs`, `src/backend/mod.rs` (add `mod codex_event;`)
**Acceptance:** `cargo check` passes with no new warnings; new module compiles.
**Estimate:** S

Define `CodexEvent` (`#[serde(tag = "type")]`), `CodexItem`, `CodexUsage`, `CodexError`, and `ParsedTurn` structs. Implement `parse_jsonl_stream` as a state machine that:
- Accumulates `agent_message` text per turn via `item.completed`
- Commits to `last_completed_turn` on `turn.completed`
- Returns `Err(BackendError::ExecutionFailed)` on `turn.failed` and `error`
- Returns `Err(BackendError::Parse)` when stream ends without `turn.completed`
- Skips unknown events at `debug!` level

Unit tests in the same file: parse individual events, unknown variants, happy path, multi-turn, turn-failed, missing-agent-message, truncated stream, unparseable line skipped.

### ST2 Refactor `CodexBackend::parse_output` to use event parser and wire through `query`
**Files:** `src/backend/codex.rs`
**Acceptance:** `cargo test backend::codex` passes (existing + new tests); no `line.contains` remains in the parse path.
**Estimate:** S

Change `parse_output` signature from `fn(&self, &str) -> String` to `fn(&self, &str) -> Result<ParsedTurn, BackendError>`. In `query()`, propagate the error with `?` and build `QueryOutput` with `parsed.agent_message` and `.with_usage(parsed.usage)`.

Add unit tests that use fixture strings embedded in the test module (no external files for `#[cfg(test)]` inside `codex.rs`):
- happy_path fixture text (single turn)
- multi-turn returns last agent_message
- turn_failed surfaces ExecutionFailed
- missing_agent_message surfaces Parse
- unknown event variant does not break parsing

### ST3 Add integration tests against on-disk fixtures
**Files:** `tests/codex_parse_output.rs` (new)
**Acceptance:** `cargo test --test codex_parse_output` passes.
**Estimate:** S

Read each fixture from `tests/fixtures/codex/` and assert:
- `turn-completed.jsonl` → Ok with agent_message text
- `multi-turn-reasoning.jsonl` → Ok with final agent_message, non-zero reasoning tokens in usage
- `turn-failed.jsonl` → Err(ExecutionFailed)
- `missing-agent-message.jsonl` → Err(Parse)

Import `parse_jsonl_stream` via `pub(crate)` re-export in `src/backend/mod.rs`.

### ST4 Clean clippy and pre-merge gate
**Files:** any touched in ST1–ST3
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
**Estimate:** S

Address any clippy lints, formatting drift, or test failures. Ensure no `line.contains("...")` heuristics remain in `codex.rs`.

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## Risks
- `#[serde(other)]` on internally-tagged enum: verified with standalone test crate (CodexItem::Other round-trips for unknown item types; CodexEvent::Unknown for unknown event types).
- Missing event types in fixtures: the four FR-40 fixtures cover all types the PRD requires; `#[serde(other)]` provides forward compat.
- `BackendError` mapping: discovery chose `ExecutionFailed` over a new `Backend` variant to keep this PR additive.
