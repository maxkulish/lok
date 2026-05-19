# Plan: CLO-378 — FR-25b: extend TokenUsage with cached_tokens + reasoning_tokens

## Context
- Design: [docs/designs/clo-378-tokenusage-extension.md](../designs/clo-378-tokenusage-extension.md)
- Discovery: [docs/discovery/clo-378.md](../discovery/clo-378.md)
- Linear: https://linear.app/cloud-ai/issue/CLO-378/

## Sub-tasks

### ST1: Add cached_tokens and reasoning_tokens fields to TokenUsage struct + update constructor

**Files:** `src/backend/mod.rs` (struct definition at L108, `TokenUsage::new` at L114)

**Changes:**
- Add `pub cached_tokens: Option<u32>` and `pub reasoning_tokens: Option<u32>` to the struct with doc comments (including "may exceed prompt_tokens" note per design review P2).
- Update `TokenUsage::new(prompt_tokens, completion_tokens)` to default both new fields to `None`.
- Ensure `#[derive(Debug, Clone, Default, PartialEq, Eq)]` stays unchanged.

**Acceptance:** `cargo build` — all 12 existing `TokenUsage::new()` and `TokenUsage::default()` call sites compile without changes.

**Estimate:** S

---

### ST2: Add with_cached and with_reasoning builder methods

**Files:** `src/backend/mod.rs` (inside `impl TokenUsage`)

**Changes:**
- Add `pub fn with_cached(mut self, cached: Option<u32>) -> Self` — doc comment including consuming-self semantics per design review P3.
- Add `pub fn with_reasoning(mut self, reasoning: Option<u32>) -> Self`.

**Acceptance:** `cargo build` — builder methods compile, callable from the same module. (No callers wire data yet per non-goals; compile-only check is sufficient at this stage.)

**Estimate:** S

---

### ST3: Extend saturating_add to fold optional fields + add sum_opt private helper

**Files:** `src/backend/mod.rs` (inside `impl TokenUsage`, plus new file-level private function)

**Changes:**
- Add private `fn sum_opt(a: Option<u32>, b: Option<u32>) -> Option<u32>` helper.
- Rewrite `saturating_add` from delegating to `TokenUsage::new()` to a struct literal summing each field individually, calling `sum_opt` for cached_tokens and reasoning_tokens.
- `total_tokens` uses `self.total_tokens.saturating_add(other.total_tokens)` (semantically identical to old prompt+completion recomputation).

**Acceptance:** `cargo test test_token_usage_saturating_add` — existing saturating-add test still passes (verifies prompt/completion/total unchanged).

**Estimate:** S

---

### ST4: Fix struct-literal test site + add 10 unit tests for new behaviour

**Files:** `src/backend/mod.rs` (test module, around L859-862 and new test functions)

**Changes:**
- Fix `test_query_output_with_usage_some`: replace `TokenUsage { prompt_tokens: 5, completion_tokens: 10, total_tokens: 15 }` with the same struct literal plus `..Default::default()`.
- Add 10 new unit tests (exact names from design doc § Test plan):

  | # | Test function | What it verifies |
  |---|---------------|-----------------|
  | 1 | `test_token_usage_new_defaults_new_optionals_to_none` | `new(10, 20)` → cached=None, reasoning=None |
  | 2 | `test_token_usage_default_is_all_zero_and_none` | `default()` → all zeros + Nones |
  | 3 | `test_token_usage_with_cached_sets_field` | `.with_cached(Some(7))` sets cached_tokens=Some(7) |
  | 4 | `test_token_usage_with_reasoning_sets_field` | `.with_reasoning(Some(13))` sets reasoning_tokens=Some(13) |
  | 5 | `test_token_usage_with_cached_none_is_idempotent` | `.with_cached(Some(7)).with_cached(None)` → cached_tokens=None |
  | 6 | `test_token_usage_total_excludes_cached_and_reasoning` | `new(100, 50).with_cached(Some(40)).with_reasoning(Some(20)).total_tokens == 150` |
  | 7 | `test_token_usage_saturating_add_folds_optionals` | All 4 Some/None combinations for both fields |
  | 8 | `test_token_usage_saturating_add_clamps_optional_overflow` | `Some(u32::MAX) + Some(1) == Some(u32::MAX)` |
  | 9 | `test_token_usage_saturating_add_preserves_total_invariant` | `total_tokens` sum unaffected by new field values |
  | 10 | `test_query_output_with_usage_some` (updated) | Re-assert equality after `..Default::default()` fix |

**Acceptance:** `cargo test --lib backend` — all backend module tests pass (existing 468 + 10 new). `cargo test test_token_usage_saturating_add` — existing test unaffected. `cargo test test_query_output_with_usage_some` — updated test passes.

**Estimate:** M (most time is writing 10 test bodies)

---

### ST5: Pre-merge gate

**Files:** All changes from ST1-ST4.

**Acceptance:**
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Also run (per design doc manual verification steps):
```bash
cargo build --features bedrock
cargo doc --no-deps
cargo run --bin lok -- --help
```

**Estimate:** S (automated gate)

---

## Pre-merge gate
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Risks
- **rs-wisper exhaustive destructure**: If rs-wisper pattern-matches `TokenUsage` exhaustively (without `..`), it gets a compile error when bumping lok. Mitigation: assumption A4 in design doc covers this; grep rs-wisper before merge to confirm. Fix is a one-line `..` addition in rs-wisper — not a blocker for this PR.
- **clippy lint surprise**: `Option<u32>` on a public field may trigger `clippy::option_option` or similar. Mitigation: `cargo clippy --all-targets -- -D warnings` catches any new lint before gate passes. If triggered, `#[allow(...)]` is a 1-line fix.
