# Design: CLO-379 - FR-3a: event-driven Codex JSONL parser (replace substring match)

## Problem

`CodexBackend::parse_output` in `src/backend/codex.rs:74` extracts the agent's final response by substring-matching each JSONL line for `"\"type\":\"item.completed\""` and `agent_message`. The discovery report scores this 2/10: it returns the first `agent_message` anywhere in the stream (wrong on multi-turn or interleaved reasoning), silently swallows `turn.failed` and top-level `error` events by falling back to raw stdout, cannot distinguish a missing `agent_message` from a real reply, and discards `usage` tokens entirely. The trigger is that CLO-373 / FR-40 landed real Codex JSONL fixtures (`tests/fixtures/codex/*.jsonl`) on `main`, so we can now pin a robust event-driven parser against recorded streams. PRD `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §4 FR-3a marks this a `Must`, and FR-25 (token usage) plus FR-3b (`--output-last-message`) build directly on the same parser.

## Goals / Non-goals

**Goals**

- Replace `parse_output`'s substring matcher with an event-driven parser over the Codex JSONL event model: `thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.started`, `item.completed`, `error`.
- Extract the agent text from the `agent_message` `item.completed` whose accumulation closed at the **last** `turn.completed`, per FR-3a.
- Surface `turn.failed` and stream-level `error` events as `BackendError`, instead of falling back to raw stdout.
- Surface a `BackendError::Parse` when the stream ends without any `turn.completed`.
- Extract `usage` (`input_tokens`, `cached_input_tokens`, `output_tokens`, `reasoning_output_tokens`) from the last `turn.completed` and attach it to `QueryOutput.usage` so FR-25 lands incrementally.
- Forward-compatible to new Codex event / item types: unknown variants log at `debug!` and are skipped, never abort parsing.
- Wire the parser through `CodexBackend::query` without changing the trait signature in `src/backend/mod.rs`.
- Cover the parser with unit tests under `src/backend/codex.rs` and consume all four existing fixtures in a new `tests/codex_parse_output.rs` integration test.

**Non-goals**

- FR-3b (`-o` / `--output-last-message` as authoritative result). Out of scope per discovery and PRD.
- Schema / structured-output extraction (FR-3). The parser returns raw text; structured JSON is `workflow::extract_json_from_text`'s job.
- Adding a new `BackendError::Backend` variant. Discovery decided to reuse `ExecutionFailed { message, exit_code: None }` to keep this PR additive (tracked as an open question below).
- Re-plumbing `QueryOutput.usage` into `StepResult.usage` (that is FR-25a / FR-25b in a separate ticket).
- Touching any other backend (`claude`, `gemini`, `ollama`, `bedrock`).
- Streaming line-at-a-time during process I/O. `CodexBackend::query` still reads stdout to completion via `cmd.output().await`; we parse the full buffer.

## Architecture

All new types live in one new private module `src/backend/codex_event.rs`. `src/backend/codex.rs` consumes it via `mod codex_event; use codex_event::*;`.

```
src/backend/
  codex.rs           # CodexBackend, query(), build_argv_prefix(), uses ParsedTurn
  codex_event.rs     # NEW: CodexEvent, CodexItem, ParsedTurn, parse_jsonl_stream
  mod.rs             # unchanged (BackendError variants reused)
```

Data flow inside `CodexBackend::query` (unchanged shell; new payload):

```
stdout: String
   |
   v
codex_event::parse_jsonl_stream(&stdout)
   |
   |  for each non-empty line:
   |    serde_json::from_str::<CodexEvent>(line)
   |      Ok(Unknown)        -> debug!, continue
   |      Ok(typed event)    -> drive state machine
   |      Err(parse error)   -> debug!, continue (line skipped, not fatal)
   |
   |  state machine per turn:
   |    on item.completed{agent_message} -> stash text in `current_turn_agent_message`
   |    on turn.completed                 -> commit (current_turn_agent_message, usage)
   |                                        to `last_completed_turn`; reset current
   |    on turn.failed                    -> return Err(ExecutionFailed { message: error.message, exit_code: None })
   |    on error                          -> return Err(ExecutionFailed { message: event.message, exit_code: None })
   |
   v
Result<ParsedTurn, BackendError>
   |
   |  ParsedTurn { agent_message: Option<String>, usage: Option<TokenUsage> }
   |
   |  - if no turn.completed observed: Err(BackendError::Parse)
   |  - if turn.completed observed but agent_message absent for that turn:
   |       agent_message = None  (PRD: "missing agent_message" fixture is a Parse error)
   |       -> Err(BackendError::Parse { message: "turn.completed without agent_message" })
   |  - otherwise Ok(ParsedTurn { agent_message: Some(text), usage })
   v
QueryOutput::from_process(parsed_text, stderr, exit_code, "codex", elapsed)
    .with_model(effective_model)
    .with_usage(parsed.usage)
```

