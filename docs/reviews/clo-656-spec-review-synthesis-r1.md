# Spec Review Synthesis: clo-656

**Synthesized**: 2026-09-14
**Pipeline**: lok spec-review

---

Only the Ollama review ran. The Claude fallback was skipped because Ollama succeeded, so no second reviewer was available to agree or disagree. Every finding below comes from that one reviewer, and none of its claims were checked against the spec or the code.

## Agreement (High Confidence)
None. A second valid review is needed for this.

## Disagreement (Needs Human Decision)
None. A second valid review is needed for this.

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | AC3 and AC4 contradict each other. AC3 bans naming anything outside MiniJinja's error range, but AC4 requires a hint naming the unclosed `{#` opener, which sits outside that range. A strict AC3 test would fail a correct AC4 implementation. | Ollama | Medium |
| 2 | Two Display rules conflict. The constraints say to drop MiniJinja's `(in <string>:N)` suffix, but also say to use MiniJinja's `Display` as-is when `detail()` is `None`. For real `UndefinedError`s, that `Display` includes the suffix. | Ollama | Medium |
| 3 | AC6's no-range test builds the error by hand without a template name, so it cannot catch the suffix leak in #2. No test drives a real MiniJinja `UndefinedError` that has a template name and no usable range. | Ollama | Medium |
| 4 | The raw-block scanning rule does not say that a `#}` inside a skipped `{% raw %}` region must not close an earlier `{#`. The existing edge test only covers the reverse order (a `{#` inside raw, then a real `{#`). | Ollama | Low |
| 5 | No test checks that `TemplateRender` Display leaves out `(in <string>:` when the inner error has a template name. | Ollama | Low |
| 6 | Nested raw blocks (`{% raw %}{% raw %}{% endraw %}{% endraw %}`) are not addressed. A single non-greedy regex could match the wrong spans. The spec should either say nested blocks are unsupported or require a small scanner. | Ollama | Low |
| 7 | Sub-task 3 bundles four behaviors: kind dispatch, full-identifier extraction, the no-range fallback and the unclosed-comment hint. If the scanner grows, it may need its own sub-task. | Ollama | Low |
| 8 | The tests-first plan does not state that the tests cannot compile until sub-task 2 adds the new `WorkflowError` variants. | Ollama | Low |

The reviewer also confirmed these parts of the spec are sound:
- Placing the classification in `map_template_error` and leaving `from_minijinja` unchanged
- Keeping `UnknownVariable`'s Display byte-for-byte
- Following the existing `LazyLock<Regex>` and `truncate_utf8` patterns
- Keeping `evaluate_condition` semantics unchanged
- Keeping CLO-655 out of scope
- No public API break, since `WorkflowError` exists only in the binary

## Consolidated Verdict
**APPROVE_WITH_SUGGESTIONS**

The only valid review returned APPROVE_WITH_SUGGESTIONS. No reviewer returned NEEDS_REVISION.

## Priority Actions
1. **Reconcile AC3 and AC4.** Rewrite AC3 to: "No error names a variable or construct taken from a location other than the MiniJinja error's own range, except for the unclosed-comment hint required by AC4."
2. **Fix the `detail() == None` rule.** Use `format!("{}", err.kind())` in that case, or require stripping the `(in <string>:N)` suffix before the message is stored.
3. **Add a real-runtime no-range test.** Use a MiniJinja-produced `UndefinedError` with a template name, and assert that neither `UnknownVariable` nor `TemplateRender` Display contains `(in <string>:`.
4. **Clarify the raw-block scanning rule.** State that both `{#` and `#}` are ignored inside skipped raw regions.
5. **Add a raw-block edge test.** Use `{# candidate\n{% raw %}#}{% endraw %}` and require the hint to name the earlier `{#`.
6. **Decide on nested raw blocks.** Either declare them unsupported or require a scanner instead of a single regex.
7. **Make the test dependency explicit.** Note that the red-first tests depend on sub-task 2. Split sub-task 3 if the scanner turns out to be more than trivial.
