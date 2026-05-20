# Design: CLO-381 - FR-25: Codex backend extracts usage from turn.completed events

## Problem

Operators running Codex steps through `lok` lose visibility into prompt-cache savings and reasoning-token spend because the JSONL event parser at `src/backend/codex_event.rs:122` discards `cached_input_tokens` and `reasoning_output_tokens` when constructing `TokenUsage`. The upstream pieces are already in place: CLO-378 extended `TokenUsage` with `cached_tokens`/`reasoning_tokens` plus `with_cached`/`with_reasoning` builders (gated behind `#[allow(dead_code)]`), CLO-379 swapped the substring matcher for the event-driven parser, and CLO-373 captured fixtures (`tests/fixtures/codex/turn-completed.jsonl`, `multi-turn-reasoning.jsonl`) that already carry all four usage fields. Without this last hop, downstream consumers (run-summary, cost tracking in rs-wisper, future FR-28/29/30 aggregation) see `None` for cached/reasoning even when Codex emits them, which is the symptom this task closes.

## Goals / Non-goals

**Goals**

- Map `CodexUsage.cached_input_tokens` and `CodexUsage.reasoning_output_tokens` into `TokenUsage.cached_tokens` / `TokenUsage.reasoning_tokens` inside `parse_jsonl_stream`'s `TurnCompleted` handler at `src/backend/codex_event.rs:121-129`.
- Drop the `#[allow(dead_code)]` attributes on `TokenUsage::with_cached` (`src/backend/mod.rs:146`) and `TokenUsage::with_reasoning` (`src/backend/mod.rs:154`), since they now have a live call site.
- Drop the `#[allow(dead_code)]` attribute on `struct CodexUsage` (`src/backend/codex_event.rs:61`) once its `cached_input_tokens` and `reasoning_output_tokens` fields are read.
- Extend the existing fixture-backed parser tests so that `cached_tokens` and `reasoning_tokens` are asserted on `turn-completed.jsonl` (expect `Some(7552)` / `Some(0)`) and `multi-turn-reasoning.jsonl` (expect `Some(30464)` / `Some(51)`).
- Pass the existing pre-merge gate (`cargo fmt --check && cargo clippy -- -D warnings && cargo test`) unchanged.

**Non-goals**

- Changing `total_tokens` semantics. `cached_tokens` and `reasoning_tokens` remain excluded from the total, matching the contract documented at `src/backend/mod.rs:113-125`.
- Multi-turn accumulation. The parser keeps last-turn-take semantics; switching to running sum is the documented discovery debt and stays out of scope.
- Adding a `From<CodexUsage> for TokenUsage` impl (rejected Approach B from discovery: extra type with no current reuse).
- Touching the Codex `Backend::query` wiring in `src/backend/codex.rs`. `QueryOutput.usage` already gets the parsed value via `with_usage`; the only data being added is on the `TokenUsage` it already carries.
- Changing `StepResult.usage` plumbing, run-summary output, or JSON formatting. Those are FR-25a / FR-28..30 territory and are tracked separately.

## Architecture

The change is local to the parser. The data flow stays identical; only the constructor call inside the `TurnCompleted` arm grows two builder calls.

```
Codex CLI stdout (JSONL)
  -> src/backend/codex_event.rs::parse_jsonl_stream
       -> serde_json::from_str -> CodexEvent::TurnCompleted { usage: Option<CodexUsage> }
       -> usage.map(|u| TokenUsage::new(u.input_tokens, u.output_tokens)
                        .with_cached(u.cached_input_tokens)
                        .with_reasoning(u.reasoning_output_tokens))
       -> ParsedTurn { usage: Option<TokenUsage>, agent_message }
  -> src/backend/codex.rs::CodexBackend::query
       -> QueryOutput::with_usage(parsed.usage)        (unchanged)
  -> StepResult.usage / run summary                    (unchanged, downstream)
```

No new modules, no new types, no signature changes on `Backend` or `QueryOutput`. The four fields of `CodexUsage` (`input_tokens`, `cached_input_tokens`, `output_tokens`, `reasoning_output_tokens`) at `src/backend/codex_event.rs:61-72` are already deserialized; only the last two transition from "read once and discarded" to "forwarded to `TokenUsage`".

Encoding choice for missing fields: `serde(default)` on `Option<u32>` fields means a JSONL line that omits `cached_input_tokens` or `reasoning_output_tokens` deserializes them as `None`. When present, they deserialize as `Some(value)`. This honours the `TokenUsage` contract ("`None` when not reported") and matches the issue spec: "`cached_tokens` and `reasoning_tokens` populated when Codex emits them; `None` when omitted."

## Public API surface

