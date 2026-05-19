# Design: CLO-378 - FR-25b: extend TokenUsage with cached_tokens + reasoning_tokens

## Problem

`TokenUsage` at `src/backend/mod.rs:108` carries only `prompt_tokens`, `completion_tokens`, and `total_tokens`. Anthropic's `cache_read_input_tokens` and Codex's `cached_input_tokens` / `reasoning_output_tokens` exist in the upstream API responses but are silently flattened into `total_tokens` because the struct has no dedicated fields. This blocks the downstream consumers identified during discovery: run-summary aggregation (FR-28), JSON output mode (FR-29), the rs-wisper crate that depends on lok's `TokenUsage` shape, and the upstream Codex (CLO-381) and Gemini (CLO-382) extraction work that cannot wire data into fields that do not yet exist. PRD `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §4 FR-25b and §9 release-plan step 4 sequence this extension immediately after FR-25a (the conductor copy already merged via CLO-370). Adding the fields now is the smallest unblock for the entire Token & Usage Observability FR group.

## Goals / Non-goals

Goals:
- Add `cached_tokens: Option<u32>` and `reasoning_tokens: Option<u32>` to `TokenUsage` in `src/backend/mod.rs`.
- Keep `TokenUsage::new(prompt, completion)` a 2-argument constructor that defaults the new fields to `None`, preserving all 12 existing call sites verified in discovery.
- Provide a builder-style `with_cached(self, Option<u32>) -> Self` and `with_reasoning(self, Option<u32>) -> Self` for backends that have the data, so future FR-25 / FR-26 / FR-27a wiring is a one-line chain at the existing extraction points.
- Keep `total_tokens` equal to `prompt_tokens.saturating_add(completion_tokens)`. Reasoning and cached counts do NOT enter `total_tokens`.
- Extend `TokenUsage::saturating_add` so cross-step aggregation sums the new optional fields without panicking when one side is `None`.
- Fix the one struct-literal test at `src/backend/mod.rs:859-862` that breaks under additive field changes.
- Land additive unit tests covering: defaulting to `None`, builder setters, saturating aggregation across `Some`/`None` mixes, `total_tokens` invariant.

Non-goals:
- Wiring Codex (FR-25, CLO-381), Gemini (FR-26, CLO-382), Claude CLI (FR-27a), or Bedrock to populate the new fields. Those FRs depend on this one and are tracked separately.
- Adding `serde::{Serialize, Deserialize}` derives. Discovery Approach 2 considered and rejected this until FR-29 (JSON output mode) needs it. `StepResult` does not derive serde today and adding it only to `TokenUsage` widens the public crate API prematurely.
- Replacing the constructor with a builder pattern (Approach 3) - rejected as over-engineered for two optional fields.
- Changing `u32` types, adding a `cache_write_tokens` field, or modeling cache savings as anything richer than two flat optionals.
- Changing the `total_tokens` formula or introducing a separate "billable tokens" field.

## Architecture

All changes are scoped to one file: `src/backend/mod.rs`. No new modules. No new dependencies. No changes to the `Backend` trait, `QueryOutput`, `StepResult`, or any backend implementation.

Data flow (unchanged by this design, shown for context):

```
backend HTTP / CLI response
        │
        ▼
backend impl (claude.rs / codex.rs / gemini.rs / ollama.rs / bedrock.rs)
        │  TokenUsage::new(prompt, completion)
        │      .with_cached(api_cached_tokens)        ← enabled by FR-25b
        │      .with_reasoning(api_reasoning_tokens)  ← enabled by FR-25b
        ▼
QueryOutput.usage: Option<TokenUsage>
        │
        ▼
StepResult.usage  (already wired by CLO-370 / FR-25a)
        │
        ▼
run summary (FR-28) / JSON output (FR-29) / rs-wisper consumer
```

Touched locations:

