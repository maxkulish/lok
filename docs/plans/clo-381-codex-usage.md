# Plan: CLO-381 — FR-25: Codex backend extracts `usage` from `turn.completed` events

## Context

- **Design**: docs/designs/clo-381-codex-usage.md
- **Discovery**: docs/discovery/clo-381.md
- **Linear**: https://linear.app/cloud-ai/issue/CLO-381/fr-25-codex-backend-extracts-usage-from-turncompleted-events
- **Branch**: `feat/clo-381-codex-usage`
- **Lessons**: `.pi/lessons/clo-378-tokenusage-extension-lessons.md` L1 (dead_code removal safe once call site exists)

## Sub-tasks

### ST1 — Change `CodexUsage` cached/reasoning fields to `Option<u32>`

**Files:** `src/backend/codex_event.rs`

Change two field types on `pub(super) struct CodexUsage`:

```rust
// Before
#[serde(default)] pub cached_input_tokens: u32,
#[serde(default)] pub reasoning_output_tokens: u32,

// After
#[serde(default)] pub cached_input_tokens: Option<u32>,
#[serde(default)] pub reasoning_output_tokens: Option<u32>,
```

Remove `#[allow(dead_code)]` from `pub(super) struct CodexUsage` (line 61).

**Acceptance:** `cargo build` compiles without dead_code warnings on `CodexUsage`.

**Estimate:** S (2 field types, 1 attribute removal)

---

### ST2 — Wire `with_cached` / `with_reasoning` in parse_jsonl_stream

**Files:** `src/backend/codex_event.rs`

Update the `TurnCompleted` handler body from:

```rust
let token_usage = usage.map(|u| TokenUsage::new(u.input_tokens, u.output_tokens));
```

to:

```rust
let token_usage = usage.map(|u| {
    TokenUsage::new(u.input_tokens, u.output_tokens)
        .with_cached(u.cached_input_tokens)
        .with_reasoning(u.reasoning_output_tokens)
});
```

Remove `#[allow(dead_code)]` from `TokenUsage::with_cached` (`src/backend/mod.rs:146`) and `TokenUsage::with_reasoning` (`src/backend/mod.rs:154`).

**Acceptance:** `cargo build` compiles without dead_code warnings on `with_cached`/`with_reasoning`. Lesson L1 confirmed: CLO-381 is the first caller, so suppression removal is safe.

**Estimate:** S (1 match arm body, 2 attribute removals)

---

### ST3 — Add unit tests for cached/reasoning mapping

**Files:** `src/backend/codex_event.rs` (mod tests block)

Add 4 new test functions:

1. **`turn_completed_with_cached_and_reasoning_populates_token_usage`** — inline JSONL with `cached_input_tokens: 5`, `reasoning_output_tokens: 3`. Assert `cached_tokens == Some(5)`, `reasoning_tokens == Some(3)`, `prompt_tokens`/`completion_tokens` unchanged.

2. **`turn_completed_omitting_cached_and_reasoning_yields_none`** — inline JSONL with `{"input_tokens":10,"output_tokens":7}` (no cached/reasoning). Assert `cached_tokens == None`, `reasoning_tokens == None`.

3. **`turn_completed_reasoning_zero_is_some_not_none`** — inline JSONL with `"reasoning_output_tokens":0` (present but zero). Assert `reasoning_tokens == Some(0)`. Verifies the `Some(0)` vs `None` boundary.

4. **`multi_turn_last_turn_cached_and_reasoning_win`** — two `turn.completed` events with different cached/reasoning values. Assert second turn's values are surfaced.

**Acceptance:** `cargo test codex_event` — all 4 new tests pass.

**Estimate:** M (4 test functions, 2 with multi-line JSONL fixtures)

---

### ST4 — Extend fixture tests with cached/reasoning assertions

**Files:** `src/backend/codex_event.rs` (existing fixture tests)

Extend two existing fixture tests:

1. **`fixture_turn_completed_returns_happy_path_message`** — add assertions:
   - `usage.cached_tokens == Some(7552)` (from `turn-completed.jsonl`)
   - `usage.reasoning_tokens == Some(0)`

2. **`fixture_multi_turn_reasoning_returns_only_final_agent_message`** — add assertions:
   - `usage.cached_tokens == Some(30464)` (from `multi-turn-reasoning.jsonl`)
   - `usage.reasoning_tokens == Some(51)`

**Acceptance:** `cargo test codex_event::tests::fixture_turn_completed` and `cargo test codex_event::tests::fixture_multi_turn_reasoning` — both pass with new assertions.

**Estimate:** S (2 tests, 2 assert lines each)

---

### ST5 — Pre-merge gate and regression check

**Files:** All project files

Run the full pre-merge gate:

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Key regression areas (no behavioral change expected):
- All existing `src/backend/codex_event.rs` tests pass unchanged.
- `src/backend/mod.rs::tests` (TokenUsage tests) pass unchanged.
- `src/backend/claude.rs`, `ollama.rs`, `bedrock.rs`, `gemini.rs` — no code changed, but confirm `cargo test` covers all backends.

**Acceptance:** exit code 0 across all three gates.

**Estimate:** S (run 3 commands)

## Pre-merge gate

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| `Some(0)` for reported-zero reasoning tokens confuses downstream consumers that check `is_some()` to mean "Codex reported this" | Low | The `Some(0)` vs `None` boundary test (ST3-3) documents the behavior. Downstream consumers (FR-28..30) must handle zero values. |
| `#[allow(dead_code)]` removal triggers unexpected clippy warning on another target | Low | Lesson L1 confirms removal is safe once call site exists. ST2 acceptance proves it with `cargo build`. |
| `multi-turn-reasoning.jsonl` fixture has unexpected token values after rebase | Low | ST4 extends fixture assertions; `cargo test` catches drift on any fixture update. |
