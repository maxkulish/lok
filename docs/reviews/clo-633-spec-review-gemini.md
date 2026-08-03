# Spec Review: clo-633

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-08-03
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem is clearly, comprehensively, and accurately defined. It categorizes the panics into two distinct types (tail slice truncation and inverted range/overflow indexing) and successfully isolates the root causes. The rationale for how a user might naturally trigger these errors (e.g., CI logs with emojis, out-of-date line references) is well thought out, proving no malicious attacker is required. The diagnosis of why `src/tasks/fix.rs` and `src/tasks/context.rs` both panic (copy-pasted implementation) correctly identifies the underlying technical debt.

## 2. Acceptance Criteria Review
**Strong**: AC1 through AC8 are highly specific, measurable, and testable. The criteria explicitly address normal operations, limits/overflows (`usize::MAX`), bounds safety, backwards-compatibility of the UI output (AC7), and code deduplication. 
**Gaps**: None. The criteria cover edge cases (0 length, out of range, overflow) effectively.

## 3. Constraints Check
**Aligned**: 
- Consolidating the helpers into `src/utils.rs` alongside the existing `truncate_utf8` aligns perfectly with current codebase structure.
- Mandating `str::is_char_boundary` for a safe `tail_utf8` walk is a robust, low-complexity standard library solution.
- Ensuring byte-identical output for the current format effectively enforces this as a refactor/bug-fix rather than a format change.
**Concerns**: 
- The spec claims `str::floor_char_boundary` stabilized "later" than 1.80. It actually stabilized in Rust 1.77.0, so it *is* available in 1.80. However, `floor_char_boundary` yields a tail suffix *longer* than `max_bytes` if iterating backwards, whereas walking forwards ensures it is `<= max_bytes`. Furthermore, `ceil_char_boundary` (which would do what we want) is indeed nightly-only. So the constraint to use your custom `is_char_boundary` walk remains the optimal path regardless of the slight inaccuracy about the version.

## 4. Decomposition Quality
**Well-scoped**: The 3 sub-tasks are highly isolated, correctly ordered (sub-task 2 before sub-task 3), and easily fit within the small estimate.
- Walking forward via `is_char_boundary` ensures the tail string size strictly adheres to `<= max_bytes`.
- The `line_window` clamping math (`saturating_add`, `min`) handles all edge cases safely and elegantly.
**Issues**: No major issues in the decomposition itself. It perfectly fixes the identified defects.

## 5. Evaluation Coverage
**Covered**: The test table is excellent and exhaustive. Testing `tail_utf8("aaa...bbb")` with an emoji boundary (Test 2 & 3), and exhaustive `line_window` limits (Test 5-9) ensures no regressions. The `rg` tests for eliminating the duplicate logic (Test 13-14) are a great touch.
**Gaps**: 
- Test 10 expects a *symmetric* output (`line 2`, `line 3`, `line 4` for `context_lines = 1`). However, the existing math (`start = line.saturating_sub(context_lines)`) with a 1-based target line yields an *asymmetric* output (0 lines before, target line, 1 line after). To honor AC7 ("same asymmetric window..."), Test 10's expected output needs to be updated (see Actionable Feedback).

## 6. Codebase Alignment
**Violations**: None found. The proposed spec perfectly aligns with existing patterns.
**Alignment**: Adding `tail_utf8` adjacent to `truncate_utf8` is exactly where it belongs. Extracting the duplicated inline render loops into a single helper fixes the root problem of diverging copies while honoring the existing design.

## 7. Blind Spots
- **Missed Panic Sites (Critical)**: The spec claims "Four slicing sites panic on ordinary inputs", but a codebase check reveals **two more** slicing panics on ordinary inputs using the exact same flawed byte-truncation pattern:
  1. `src/main.rs`: `&diff[..max_diff_chars]` truncates the git diff to 50,000 bytes. If byte 50,000 is inside a multibyte char, it panics.
  2. `src/tasks/implement.rs`: `&code[..code.len().min(150)]` truncates to 150 bytes. If byte 150 is inside a multibyte char, it panics.
- **Deduplication Opportunity**: While the `render_line_window` logic is being consolidated to `src/utils.rs`, both `src/tasks/context.rs` and `src/tasks/fix.rs` still maintain identical copies of the `extract_file_references` function.

## 8. Verdict
APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Fix Missing Panics**: Expand the scope to include the two missed head-truncation panics. Replace `&diff[..max_diff_chars]` in `src/main.rs` and `&code[..code.len().min(150)]` in `src/tasks/implement.rs`. Unlike the CI tasks which need the tail, these both need the head, meaning they can simply be replaced by calls to the *existing* `crate::utils::truncate_utf8` function.
2. **Correct Test 10 Expectation**: Update the expected output for Test 10 in Section 5. A 1-based target line of 3 with `context_lines = 1` using the current math will yield 0 lines before and 1 line after. The expected string should be exactly `">>>    3: c\n     4: d\n"` to actually honor AC7.
3. **Consolidate `extract_file_references`**: Since you are deduplicating the rendering logic from `context.rs` and `fix.rs`, consider moving their identical copies of `extract_file_references` into `src/utils.rs` as part of this cleanup to prevent future drift.
