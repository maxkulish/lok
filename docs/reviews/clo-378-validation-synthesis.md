# Pre-PR validation: clo-378

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-19
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned structured PASS_WITH_NOTES report with 3 LOW findings. |
| Gemini | REVIEW_FAILED | Gemini CLI trust/empty-output failure; no structured report. |
| Claude fallback | SKIPPED | At least one external reviewer (Codex) succeeded. |

## Verdict
PASS

## Must Fix Before PR

- **[src/backend/mod.rs:903-925] Complete the `reasoning_tokens` Some/None matrix in `test_token_usage_saturating_add_folds_optionals`.** The plan ST4 table row #7 explicitly required "All 4 Some/None combinations for both fields", but the test only asserts `Some + Some` for `reasoning_tokens`. While `sum_opt` is shared (so production behavior is correct), the plan's documented acceptance is violated. Add 3 assertions: `Some + None`, `None + Some`, `None + None` for `reasoning_tokens`. ~3 lines.

- **[src/backend/mod.rs:145, 153] Remove `#[allow(dead_code)]` from `with_cached` and `with_reasoning`.** Neither the design nor the plan specified these suppressions. The methods are `pub` on a public struct — that's the intentional API surface to be consumed by CLO-381/CLO-382. The suppression both clutters the public API and is unnecessary for `pub` items in a library crate. If clippy/rustc actually warns when the gate runs, that's worth investigating; pre-emptive suppression is wrong. Remove both attributes and verify the pre-merge gate (`cargo clippy --all-targets -- -D warnings`) still passes.

## Out of Scope / Deferred

- None.

## False Positives / Tooling Artifacts

- **Codex finding on `saturating_add` total_tokens computation (src/backend/mod.rs:159).** Codex flagged that `total_tokens: self.total_tokens.saturating_add(other.total_tokens)` could diverge from `prompt + completion` for manually-constructed inconsistent inputs. This is a false positive against the design/plan: the design doc explicitly specifies this exact line (design L143-144), and plan ST3 calls it out as "semantically identical to old prompt+completion recomputation" for any `TokenUsage` constructed via `new()`. The chosen formulation also preserves saturation behavior under aggregation chains where individual `total_tokens` already saturated. Not actionable in this PR; revisit only if a future caller starts constructing `TokenUsage` literals that violate the invariant.
- **Gemini review failure.** Tooling artifact (Gemini CLI trust/empty-output); does not affect verdict because Codex succeeded.

## Recommendation

PROCEED_WITH_FIXES. Two bounded fixes before opening the PR: (1) add the three missing `reasoning_tokens` Some/None assertions to `test_token_usage_saturating_add_folds_optionals` to honor the plan's explicit matrix requirement; (2) drop the two unnecessary `#[allow(dead_code)]` attributes on the public builder methods and confirm the pre-merge gate (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`) is still green. Both fixes are local to `src/backend/mod.rs`, ~5 lines total, single iteration. The implementation otherwise matches the design exactly — additive fields, preserved 2-arg constructor, total_tokens excludes new fields, struct-literal test updated, 10 new unit tests landed.

## Re-validation

Fix iteration applied on commit `29b4e3d`.

| Fix | Status | Notes |
|-----|--------|-------|
| Complete `reasoning_tokens` Some/None matrix | ✅ Applied | Added 3 assertions (`Some + None`, `None + Some`, `None + None`) to `test_token_usage_saturating_add_folds_optionals`. |
| Remove `#[allow(dead_code)]` from builder methods | ❌ Reverted | Both methods trigger `-D dead-code` at `cargo clippy --all-targets -- -D warnings` because the binary crate (`lok`/`lokomotiv`) does not yet call them. They are intentionally public API for downstream FRs (CLO-381, CLO-382). The annotations are correct for the current slice; synthesis reviewer noted this would be "worth investigating if clippy warns" — investigation confirms the suppressions are necessary. |

Post-fix gate (2026-05-19):
- `cargo fmt --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo test` ✅ (all ~470 backend tests + integration tests pass)
- `cargo build --features bedrock` ✅
- `cargo doc --no-deps` ✅

**Revised verdict: PASS** — all `Must Fix Before PR` items have been addressed (test matrix complete; dead_code suppression investigated, confirmed necessary, and retained with explanation).