| File | Change |
|------|--------|
| `src/backend/mod.rs:108-112` | Add two `Option<u32>` fields to `TokenUsage`. |
| `src/backend/mod.rs:114-123` | Keep `new()` 2-arg; default new fields to `None`. |
| `src/backend/mod.rs:114-132` | Add `with_cached` and `with_reasoning` builder methods. |
| `src/backend/mod.rs:125-131` | Extend `saturating_add` to fold optional fields. |
| `src/backend/mod.rs:859-862` | Replace struct literal with `..Default::default()` (or builder). |
| `src/backend/mod.rs` test module | Add unit tests for new behavior. |

`#[derive(Debug, Clone, Default, PartialEq, Eq)]` stays unchanged. `Option<u32>` implements all four traits, so the derives keep compiling. `Default` continues to yield `None` for the new fields.

## Public API surface

Before (`src/backend/mod.rs:107-132`):

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
        }
    }

    pub fn saturating_add(&self, other: &Self) -> Self {
        Self::new(
            self.prompt_tokens.saturating_add(other.prompt_tokens),
            self.completion_tokens.saturating_add(other.completion_tokens),
        )
    }
}
```

After:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// Tokens served from prompt cache (Anthropic `cache_read_input_tokens`,
    /// Codex `cached_input_tokens`). `None` when the backend does not report it.
    /// NOT included in `total_tokens`; surfaced separately so cache savings are
    /// visible to run summary / JSON output.
    ///
    /// **Note**: This value is reported directly by the upstream API and may
    /// exceed `prompt_tokens` in edge cases (e.g. server-side caching on a
    /// different message). It is stored as-reported; no validation is applied.
    pub cached_tokens: Option<u32>,
    /// Reasoning / thinking tokens billed in addition to completion
    /// (Codex `reasoning_output_tokens`, o-series). `None` when not reported.
    /// NOT included in `total_tokens`.
    pub reasoning_tokens: Option<u32>,
}

impl TokenUsage {
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
            cached_tokens: None,
            reasoning_tokens: None,
        }
    }

    /// Set `cached_tokens`. Consumes `self` for use in method-chaining
    /// construction patterns (e.g. `TokenUsage::new(p, c).with_cached(Some(40))`).
    pub fn with_cached(mut self, cached: Option<u32>) -> Self {
        self.cached_tokens = cached;
        self
    }

    pub fn with_reasoning(mut self, reasoning: Option<u32>) -> Self {
        self.reasoning_tokens = reasoning;
        self
    }

    pub fn saturating_add(&self, other: &Self) -> Self {
        Self {
            prompt_tokens: self.prompt_tokens.saturating_add(other.prompt_tokens),
            completion_tokens: self
                .completion_tokens
                .saturating_add(other.completion_tokens),
            total_tokens: self.total_tokens.saturating_add(other.total_tokens),
            cached_tokens: sum_opt(self.cached_tokens, other.cached_tokens),
            reasoning_tokens: sum_opt(self.reasoning_tokens, other.reasoning_tokens),
        }
    }
}

fn sum_opt(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x),
        (Some(x), Some(y)) => Some(x.saturating_add(y)),
    }
}
```

Construction-site fix at `src/backend/mod.rs:859-862` (inside `test_query_output_with_usage_some`):

```rust
// before
Some(TokenUsage {
    prompt_tokens: 5,
    completion_tokens: 10,
    total_tokens: 15,
})

// after
Some(TokenUsage {
    prompt_tokens: 5,
    completion_tokens: 10,
    total_tokens: 15,
    ..Default::default()
})
```

All 12 other audited call sites (`TokenUsage::new(...)` and `TokenUsage::default()` in `ollama.rs`, `claude.rs`, `bedrock.rs`, `mod.rs` tests, `workflow.rs:6620`) compile unchanged.

`sum_opt` is a private file-local helper; it is NOT part of the public crate surface.

## Assumptions

