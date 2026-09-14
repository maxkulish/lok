# Spec Review: clo-656

**Reviewer**: Codex via Ollama (glm-5.3:cloud)
**Reviewed**: 2026-09-14
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is clear, self-contained, and matches the Linear task.

**Strong points**
- It correctly identifies the actual defect: `map_template_error` collapses every `TemplateError` into `UnknownVariable`, including pure syntax and render failures (`src/workflow.rs:3117-3151`).
- It grounds the issue in the exact CLO-655 reproduction and explains why the current fallback blames the first `{{ }}` in the template rather than the failing construct.
- It usefully expands the Linear ticket's scope to two adjacent defects that the same code path also causes:
  - non-undefined render errors such as `UnknownFilter` and `InvalidOperation`
  - genuine undefined attributes being reported with a leading dot, e.g. `.missing.output` rather than `steps.missing.output`
- It correctly notes that `TemplateError::ParseError` is not a reliable syntax classifier because `from_minijinja` also routes render-time `InvalidOperation` errors with a line into that variant (`src/template/mod.rs:24-35`).
- It correctly treats CLO-655's underlying `${#VAR}` parse failure as out of scope and limits this ticket to reporting.

**Minor accuracy note**
- AC3 as written conflicts with AC4; see the acceptance-criteria review.

## 2. Acceptance Criteria Review

**Strong**
- AC1 is specific and testable: syntax errors must produce `TemplateSyntax`, never `UnknownVariable`, and the Display must contain the MiniJinja detail and line.
- AC2 is specific and testable: non-undefined render errors must produce `TemplateRender`, with the MiniJinja kind and detail.
- AC4 is unusually well specified: it names the exact trigger (`unexpected end of comment`), the hint contents, the `$` prefix condition, and the fallback when no candidate is found.
- AC5 is measurable and directly tightens the existing weak `contains("missing")` assertion to exact full-path equality.
- AC6 covers the no-range undefined case and prevents another invented variable name.
- AC7 correctly preserves `evaluate_condition`'s current false/true semantics.
- AC8 adds the usual CI gates.

**Gaps**
1. **AC3 contradicts AC4.**  
   AC3 says:
   > No error produced by `map_template_error` names a variable or construct taken from a location other than the MiniJinja error's own range.

   AC4 then requires a hint naming the unclosed `{#` opener, whose location is **not** MiniJinja's end-of-input range. The intended reading is obvious — AC3 bans location-independent guesses, while AC4 permits one narrowly scoped, syntax-specific hint — but the criteria do not state that exception. A strict AC3 test would fail a correct AC4 implementation.
2. **AC6/Display handling is underspecified.**  
   The constraints say to omit MiniJinja's `(in <string>:N)` suffix, but also say:
   > If `detail()` is `None`, use MiniJinja's `Display` text as it is.

   For real MiniJinja `UndefinedError`s, `Display` includes that suffix. The direct unit test in test 7 constructs an error without a template name, so it will not catch this. The spec should say to use `format!("{}", err.kind())` when `detail()` is `None`, or explicitly strip the location suffix.
3. **Raw-block scanning is not fully pinned down.**  
   The rule says to skip `{% raw %}...{% endraw %}` regions and then find the first `{#` with no later `#}`. It does not explicitly say that a `#}` **inside** a skipped raw region must not close an earlier `{#`. The existing edge test covers a `{#` inside raw followed by a real `{#`; it does not cover the inverse ordering.

## 3. Constraints Check

**Aligned**
- Adding typed `WorkflowError` variants follows the existing `thiserror` pattern in `src/workflow.rs:28-82`.
- Preserving `UnknownVariable`'s Display byte-for-byte is the right compatibility constraint.
- Using `LazyLock<Regex>` matches the existing regex idiom in `workflow.rs`.
- Reusing `crate::utils::truncate_utf8` matches the codebase's UTF-8-safe truncation pattern.
- Keeping `TemplateError::from_minijinja` unchanged is correct; the new classification belongs in `map_template_error`, which sees the real MiniJinja kind.
- Keeping `evaluate_condition` semantics unchanged is correct and explicitly covered.
- Keeping CLO-655 and MiniJinja delimiter changes out of scope is a good boundary.

