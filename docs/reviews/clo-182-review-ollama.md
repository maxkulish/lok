# Design Review: clo-182

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-03-31
**Pipeline**: lok design-review

---

The review is valid. It contains a clear verdict (APPROVE_WITH_SUGGESTIONS), structured sections with assessment language, and 7 numbered actionable items across 3 priority tiers.

**APPROVE_WITH_SUGGESTIONS**

---

### Key Findings

**Strengths**

- **Thorough discovery**: Prior research section explains key decisions (`raw_output` as `Option<String>`, `FailureType` scoping) with clear rationale
- **Clean abstraction**: `ShellOutput` as private implementation detail, `Option<T>` for all new fields prevents breaking changes
- **Well-scoped**: Intentionally limits `FailureType` to validation failures, defers execution failures to existing error handling
- **Pragmatic threading**: Honest assessment of which paths get stderr/exit_code vs. which don't (consensus/for_each deferred)
- **Clear acceptance criteria**: Grep-verification commands are practical and testable

---

### Actionable Items (Priority Order)

**P1 - Address Before Implementation**

1. **Add serde derive macros** if serialization is intended (mentioned in Dependencies but missing from code snippets)
2. **Add integration test for `run_shell()` behavior change**: Separating stdout/stderr is behavioral - one test verifying shell step with non-empty stderr populates `StepResult.stderr` separately

**P2 - Consider for Implementation**

3. **Document stderr size limits**: Consider a practical limit or truncation strategy for large stderr output
4. **Add migration note for shell step users**: stderr no longer appearing in `{{ steps.X.output }}` is a breaking change
5. **Clarify `raw_output` semantics**: Document what happens when validation reads but doesn't mutate output

**P3 - Minor Nits**

6. **Windows signal handling**: Note whether this is Unix-focused for now
7. **Typo in Evaluation table**: Verify grep pattern with unescaped `|` works as written

---

### Summary

Well-structured design document with good architectural decisions and honest scoping. Ready for implementation after addressing P1 items (serde derives if serialization is planned, one integration test for the `run_shell()` behavioral change).
