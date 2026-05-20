# Design Review: CLO-381

**Reviewer**: Agent (lok pipeline failed — `ollama_review` unknown variable)

**Reviewed**: 2026-05-20

**Pipeline**: lok design-review (degraded — manual review)

---

## 1. Completeness Check

All 8 required sections present and substantive:

| Section | Status | Notes |
|---------|--------|-------|
| Problem | ✅ | Cites discovery context, clear symptom description |
| Goals / Non-goals | ✅ | 5 goals, 5 non-goals — well-bounded |
| Architecture | ✅ | ASCII data-flow diagram, explicit file paths |
| Public API | ✅ | Before/after Rust code with line numbers |
| Assumptions | ✅ | 5 assumptions, all high confidence with verification paths |
| Test plan | ✅ | 3 new unit tests + 2 fixture extensions + per-backend matrix |
| Migration | ✅ | Additive, no breakage, single PR |
| Open questions | ✅ | Multi-turn delta tracking, carried from discovery debt |

## 2. Architecture Assessment

**Strengths**:

- The change is surgically precise: one arm of a match expression, two field type changes, three `#[allow(dead_code)]` removals. No new modules, no trait changes, no dependency additions.
- The ASCII data flow diagram correctly shows that downstream `QueryOutput.usage` → `StepResult.usage` is untouched — this task only enriches data within the existing `TokenUsage` carrier.
- The `Option<u32>` field type change on `CodexUsage` is the correct resolution for the `None`-when-absent contract. It aligns with `TokenUsage`'s documented semantics.

**Concerns**:

1. **Serde default inconsistency on `CodexUsage`.** The design changes `cached_input_tokens` and `reasoning_output_tokens` from `u32` to `Option<u32>` (with `#[serde(default)]` → `None` when absent). However, `input_tokens` and `output_tokens` remain as bare `u32` with `#[serde(default)]` → `0` when absent. This means a Codex event with `{"input_tokens":100}` would produce `output_tokens: 0`, which is semantically different from `cached_input_tokens: None` — one is "zero", the other is "not reported." While `input_tokens`/`output_tokens` are always present in practice, the inconsistency is worth noting. **Severity: Low** — not a blocker for this task, as the PRD only scopes cached/reasoning mapping, but worth a comment in the implementation.

2. **The architecture diagram shows stale code.** The ASCII diagram reads `.with_cached(Some(u.cached_input_tokens))` which would be incorrect after the `Option<u32>` change (should be `.with_cached(u.cached_input_tokens)` without `Some()`). The Public API section correctly shows the unwrapped version — the ASCII diagram needs to match.

## 3. ADR Compliance

No ADRs in `docs/adrs/` directly address Codex usage extraction. The design follows the existing backend module pattern established by CLO-378 (TokenUsage extension) and CLO-379 (event-driven parser). **No ADR violations.**

## 4. Security Review

This change adds no new I/O, no command execution, no user-controlled input paths. The `CodexUsage` fields are deserialized from JSONL produced by the Codex CLI process that `lok` spawns — an already-trusted channel. The `serde(default)` guard prevents panics on missing fields. **No security concerns.**

## 5. Implementation Concerns

1. **Fixture test assertions on `reasoning_tokens: Some(0)`.** The `turn-completed.jsonl` fixture has `"reasoning_output_tokens":0` (present, value 0). After the `Option<u32>` change, this deserializes as `Some(0)`, not `None`. The design correctly asserts `Some(0)` — this distinction matters because `Some(0)` means "Codex reported it as zero" while `None` means "Codex didn't report it." **Fine as-is.**

2. **`TokenUsage.with_cached` / `.with_reasoning` accept `Option<u32>`** — after the `CodexUsage` field type change, the call naturally becomes `.with_cached(u.cached_input_tokens)` (without `Some()` wrapper). This is cleaner than the original discovery approach. **Verify the Public API section is definitive for implementation** — it shows the correct builder calls; the ASCII diagram in Architecture needs syncing.

3. **`#[allow(dead_code)]` removal order.** The attribute on `CodexUsage` struct can be removed because both `cached_input_tokens` and `reasoning_output_tokens` will be read. However, `input_tokens` and `output_tokens` are also read — so the struct was never actually dead. The `#[allow(dead_code)]` was likely unnecessary from the start. **Fine to remove.**

## 6. Blind Spots

1. **Zero-value presence.** When Codex emits `"reasoning_output_tokens":0`, the token count is zero but reported. The `Option<u32>` type correctly preserves this as `Some(0)`. However, a downstream consumer checking `is_some()` to mean "Codex reported this metric" would get a false positive if they only care about non-zero reasoning. This is a downstream concern, not a parser concern — the parser faithfully passes through what Codex emits. **No action needed**, but worth documenting in a code comment that `Some(0)` is intentional.

2. **Multi-turn accumulation with Option fields.** The discovery debt correctly notes that multi-turn accumulation would need to switch from last-turn-take to running sum. If that happens, the `Option<u32>` fields add minor complexity to summation (need `sum_opt` or similar). The `TokenUsage::saturating_add` method at `src/backend/mod.rs:165` already handles `Option` fields. **No action needed.**

## 7. Verdict

**APPROVE**

The design is minimal, well-bounded, and has no architectural or security issues. The two concerns identified (ASCII diagram sync, serde default inconsistency note) are cosmetic/documentation-level and do not affect correctness.

## 8. Actionable Feedback

| # | Priority | Category | Action |
|---|----------|----------|--------|
| 1 | Low | Doc | Sync the ASCII data-flow diagram in Architecture with the corrected builder calls (remove `Some()` wrapper) |
| 2 | Low | Doc | Add a comment to `CodexUsage` explaining why `input_tokens`/`output_tokens` stay as bare `u32` with `#[serde(default)]` while `cached_input_tokens`/`reasoning_output_tokens` are `Option<u32>` |
| 3 | Low | Test | Consider adding an inline test for the `Some(0)` vs `None` boundary: `"reasoning_output_tokens":0` → `Some(0)`, verifying the distinction from absent-field → `None` |
