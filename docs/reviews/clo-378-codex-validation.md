# Pre-PR validation: clo-378

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-19
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

LOW - [src/backend/mod.rs:159](/Users/mk/Code/orchestrator/lok--feat-clo-378-usage/src/backend/mod.rs:159): `saturating_add` now derives `total_tokens` from `self.total_tokens + other.total_tokens`. The previous implementation recomputed from summed `prompt_tokens` and `completion_tokens`, which normalized manually constructed inconsistent `TokenUsage` values. Since the design states `total_tokens` should remain `prompt + completion`, consider computing `prompt_sum` and `completion_sum` first, then `total_tokens: prompt_sum.saturating_add(completion_sum)`.

LOW - [src/backend/mod.rs:913](/Users/mk/Code/orchestrator/lok--feat-clo-378-usage/src/backend/mod.rs:913): The optional aggregation test covers all `Some`/`None` combinations for `cached_tokens`, but only `Some + Some` for `reasoning_tokens`. The helper is shared, so production behavior looks correct, but the plan explicitly called for all four combinations for both fields.

LOW - [src/backend/mod.rs:145](/Users/mk/Code/orchestrator/lok--feat-clo-378-usage/src/backend/mod.rs:145): `#[allow(dead_code)]` was added to the new public builders. If these are only to satisfy current no-caller state, I'd prefer removing them unless the build actually warns; the design did not call for suppressions, and public builder methods are intentional API.

## Missing Items

No functional acceptance criteria appear missing. The branch adds the two fields, preserves `TokenUsage::new(prompt, completion)`, adds both builders, folds optionals in aggregation, keeps new fields out of normal constructor totals, and updates the struct-literal test.

The only gap is test completeness for the `reasoning_tokens` `Some`/`None` matrix noted above.

## Recommendations

Recompute aggregate `total_tokens` from aggregate prompt/completion to preserve the documented invariant even for manually constructed public structs.

Add the missing `reasoning_tokens` `Some + None`, `None + Some`, and `None + None` assertions.

I did not run `cargo test`/`clippy`; the current sandbox is read-only, so build commands that write `target/` are not available here. `git diff --check main...HEAD` passed.