The crate-public surface does not change. `TokenUsage`, `CodexUsage`, `ParsedTurn`, and `parse_jsonl_stream` keep their current signatures. The only diffs are (a) the body of one match arm and (b) removed `#[allow(dead_code)]` attributes.

Before, at `src/backend/codex_event.rs:121-129`:

```rust
CodexEvent::TurnCompleted { usage } => {
    let token_usage = usage.map(|u| TokenUsage::new(u.input_tokens, u.output_tokens));
    last_completed_turn = ParsedTurn {
        agent_message: current_turn_agent_message.clone(),
        usage: token_usage,
    };
    current_turn_agent_message = None;
}
```

After:

```rust
CodexEvent::TurnCompleted { usage } => {
    let token_usage = usage.map(|u| {
        TokenUsage::new(u.input_tokens, u.output_tokens)
            .with_cached(u.cached_input_tokens)
            .with_reasoning(u.reasoning_output_tokens)
    });
    last_completed_turn = ParsedTurn {
        agent_message: current_turn_agent_message.clone(),
        usage: token_usage,
    };
    current_turn_agent_message = None;
}
```

`CodexUsage` field types change (enables `None`-when-absent contract):

Before:
```rust
#[serde(default)] pub cached_input_tokens: u32,
#[serde(default)] pub reasoning_output_tokens: u32,
```

After:
```rust
#[serde(default)] pub cached_input_tokens: Option<u32>,
#[serde(default)] pub reasoning_output_tokens: Option<u32>,
```

Builder signatures referenced (defined in `src/backend/mod.rs:147` and `src/backend/mod.rs:155`, unchanged - the only edit is the attribute):

```rust
pub fn with_cached(mut self, cached: Option<u32>) -> Self { /* ... */ }
pub fn with_reasoning(mut self, reasoning: Option<u32>) -> Self { /* ... */ }
```

Attribute removals (cosmetic but mandatory for `-D warnings` cleanliness once the call site lights up):

```rust
// src/backend/codex_event.rs:61 - remove the #[allow(dead_code)] above `pub(super) struct CodexUsage`.
// src/backend/mod.rs:146       - remove the #[allow(dead_code)] above `pub fn with_cached`.
// src/backend/mod.rs:154       - remove the #[allow(dead_code)] above `pub fn with_reasoning`.
```

## Assumptions

- **Confidence: high** - Codex's `turn.completed.usage` ships `cached_input_tokens` and `reasoning_output_tokens` as plain `u32` token counts. Verified directly against `tests/fixtures/codex/turn-completed.jsonl:4` (`"cached_input_tokens":7552`, `"reasoning_output_tokens":0`) and `tests/fixtures/codex/multi-turn-reasoning.jsonl:6` (`30464` / `51`). Verification path: re-run those fixtures through `parse_jsonl_stream` in the new tests.
- **Confidence: high** - `cached_tokens` and `reasoning_tokens` are NOT counted into `total_tokens`; that contract is fixed by CLO-378. Verified at `src/backend/mod.rs:113-125` doc comments and `src/backend/mod.rs:138` (`total_tokens` is `prompt + completion` only). Verification path: existing tests `test_token_usage_with_cached_sets_field` and `test_token_usage_with_reasoning_sets_field` in `src/backend/mod.rs`.
- **Confidence: high** - Codex reports cumulative-per-turn usage (each `turn.completed` repeats the running total), so taking the last turn's value is equivalent to summing across turns for cached/reasoning as well as for prompt/completion. Verification path: see discovery doc `docs/discovery/clo-381.md` "Multi-turn semantics" and discovery debt; would need a real multi-turn capture to disprove.
- **Confidence: high** - When Codex omits `cached_input_tokens` or `reasoning_output_tokens` from a `turn.completed.usage` object, `serde(default)` on `Option<u32>` produces `None`, which passes through to `TokenUsage.cached_tokens` / `TokenUsage.reasoning_tokens` as `None`. This matches the `TokenUsage` contract that those fields are `None` "when not reported." Verification path: a new unit test feeding `{"input_tokens":10,"output_tokens":7}` and asserting `result.usage.unwrap().cached_tokens == None && reasoning_tokens == None`.
- **Confidence: high** - No downstream consumer currently branches on `cached_tokens.is_none()` vs. `Some(0)`. Verified via `rg "cached_tokens|reasoning_tokens"` over `src/`; only definitions, builders, summation, and tests appear. Verification path: same `rg` after the change to confirm no new None-sensitive callers were added.

## Test plan

**Unit tests (added to the existing `mod tests` block in `src/backend/codex_event.rs`)**