**Concerns**
1. **The `detail() == None` Display rule conflicts with the location-suffix rule** described above.
2. **The raw-block scanner semantics need one clarifying sentence**: `#}` inside a skipped raw region must not close an earlier candidate.
3. **Nested raw blocks are not addressed.** If a template contains `{% raw %}{% raw %}{% endraw %}{% endraw %}`, a simple non-greedy regex may misclassify spans. This may be rare, but the spec should either state that only non-nested raw blocks are supported or require a small scanner rather than a single regex.

## 4. Decomposition Quality

**Well-scoped**
1. `TemplateError` accessors — small and independent.
2. New `WorkflowError` variants — small and independent.
3. `map_template_error` rewrite — depends on 1 and 2, but is still well bounded.
4. Tests — depends on 3, and the table already defines them concretely.

**Issues**
- Sub-task 3 bundles four behaviors: kind dispatch, full-identifier extraction, no-range fallback, and unclosed-comment hinting. That is still small, but if the raw-block scanner grows, it may deserve its own sub-task.
- The spec says tests can be written first and run red before sub-task 3, which is good, but the tests cannot compile until the new variants exist. That dependency is implicit rather than explicit.

## 5. Evaluation Coverage

**Covered**
- AC1: tests 1 and 3.
- AC2: tests 4 and 5.
- AC4: tests 1 and 2, plus raw-block, closed-comment, protected-loop-variable, and truncation edge cases.
- AC5: test 6.
- AC6: test 7.
- AC7: existing tests plus the edge-case confirmation.
- AC8: test 8.
- End-to-end reporting: test 9.

**Gaps**
- No test for a candidate `{#` followed by a `#}` inside a raw block.
- No test for a real MiniJinja-produced `UndefinedError` with a template name and no usable range; the current direct-construction test cannot catch the `(in <string>:N)` suffix issue.
- No explicit test that the new `TemplateRender` Display does not contain `(in <string:` when the inner error has a name.

## 6. Codebase Alignment

**Violations**
- None material.

**Alignment**
- `WorkflowError` is binary-only (`src/main.rs:18`), so adding variants is not a public-library API break.
- The proposed accessors on `TemplateError` also stay inside the binary-only template module.
- The design correctly avoids changing the Backend trait or `BackendErrorKind`; this is workflow-layer error reporting, not backend error classification.
- The spec preserves the existing `interpolate_loop_vars` behavior and does not widen scope into `for_each` rendering.
- The use of a dedicated hint only for `unexpected end of comment` is a narrowly scoped special case, not a general guessing mechanism.

## 7. Blind Spots

1. **AC3/AC4 exception not stated.**  
   The spec needs to carve out the AC4 hint from AC3's no-guessing rule.
2. **Real-runtime no-range undefined errors.**  
   AC6's direct unit test is synthetic. It does not exercise the actual MiniJinja error shape with a template name.
3. **Raw-block scanner interaction.**  
   The current edge case only tests a `{#` inside raw before a real `{#`; it does not test an earlier `{#` followed by a `#}` inside raw.
4. **Nested raw blocks.**  
   Not addressed. This may be acceptable if the implementation uses a scanner rather than one regex, but the spec should say which is required.
5. **Display suffix handling.**  
   The `(in <string>:N)` removal rule and the `detail() == None` rule conflict.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback

1. **Reconcile AC3 and AC4.**  
   Rewrite AC3 to say: "No error names a variable or construct taken from a location other than the MiniJinja error's own range, except for the unclosed-comment hint required by AC4." This removes the internal contradiction.
2. **Fix the `detail() == None` Display rule.**  
   Change it to use `format!("{}", err.kind())` when `detail()` is `None`, or explicitly require stripping MiniJinja's `(in <string>:N)` suffix before storing the message.
3. **Clarify raw-block scanning.**  
   State that both `{#` and `#}` occurrences inside skipped raw regions are ignored.
4. **Add one raw-block edge test.**  
   Test a template like `{# candidate\n{% raw %}#}{% endraw %}` and require the hint to name the earlier `{#`, not treat the raw `#}` as its closer.
5. **Consider splitting sub-task 3.**  
   If the raw-block scanner becomes nontrivial, separate it from kind dispatch so each sub-task stays under two hours.
6. **Add one real-runtime no-range test if feasible.**  
   A MiniJinja-produced `UndefinedError` with a template name but unusable range would catch the Display suffix issue that the synthetic test misses.