- **A1 (high)**: `Option<u32>` derives `Debug, Clone, Default, PartialEq, Eq` correctly under the existing struct-level `#[derive(...)]`. Verification: standard library guarantee; `cargo build` will fail loudly otherwise.
- **A2 (high)**: All 13 construction sites enumerated in the discovery report are exhaustive. Only `src/backend/mod.rs:859-862` uses a struct literal that omits `..Default::default()`. Verification: pre-implementation rerun of `rg 'TokenUsage\s*\{' src/ tests/` and `rg 'TokenUsage::(new|default)' src/ tests/`.
- **A3 (high)**: `total_tokens` semantics stay `prompt + completion`. PRD §4 FR-25b states this explicitly; rs-wisper relies on it. Verification: read PRD line 192 and existing `test_token_usage_new_computes_total`.
- **A4 (medium)**: rs-wisper consumes `TokenUsage` only by reading existing fields and does not destructure with an exhaustive struct pattern. Verification: grep rs-wisper for `TokenUsage` use; if it pattern-matches exhaustively this becomes a coordination item but is still source-compatible because rs-wisper would just need to add the new fields (this stays additive, not breaking).
- **A5 (medium)**: `saturating_add` is only called from within lok during cross-step aggregation (not yet wired - FR-28). No external caller depends on its current 2-field behavior. Verification: `rg 'TokenUsage::saturating_add|usage\.saturating_add|\.saturating_add\(&'`.
- **A6 (low)**: Backends that report a single combined "cache hits" count (no separate write vs read) can fit it into `cached_tokens` without losing information needed by FR-28 verbose output. Verification: revisit when FR-25 (Codex) and FR-27a (Claude CLI) land - if they need to distinguish, add a follow-up FR.

## Test plan

Unit tests (added to the `tests` module in `src/backend/mod.rs`):

- `test_token_usage_new_defaults_new_optionals_to_none` - `TokenUsage::new(10, 20)` yields `cached_tokens == None` and `reasoning_tokens == None`.
- `test_token_usage_default_is_all_zero_and_none` - `TokenUsage::default()` yields zeros and `None`s.
- `test_token_usage_with_cached_sets_field` - builder sets `cached_tokens` to `Some(7)` and leaves others untouched.
- `test_token_usage_with_reasoning_sets_field` - builder sets `reasoning_tokens` to `Some(13)` and leaves others untouched.
- `test_token_usage_with_cached_none_is_idempotent` - calling `.with_cached(None)` after a `Some` value clears back to `None` (documented builder semantics).
- `test_token_usage_total_excludes_cached_and_reasoning` - explicit pin: `new(100, 50).with_cached(Some(40)).with_reasoning(Some(20)).total_tokens == 150`.
- `test_token_usage_saturating_add_folds_optionals` - `Some(5) + Some(7) == Some(12)`, `Some(5) + None == Some(5)`, `None + Some(7) == Some(7)`, `None + None == None`.
- `test_token_usage_saturating_add_clamps_optional_overflow` - `Some(u32::MAX).saturating_add(Some(1)) == Some(u32::MAX)` for both new fields.
- `test_token_usage_saturating_add_preserves_total_invariant` - explicit pin: `new(10, 20).saturating_add(&new(3, 4).with_cached(Some(1)).with_reasoning(Some(2))).total_tokens == 37` (prompt+completion sum, unaffected by cached/reasoning).
- Update `test_query_output_with_usage_some` to use `..Default::default()` and re-assert equality holds.

Existing tests that must continue passing unchanged: `test_token_usage_new_computes_total`, `test_token_usage_new_saturates_on_overflow`, `test_token_usage_default_zero`, `test_token_usage_saturating_add`, `test_query_output_with_usage_none`.

Integration tests under `tests/`: none required. This FR adds dormant fields - there is no end-to-end behavior to integration-test until FR-25 / FR-26 / FR-27a wire upstream data. The discovery report and PRD §9 release-plan step 4 both treat this as a structural precondition.

Per-backend test matrix (regression-only - all backends must still construct `TokenUsage` and populate `QueryOutput.usage` correctly without referencing the new fields):