- `turn_completed_with_cached_and_reasoning_populates_token_usage` - feed an inline JSONL stream with `cached_input_tokens: 5`, `reasoning_output_tokens: 3`; assert `result.usage.unwrap().cached_tokens == Some(5)` and `reasoning_tokens == Some(3)` while `prompt_tokens`/`completion_tokens` stay at the existing values.
- `turn_completed_omitting_cached_and_reasoning_yields_none` - feed `{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":7}}`; assert `result.usage.unwrap().cached_tokens == None && reasoning_tokens == None`.
- `turn_completed_reasoning_zero_is_some_not_none` - feed `"reasoning_output_tokens":0` (present, value zero); assert `result.usage.unwrap().reasoning_tokens == Some(0)` to verify the distinction from absent-field → `None`.
- `multi_turn_last_turn_cached_and_reasoning_win` - extend the existing `multi_turn_returns_last_agent_message` shape with two `turn.completed` events carrying different cached/reasoning values; assert the second turn's values are surfaced.

**Fixture tests (modified in `src/backend/codex_event.rs`)**

- Extend `fixture_turn_completed_returns_happy_path_message` to additionally assert `usage.cached_tokens == Some(7552)` and `usage.reasoning_tokens == Some(0)`.
- Extend `fixture_multi_turn_reasoning_returns_only_final_agent_message` to additionally assert `usage.cached_tokens == Some(30464)` and `usage.reasoning_tokens == Some(51)`.

**Existing tests that must keep passing unchanged**

- All existing unit tests under `src/backend/codex_event.rs::tests` (the parser's contract for thread/turn lifecycle, error mapping, malformed-line skipping, truncated-stream handling).
- The full `TokenUsage` test suite in `src/backend/mod.rs::tests` (build, sum, saturate, with_cached/with_reasoning idempotence).
- `src/backend/codex.rs` integration with `QueryOutput::with_usage`; no change is expected, but the test should still go green.

**Per-backend matrix (regression pin: only Codex is in scope; the others must keep behaving)**

| Backend | Where `TokenUsage` is built | Expected after this change |
|---|---|---|
| Codex (CLI, JSONL) | `src/backend/codex_event.rs::parse_jsonl_stream` | `cached_tokens` and `reasoning_tokens` populated from Codex `usage` block. |
| Claude API | `src/backend/claude.rs` (CLO-378 already lands cached). | Unchanged. Cached is filled by claude.rs; reasoning stays `None` (Anthropic does not emit it). |
| Ollama | `src/backend/ollama.rs` | Unchanged. Both fields stay `None`. |
| Bedrock | `src/backend/bedrock.rs` | Unchanged. Both fields stay `None`. |
| Gemini | `src/backend/gemini.rs` | Unchanged. Both fields stay `None`. |

**Manual verification**

1. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` - the pre-merge gate.
2. `cargo test --test '*' codex` (or equivalent filter) - run the Codex fixture tests in isolation to confirm they assert the new fields.
3. Replay a real Codex run through `lok run --output json` against a minimal workflow that exercises a single Codex step; confirm the JSON envelope's `step.usage` for the Codex step carries non-null `cached_tokens` and `reasoning_tokens` (note: this end-to-end JSON exposure depends on FR-25a having already landed - if not, the verification reduces to dropping a `dbg!` on `QueryOutput.usage` inside `CodexBackend::query` for a one-off local run).

## Migration / rollout

This is purely additive at the API level: `TokenUsage` already exposes `cached_tokens` and `reasoning_tokens` since CLO-378 and `QueryOutput.usage` already carries `Option<TokenUsage>`. No types change, no fields are removed, no breakage to rs-wisper (the only external Rust consumer, pinned via crate dependency).

- No feature flag required. The fields go from "always `None` for Codex" to "always populated when Codex emits them"; the change is observable but compatible.
- No deprecations. The three `#[allow(dead_code)]` attributes that come off (`CodexUsage`, `with_cached`, `with_reasoning`) are an internal cleanup with no API impact.
- Rollout order: ships as a single PR. Pre-merge gate is the existing `cargo fmt --check && cargo clippy -- -D warnings && cargo test`. No data backfill, no migration script, no config flag flips.
- Downstream effect: `lok run --output json` will start showing non-null `cached_tokens`/`reasoning_tokens` on Codex `step.usage` entries once FR-25a is also live. If FR-25a is not yet live in the target branch, the data still flows to `QueryOutput.usage` but stops at the `StepResult` boundary (the documented FR-25a gap).

## Open questions

- **Multi-turn delta vs. cumulative reporting (carried from discovery debt).** If a future Codex release switches from cumulative-per-turn to delta-per-turn usage, the last-turn-take strategy understates cached/reasoning totals. Today's evidence supports cumulative; no action until a real fixture proves otherwise. Tracked here rather than re-opened as a follow-up because the parser, not just this field mapping, would need to change.
