# Spec Review: clo-655

**Reviewer**: Codex via Ollama (glm-5.3:cloud)
**Reviewed**: 2026-09-14
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is clear, specific, and well-scoped. It correctly identifies:

- the exact trigger (`{#` inside `${#OUTPUT}`),
- where it occurs (`.lok/workflows/design-review.toml:40`),
- why the whole review pipeline dies before any reviewer runs,
- why the symptom was previously misread as an unknown-variable problem rather than a syntax error,
- and the right architectural distinction between "template parse error" and "render error."

It matches the Linear issue and goes further in a useful way: it also recognizes that the same trap can affect Markdown heading attributes (`{#setup}`), so the fix belongs in the engine, not just in one workflow file.

One unstated assumption should be made explicit: **disabling standard Jinja comment syntax is a user-visible behavior change**, not just an internal bug fix. The spec justifies it by sweeping local workflows and finding no comments, but because this is a library and a CLI, other users may still rely on `{# ... #}`. That is acceptable given the evidence, but it should be called out in the release notes and docs.

## 2. Acceptance Criteria Review

**Strong**

- AC1–AC8 are specific and testable.
- AC1 and AC2 directly prove the new syntax behavior.
- AC3 is the critical end-to-end regression.
- AC5–AC8 correctly separate syntax, render, and undefined-variable errors.
- AC11 documents the new rule.
- AC12 matches the project's real gates (`make check` plus the MSRV check from CLO-638).
- The requirement that every new regression test must fail against the pre-change code is excellent and matches prior lessons in this repo.

**Gaps**