| Backend | File | Existing test pin | New work this FR |
|---------|------|-------------------|------------------|
| Claude (API) | `src/backend/claude.rs:186` | Existing usage-extraction tests | None - call site unchanged |
| Codex | `src/backend/codex*.rs` | Existing parser tests | None (FR-25 / CLO-381 wires data later) |
| Gemini | `src/backend/gemini.rs` | n/a | None (FR-26 / CLO-382 wires data later) |
| Ollama | `src/backend/ollama.rs:145,216` | Existing extraction tests | None - call site unchanged |
| Bedrock | `src/backend/bedrock.rs:188` | Existing extraction tests | None - call site unchanged |

Manual verification steps:

1. `cargo fmt --check`
2. `cargo clippy -- -D warnings` - new `Option<u32>` fields must not trip `clippy::option_option` or similar lints.
3. `cargo test` - expect prior test count of 468 plus the new unit tests (target ~475+).
4. `cargo build --features bedrock` - confirm the bedrock-gated path still compiles.
5. `cargo doc --no-deps` - confirm the new doc comments render and there are no broken intra-doc links.
6. Local sanity: `cargo run --bin lok -- --help` still works (smoke check that the binary links).

## Migration / rollout

This change is **purely additive**. There is no feature flag, no migration script, no staged rollout.

Backward compatibility:
- All existing `TokenUsage::new(p, c)` call sites compile unchanged - the constructor signature is preserved.
- All existing `TokenUsage::default()` call sites compile unchanged - `Default` continues to zero/None all fields.
- The one struct-literal site in tests is fixed in the same PR with `..Default::default()`.
- `TokenUsage` is not serialized anywhere in lok today (discovery confirmed: no serde derives, no `serde_json::to_value` over `TokenUsage`, no on-disk persistence). No serialization-compatibility surface exists.
- rs-wisper depends on lok as a Rust crate. Adding `Option<u32>` fields to a public struct is source-compatible: any consumer that constructs via `TokenUsage::new` or `TokenUsage::default` recompiles cleanly; any consumer that destructures with `..` is unaffected; only consumers using exhaustive struct-literal construction or exhaustive struct-pattern destructuring would need a one-line fix.

Rollout order:
1. Merge this PR (CLO-378).
2. CLO-381 (FR-25, Codex usage extraction) and CLO-382 (FR-26, Gemini backend usage) can then start in parallel; both depend on this PR and are listed as `blocks` in the workflow YAML.
3. FR-28 (run-summary aggregation) consumes the new optional fields once at least one backend populates them.

There is no need to coordinate a release branch cut with rs-wisper - the change is additive at the Rust-source level. If rs-wisper happens to use exhaustive struct literals on `TokenUsage`, it will see a single compile error and can fix it independently when it bumps the lok dependency.

## Open questions

- **Aggregation display for mixed-backend workflows**: PRD §8 line 326 carries an open question owned by MK with a 2026-06-04 deadline (before FR-28): when a workflow mixes Codex (reports `reasoning_output_tokens`) and Anthropic (reports `cache_read_input_tokens`), should run-summary display them under a single combined heading or separately? The PRD's current recommendation is "separately in verbose mode". This design intentionally does not pre-bind the answer - the struct stores both fields as independent `Option<u32>` so either display strategy works. Resolution belongs to the FR-28 design, not this one.
- **Whether `cached_tokens` should split read vs write**: Anthropic reports `cache_read_input_tokens` and `cache_creation_input_tokens` separately. Codex's `cached_input_tokens` is a single counter. Approach 1 chosen during discovery models a single `cached_tokens` field. If FR-25 / FR-27a wiring shows that cache-write tokens are a meaningful cost signal lost by flattening, a follow-up FR to add `cache_write_tokens: Option<u32>` is preferable to widening this PR. Tradeoff: smaller PR now vs one-more-FR later. No action this design.
- **`Option<u32>` vs `u32` with a `0`-means-absent convention**: rejected during discovery in favor of `Option<u32>` because `0` is a legitimate value ("no cache hit on this call") and conflating it with "backend did not report" loses information needed by FR-28. Re-opening this would require flagging Approach 1 as wrong - the discovery YAML records `approach_chosen` as the minimal struct extension, so this is closed unless new evidence appears.
