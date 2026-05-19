# Design Review: CLO-378

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-05-19
**Pipeline**: lok design-review (manual fallback — lok template bug in health_check ref)

---

## 1. Completeness Check

All 9 required sections present and populated:

| Section | Present | Assessment |
|---------|---------|------------|
| Problem | ✓ | 5 sentences; cites discovery, PRD, blocked downstream tasks |
| Goals / Non-goals | ✓ | 7 goals, 5 non-goals; explicit boundary |
| Architecture | ✓ | Data-flow diagram, touched-locations table, one-file scope |
| Public API surface | ✓ | Complete before/after Rust code blocks, construction-site fix |
| Assumptions | ✓ | 6 assumptions with confidence levels and verification paths |
| Test plan | ✓ | Named test functions, per-backend regression matrix, manual steps |
| Migration / rollout | ✓ | Purely additive, 2-paragraph backward compat, 3-step rollout order |
| Open questions | ✓ | 3 genuine open questions with tradeoff descriptions |

## 2. Architecture Assessment

**Strengths**:
- Single-file scope (`src/backend/mod.rs` only) is exactly right for a struct-level additive change
- `sum_opt` extracted as a private helper avoids inline match-arm duplication in `saturating_add`
- Builder methods (`with_cached`, `with_reasoning`) use consuming self — correct for the "construct once, chain once" pattern used by backend extraction sites
- Data-flow diagram makes the downstream relationship to FR-25/FR-28/FR-29 explicit

**Concerns**:
- `saturating_add` changes internal computation from `TokenUsage::new(self.prompt_tokens + other.prompt_tokens, ...)` to a struct literal with manual field addition. The new path uses `total_tokens: self.total_tokens.saturating_add(other.total_tokens)` instead of recomputing from prompt+completion. This is semantically identical but worth noting as a behavioral change in the computation path — if a future bug corrupts `total_tokens`, the old code would have masked it; the new code propagates the corruption. Mitigation: the "total excludes cached/reasoning" test (`test_token_usage_total_excludes_cached_and_reasoning`) partially covers this.

## 3. Codebase Alignment

- `TokenUsage::new(p, c)` signature preserved — all 12 existing call sites compile unchanged ✓
- Uses `#[derive(Default)]` which `tokenusage` already derives ✓
- `sum_opt` follows existing `saturating_add` naming convention ✓
- Builder methods match Rust ecosystem patterns (std, builder crates) ✓

## 4. Code Quality

- Clean interface: `new()`, `with_cached()`, `with_reasoning()`, `saturating_add()` — four public methods with clear single responsibilities ✓
- `sum_opt` is correctly private — it's an implementation detail not exposed on `TokenUsage` ✓
- No new traits, generics, or lifetimes introduced ✓

## 5. Security Posture

N/A — this is a pure data struct with no file I/O, network access, or secret management. `TokenUsage` carries unsigned integer token counts only.

## 6. Operational Readiness

N/A — no runtime behavior changes. The struct change is compile-time only until FR-25 / FR-26 / FR-27a wire upstream data.

## 7. Concurrency Safety

N/A — `TokenUsage` is a plain data struct with `Debug + Clone + Default + PartialEq + Eq`. No `Send`/`Sync` implications beyond what derives already provide.

## 8. Blind Spots

**B1: `cached_tokens` exceeding `total_tokens`** (LOW). Anthropic can report `cache_read_input_tokens` that exceeds `total_tokens` in edge cases (e.g., server-side caching on a different message). The design stores whatever the API reports without validation. This is defensible (it's upstream data, not a derivation) but a paranoid validation in `saturating_add` or `with_cached` could save downstream aggregation from confusing numbers. Recommendation: add a doc comment on `cached_tokens` noting that "this value is reported by the upstream API and may exceed prompt_tokens in edge cases."

**B2: `with_cached(None)` clears a previously-set value** (LOW). The design says `.with_cached(None)` after a `Some` value "clears back to None" — this is documented as a test case. For a consuming builder with a single construction chain, this is fine. But if someone stores `TokenUsage` and calls `.with_cached()` later (impossible with consuming self), the semantics are irrelevant. A doc comment on `with_cached` clarifying consuming-self behavior would help.

## 7. Verdict

**APPROVE** — the design is complete, well-scoped, and correctly sized for a small additive struct extension. No design-level blockers. The two blind spots above are doc-level suggestions.

## 8. Actionable Feedback

| Priority | Item |
|----------|------|
| P2 | Add doc comment on `cached_tokens` field: "Reported by upstream API; may exceed prompt_tokens in edge cases. Not validated." |
| P3 | Add doc comment on `with_cached` clarifying consuming-self semantics |
| P4 | Consider adding a test that `saturating_add` total_tokens computes correctly when inputs have mismatched cached/reasoning (current test plan covers this implicitly via the "total excludes" test, but an explicit pin for the new computation path is cheap) |