- AC9 and AC10 are useful but are **live, environment-dependent checks**. They depend on Ollama, Codex, Claude, and the user's local `~/.codex/config.toml`. That is acceptable as manual verification, but the spec should state the exact environment preconditions and note that a live failure unrelated to template rendering is not automatically a spec failure.
- AC7 says the undefined-variable hint must still be present, but the referenced existing test does not currently assert the hint text. If the intent is to preserve the hint, the test should assert it explicitly.
- There is no explicit acceptance criterion that `when` conditions and `interpolate_loop_vars` remain unchanged. The spec correctly says they are out of scope, but a small regression test would make that contract observable.
- AC4's `rg -n '\{%' .lok/workflows/` check is fine for the current tree, but it is a snapshot-style criterion. If new workflows are added later, the expected output will change. That is acceptable, but the spec should say "at implementation time."

## 3. Constraints Check

**Aligned**

- Keeping one `TemplateEngine` per runner matches the existing design.
- Preserving `{{ }}` and `{% %}` while only changing comments is the right minimal change.
- Requiring `custom_syntax` on the existing optional `minijinja` dependency is correct; `aho-corasick` is already in `Cargo.lock` through `regex`.
- Mapping by variant rather than by source-range presence is the correct fix for the CLO-656 misdiagnosis.
- The `Must-not` items correctly preserve `evaluate_condition` and `interpolate_loop_vars`, both of which have documented legacy behavior.
- Excluding unrelated shell-quoting defects (CLO-631, CLO-649) is the right scope boundary.
- The `Escalate when` section is well chosen.

**Concerns**

- The global removal of comment syntax should be documented as a deliberate, user-visible behavior change.
- The suggested sentinel delimiter is unlikely to collide, but any sentinel can collide by definition. The spec already says a collision is unsupported, which is good; a slightly more improbable sentinel would reduce the risk further.
- `TemplateRender { workflow, step, detail }` omits a line number even though MiniJinja render errors can carry one. That may be acceptable, but it is worth considering `Option<usize>` for symmetry with `TemplateSyntax`.
- The spec should explicitly require release notes or a README note for the comment-syntax change.

## 4. Decomposition Quality

**Well-scoped**

- Each sub-task is well under two hours.
- Dependencies are clearly identified.
- The split between engine behavior, error reporting, integration test, workflow fix, docs, and live verification is clean.
- Sub-task 4 is correctly independent because it fixes the workflow even before the engine change lands.

**Issues**

- Sub-tasks 1 and 2 both touch `src/template/mod.rs`, albeit in different regions. That is acceptable, but implementers should sequence them to avoid merge friction.
- Sub-task 6 depends on 1–4, but the live verification would also benefit from sub-task 2 landing first, because if a live run fails, the new error reporting is what makes the failure actionable.
- AC9 and AC10 may still be blocked by the separate `~/.codex/config.toml` / `model_reasoning_effort` issue noted in the prior CLO-653 review. The spec should mention that possibility so implementers do not misread a live-run failure as a template regression.

## 5. Evaluation Coverage

**Covered**

- The evaluation table maps well to AC1–AC4 and AC5–AC12.
- Unit tests cover the engine and error-mapping behavior.
- The integration test covers the real failure path.
- Static checks verify the workflow change.
- Live runs verify the actual review pipeline.
- Edge cases are unusually thorough for a spec of this size.

**Gaps**

- No explicit test for `when` conditions under the custom syntax, even though `compile_expression` should be unaffected.
- No explicit test for `interpolate_loop_vars` under the custom syntax, even though the spec says it is out of scope.
- No explicit test for the sentinel token itself appearing in a template. The edge case is noted, but not included in the test table.
- No explicit test for an undefined-variable error with no usable source range. AC8 mentions it, but the table does not show how to construct that case.

## 6. Codebase Alignment

**Violations**

- No material codebase violations.
- The only architectural concern is the **global MiniJinja behavior change**, but the spec now explicitly designs it rather than smuggling it in, which resolves the earlier CLO-373 complaint about an out-of-scope delimiter change.

**Alignment**

- `src/template/mod.rs` currently creates a default `Environment`; the proposed change fits its constructor cleanly.
- `src/workflow.rs:2930-2944` and `src/workflow.rs:3117-3151` are correctly identified.
- The proposed `TemplateSyntax` and `TemplateRender` variants match the existing `WorkflowError` style.
- The documentation target (`docs/guides/lok-setup-guide.md`, "Template Variables") is the correct place for the new syntax note.
- `Makefile check` and the CI MSRV job are accurate.
- The `wc -c` replacement matches the form already used in `.lok/workflows/spec-review.toml`.
- The decision to keep `evaluate_condition` and `interpolate_loop_vars` unchanged matches their documented lenient behavior and existing tests.

## 7. Blind Spots

- **User-visible behavior change:** Removing standard Jinja comments is visible to external users, not just this repo. It should be documented in release notes.
- **Live-run fragility:** AC9 and AC10 can still be affected by the separate `model_reasoning_effort` issue from the previous review.
- **Sentinel collision:** Any sentinel can collide; the spec should note that collisions are intentionally unsupported.
- **Error detail symmetry:** `TemplateRender` does not carry a line number even when MiniJinja can provide one.
- **Undefined variables without a source range:** Mapping these to `TemplateRender` is the right "no arbitrary blame" decision, but it may be less helpful in rare cases. The spec should explicitly accept that tradeoff.
- **`interpolate_loop_vars` remains fail-open:** That is correctly out of scope, but it is still a blind spot for template diagnostics.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback

1. **Document the comment-syntax removal as user-visible.**
   Add a release note or README note stating that `{# ... #}` is no longer treated as a comment and is literal text.

2. **State live-run preconditions explicitly.**
   AC9 and AC10 should say what backend state is required and mention that failures unrelated to template rendering should be recorded as follow-ups.

3. **Make AC7 assert the hint.**
   Add an assertion for the `valid forms are ...` hint, not just the variable name.

4. **Add a regression test for `when` conditions.**
   A small test that a condition still evaluates correctly under the custom syntax proves the engine change is isolated to comments.

5. **Add a regression test for `interpolate_loop_vars`.**
   Even though it is out of scope, a simple test that `{{ item }}` still survives `protect_loop_vars` under the custom syntax would protect the existing contract.

6. **Consider adding `line: Option<usize>` to `TemplateRender`.**
   MiniJinja render errors can carry line information; omitting it loses useful context.

7. **Add the sentinel-collision case to the test table.**
   The spec mentions it as an edge case, but it is not represented in the evaluation matrix.

8. **Mention the prior CLO-373 history.**
   A short note that this change is now explicitly scoped and designed, rather than out-of-scope leakage, would show why this time is different.