State machine notes:

- `current_turn_agent_message` is overwritten by every `item.completed{agent_message}` within the turn, so if a turn legitimately contains multiple agent messages we keep the last (matches Codex's own "last reply wins" semantics).
- `last_completed_turn` is overwritten on every `turn.completed`, so the parser naturally returns the agent message from the final turn even if intermediate turns emitted their own.
- Tool / reasoning items (`command_execution`, `reasoning`, anything that is not `agent_message`) flow through the `CodexItem::Other` arm and are ignored for extraction.
- `error` is treated as terminal because the `turn-failed.jsonl` fixture shows it immediately preceding `turn.failed`. If `error` arrives without a subsequent `turn.failed`, returning early is still correct: there is no useful agent text to recover.

Concrete Rust types (signatures in the next section). `CodexEvent` and `CodexItem` are `#[serde(tag = "type", rename_all = "snake_case")]` enums with `#[serde(other)]` arms for forward compatibility. `ParsedTurn` is the parser's return shape and is not exported beyond the `backend` module.

## Public API surface

All names are `pub(crate)` or private. Nothing crosses the crate boundary; `Backend` trait stays as it is in `src/backend/mod.rs:235`.

**`src/backend/codex_event.rs` (new)**

```rust
use serde::Deserialize;

use crate::backend::{BackendError, TokenUsage};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum CodexEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted {},

    #[serde(rename = "turn.started")]
    TurnStarted {},

    #[serde(rename = "turn.completed")]
    TurnCompleted {
        #[serde(default)]
        usage: Option<CodexUsage>,
    },

    #[serde(rename = "turn.failed")]
    TurnFailed {
        #[serde(default)]
        error: Option<CodexError>,
    },

    #[serde(rename = "item.started")]
    ItemStarted {
        #[serde(default)]
        item: Option<CodexItem>,
    },

    #[serde(rename = "item.completed")]
    ItemCompleted {
        #[serde(default)]
        item: Option<CodexItem>,
    },

    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: Option<String>,
    },

    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum CodexItem {
    AgentMessage {
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub(super) struct CodexUsage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub cached_input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub reasoning_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct CodexError {
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct ParsedTurn {
    pub agent_message: String,
    pub usage: Option<TokenUsage>,
}

pub(super) fn parse_jsonl_stream(stream: &str) -> Result<ParsedTurn, BackendError>;
```

**`src/backend/codex.rs` (changed)**

Before:

```rust
fn parse_output(&self, output: &str) -> String { /* substring match */ }

// in query():
let parsed_stdout = self.parse_output(&stdout);
Ok(super::QueryOutput::from_process(parsed_stdout, stderr_str, exit_code, "codex", start.elapsed())
    .with_model(effective_model))
```

After:

```rust
fn parse_output(&self, output: &str) -> Result<codex_event::ParsedTurn, super::BackendError> {
    codex_event::parse_jsonl_stream(output)
}

// in query():
let parsed = self.parse_output(&stdout)?;
Ok(super::QueryOutput::from_process(parsed.agent_message, stderr_str, exit_code, "codex", start.elapsed())
    .with_model(effective_model)
    .with_usage(parsed.usage))
```

`parse_output` keeps `&self` for symmetry with the existing method even though `parse_jsonl_stream` is free-standing; this avoids a wider refactor and lets future per-backend config (e.g., strictness toggles) thread through one place.

## Assumptions

- **A1** (high): The four existing fixtures in `tests/fixtures/codex/` (`turn-completed.jsonl`, `turn-failed.jsonl`, `multi-turn-reasoning.jsonl`, `missing-agent-message.jsonl`) faithfully represent the Codex 0.130.0 wire format and exhaust the event types FR-3a requires. Verification: `cargo test --test codex_fixtures` already passes; new parser tests will consume the same files.
- **A2** (high): Reusing `BackendError::ExecutionFailed { message, exit_code: None }` for `turn.failed` and stream `error` events is the correct mapping. Verification: discovery report §"Approach 1" recorded this choice; reviewers can flag if a new `Backend` variant is preferred (see Open questions).
- **A3** (medium): The PRD's "missing `agent_message` → `BackendError::Parse`" rule applies when `turn.completed` is observed but the turn contained no `agent_message` item. The `missing-agent-message.jsonl` fixture shows exactly this shape (no `item.completed` between `turn.started` and `turn.completed`). Verification: integration test asserts `Err(Parse)` for that fixture.
- **A4** (medium): The Codex stream always emits exactly one terminal event per invocation (`turn.completed`, `turn.failed`, or top-level `error`). If a stream ends mid-turn with no terminal, the parser treats it as `BackendError::Parse`. Verification: existing fixture coverage; we add a synthetic "truncated" unit test.
- **A5** (low): Tool-related items (`command_execution`, etc.) and `reasoning` items will continue to share the `item.completed` envelope with a `type` discriminator on the inner `item` object. Verification: `multi-turn-reasoning.jsonl` shows `command_execution` items; the `CodexItem::Other` `#[serde(other)]` arm is the forward-compat hatch if the envelope ever shifts.
- **A6** (low): Codex never emits more than one `agent_message` per turn in current versions. The state machine handles the multi-`agent_message` case (last wins) defensively but is not exercised by fixtures. Verification: covered by a synthetic unit test (`agent_message_multiple_in_turn_returns_last`).

## Test plan

**Unit tests in `src/backend/codex_event.rs` (`#[cfg(test)] mod tests`)**

| Test fn | Asserts |
|---|---|
| `parses_thread_started` | `CodexEvent::ThreadStarted` round-trips from `{"type":"thread.started", ...}` |
| `parses_turn_completed_with_usage` | `CodexEvent::TurnCompleted { usage: Some(_) }` with all four token fields |
| `parses_turn_failed_with_error_message` | `CodexEvent::TurnFailed { error: Some(msg) }` |
| `parses_item_completed_agent_message` | `CodexItem::AgentMessage { text: Some("...") }` |
| `parses_item_completed_other_kind_as_other` | `command_execution` deserializes to `CodexItem::Other` |
| `unknown_event_type_falls_through_to_unknown` | `{"type":"future.event"}` deserializes to `CodexEvent::Unknown` |
| `unknown_item_type_falls_through_to_other` | `{"type":"item.completed","item":{"type":"future_item"}}` is `Unknown` envelope safe |
| `happy_path_returns_last_agent_message` | Single-turn stream returns `ParsedTurn { agent_message: "fixture happy path", usage: Some(_) }` |
| `multi_turn_returns_last_agent_message` | Stream with two `turn.completed` events keeps the second turn's message |
| `turn_failed_returns_execution_failed` | `parse_jsonl_stream` returns `Err(BackendError::ExecutionFailed { exit_code: None, .. })` and message contains the inner error text |
| `top_level_error_returns_execution_failed` | Stream with only a top-level `error` event surfaces as `ExecutionFailed` |
| `missing_agent_message_returns_parse_error` | `turn.completed` without an `agent_message` item → `Err(BackendError::Parse)` |
| `truncated_stream_returns_parse_error` | No terminal event → `Err(BackendError::Parse)` |
| `unparseable_line_skipped_then_succeeds` | A garbage line between valid events does not abort parsing |
| `agent_message_multiple_in_turn_returns_last` | Two `item.completed{agent_message}` in one turn → second text wins |
| `usage_extracted_to_token_usage` | `TokenUsage::new(input_tokens, output_tokens)` populated; `cached_input_tokens` and `reasoning_output_tokens` are accepted by serde even though `TokenUsage` does not surface them yet (forward path for FR-25b) |

**Integration test `tests/codex_parse_output.rs` (new file)**

Drives the parser against each on-disk fixture exactly as `tests/codex_fixtures.rs` does for shape validation, but asserts parser output:

```rust
#[test] fn fixture_turn_completed_returns_happy_path_message();          // -> "fixture happy path"
#[test] fn fixture_multi_turn_reasoning_returns_only_final_agent_message(); // -> "323"
#[test] fn fixture_turn_failed_returns_execution_failed();
#[test] fn fixture_missing_agent_message_returns_parse_error();
```

These tests call `codex_event::parse_jsonl_stream` directly (re-exported `pub(crate)` for tests via `#[cfg(test)] pub use`) rather than spawning the `codex` binary, so the suite stays hermetic.

**Per-backend test matrix**

| Backend | Touched by this PR | Required test coverage |
|---|---|---|
| `codex` | Yes (rewrite) | All tests above |
| `claude` | No | None new |
| `gemini` | No | None new |
| `ollama` | No | None new |
| `bedrock` | No | None new |

**Manual verification steps**

1. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` is green.
2. `codex exec --json --ephemeral -s read-only -- "say hi"` run locally; confirm `lok ask --backend codex "say hi"` returns the agent text only (no JSONL leaks into stdout).
3. Force a failure with a bad model: `codex exec --json --ephemeral -s read-only --model definitely-not-real -- "hi"`; confirm `lok` surfaces a non-zero result and the error message contains the upstream `"model is not supported"` text rather than the raw JSONL.
4. Inspect `QueryOutput.usage` via `lok run --output json` on a Codex step to confirm `prompt_tokens` / `completion_tokens` are populated.

## Migration / rollout

- Purely internal refactor inside `src/backend/codex.rs` plus one new private sibling module `src/backend/codex_event.rs`. No public crate API changes, no `Backend` trait changes, no config schema changes, no CLI flag changes.
- `QueryOutput.stdout` semantics for the Codex backend tighten: callers used to receive raw JSONL when extraction failed; now they receive a `BackendError`. This is the intended behavior change (and the whole reason the ticket exists). The retry layer (`RetryExecutor` in `src/backend/retry.rs`) will see a non-retryable `Parse` / `ExecutionFailed`, which matches `BackendError::is_retryable` returning `false` for both. No feature flag is needed because the old behavior was a latent bug, not a contract.
- No external dependencies added. `serde`, `serde_json`, `thiserror`, and `tokio` are already in `Cargo.toml`.
- Rollout order: single PR. CLO-373 / FR-40 fixtures are already on `main`, so this PR has no upstream blockers. FR-25 (full `cached_tokens` / `reasoning_tokens` surfacing) and FR-3b (`--output-last-message`) build on this work in later PRs but are independent.

## Open questions

- **Q1: Add `BackendError::Backend` variant or reuse `ExecutionFailed`?** Discovery chose `ExecutionFailed { exit_code: None }` to keep this PR additive; the PRD/ticket prose says `BackendError::Backend`. Reusing `ExecutionFailed` muddies retry semantics (the variant is also produced by non-zero CLI exits and by the `From<anyhow::Error>` fallback in `src/backend/mod.rs:95`); a dedicated `Backend { message }` variant would let `retry::is_retryable` and any future error UI distinguish "Codex reported an internal failure" from "Codex exited non-zero". Tradeoff: one-line enum addition plus a `matches!` arm in `is_retryable` vs. zero churn. Recommended resolution: keep `ExecutionFailed` for this PR (per discovery) and revisit when FR-25 / FR-3b touch this code again. Discovery-approved choice; do not change without flagging.
- **Q2: `BackendError::Parse` vs. `BackendError::ExecutionFailed` for the "stream ended without `turn.completed`" case (A4).** Treating truncation as `Parse` matches the PRD; treating it as `ExecutionFailed` would be retry-friendlier if Codex stream truncations turn out to be transient (e.g., dropped pipe). No fixture exercises this today. Resolution pending the first real-world occurrence.
- **Q3: How much of `CodexUsage` should land in `TokenUsage` in this PR?** `TokenUsage::new(prompt, completion)` only surfaces two fields; `cached_input_tokens` and `reasoning_output_tokens` need FR-25b's `TokenUsage` extension before they can be exposed. Options: (a) populate only `prompt_tokens` / `completion_tokens` now and accept that cached/reasoning data is parsed but dropped; (b) defer `with_usage` entirely until FR-25b lands. Recommended (a) for incremental value and to validate the parser end-to-end; explicit decision needed during review.
