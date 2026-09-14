# Spec Review Synthesis: clo-655

**Synthesized**: 2026-09-14
**Pipeline**: lok spec-review

---

Only one review came back: **Ollama succeeded**. The Claude fallback was **skipped** because Ollama succeeded, so it did not fail. No findings can be cross-checked, so the Agreement and Disagreement sections are empty. Every finding below comes from Ollama alone.

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| - | None. Only one reviewer ran. | - |

## Disagreement (Needs Human Decision)
| # | Topic | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|
| - | None. Only one reviewer ran. | - | - |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | Turning off `{# ... #}` comments changes behavior that users can see, in both the library and the CLI. It needs a release note and a docs note saying that `{#` is now literal text. The local workflow sweep does not cover outside users. | Ollama | Medium |
| 2 | AC9 and AC10 are live runs that need Ollama, Codex, Claude and the local `~/.codex/config.toml`. The spec does not list these preconditions. Runs can still break on the separate `model_reasoning_effort` config issue (CLO-653), which could be mistaken for a template regression. | Ollama | Medium |
| 3 | AC7 says the undefined-variable hint must stay, but the test it points to only checks the variable name. It does not check the `valid forms are ...` hint text. | Ollama | Medium |
| 4 | AC8 covers an undefined variable with no source range. The evaluation table does not say how to create that case, so the "no arbitrary blame" mapping to `TemplateRender` is not tested. The spec should also say it accepts the weaker diagnostic here. | Ollama | Medium |
| 5 | No regression test shows that `when` conditions (`evaluate_condition` / `compile_expression`) still work with the custom syntax. They are listed under Must-not, but nothing enforces that. | Ollama | Low |
| 6 | No regression test shows that `{{ item }}` still survives `protect_loop_vars` / `interpolate_loop_vars` with the custom syntax. | Ollama | Low |
| 7 | `TemplateRender { workflow, step, detail }` has no line number, though MiniJinja render errors can carry one. Adding `line: Option<usize>` would match `TemplateSyntax`. | Ollama | Low |
| 8 | The spec lists a template containing the sentinel as an edge case, but the evaluation table has no test for it. The spec already calls collisions unsupported. A less likely sentinel would lower the risk further. | Ollama | Low |
| 9 | AC4's `rg -n '\{%' .lok/workflows/` check depends on the files present today. It should say "at implementation time". | Ollama | Low |
| 10 | Sub-task 6 (live verification) should also depend on sub-task 2 (error reporting). If a live run fails, the new errors are what explain the failure. | Ollama | Low |
| 11 | Sub-tasks 1 and 2 both edit `src/template/mod.rs`. Doing them in order avoids merge conflicts. | Ollama | Low |
| 12 | A short note on CLO-373 would explain that the delimiter change is now deliberately scoped, where before it slipped in out of scope. | Ollama | Low |

## Consolidated Verdict
**APPROVE_WITH_SUGGESTIONS**

Ollama returned APPROVE_WITH_SUGGESTIONS and no reviewer returned NEEDS_REVISION. Ollama found no codebase conflicts. It confirmed that the file references (`src/template/mod.rs`, `src/workflow.rs:2930-2944`, `src/workflow.rs:3117-3151`), the `custom_syntax` feature, the docs target and the gates (`make check` plus the MSRV check) are all accurate.

## Priority Actions
No findings were confirmed by two reviewers, so this list is ordered by severity only.

1. **Document the comment-syntax removal.** Add a release note and a line in `docs/guides/lok-setup-guide.md` ("Template Variables") saying that `{# ... #}` is no longer a comment and is rendered as literal text.
2. **Add preconditions to AC9 and AC10.** List the backend and config state they need. Record live failures that have nothing to do with rendering, such as the `model_reasoning_effort` issue, as follow-ups rather than spec failures.
3. **Make the AC7 test check the hint text.** Assert the `valid forms are ...` string as well as the variable name.
4. **Add an AC8 row to the evaluation table.** Say how to create an undefined-variable error with no source range, and note that the weaker diagnostic is accepted.
5. **Add a regression test for `when` conditions** with the custom syntax.
6. **Add a regression test for `interpolate_loop_vars`** that checks `{{ item }}` survives `protect_loop_vars`.
7. **Add the sentinel-collision case to the evaluation table** and consider a less likely sentinel.
8. **Decide on `line: Option<usize>` for `TemplateRender`.** Add it, or say in the spec why it is left out.
9. **Update the sub-task order.** Add sub-task 2 as a dependency of sub-task 6, and have sub-task 2 follow sub-task 1 in `src/template/mod.rs`.
10. **Small wording fixes.** Add "at implementation time" to AC4 and a one-line CLO-373 note.
