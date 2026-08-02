# Spec Review: clo-625

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-08-02
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is exceptionally clear, precise, and completely defines the issue. It accurately maps the Linear task description to the underlying technical cause (GitHub Ruleset 20153405) and explicitly lists all failing push sites. It correctly identifies why a partial fix (only fixing Step 7) is inadequate.

## 2. Acceptance Criteria Review
**Strong**: AC1-AC3 and AC6-AC8 provide highly specific, measurable, and testable outcomes. The inclusion of an end-to-end test against the live ruleset (AC6/AC7) is excellent.
**Gaps**: 
- AC4 states the command waits for `CI Gate` and merges, but does not specify the failure path: if `CI Gate` fails, the merge should abort and the task state should remain incomplete.
- Missing an AC for the "empty diff" edge case (where aggregation files were already updated). If there's nothing to commit, no PR should be opened, but the task must still complete in Linear.

## 3. Constraints Check
**Aligned**: Constraints make excellent use of Must/Must-not/Prefer/Escalate categories. Avoiding `--admin` and ruleset bypasses aligns perfectly with standard security practices. Preserving worktree logic is critical.
**Concerns**: There is a minor contradiction between the constraints and evaluation sections. The constraint states "Escalate when: The finalize branch already exists", but the Evaluation Edge Cases section dictates "the command must say so and offer reuse or delete rather than failing". While "escalate" in agent terminology usually implies prompting the user, the wording in constraints should explicitly match the evaluation logic.

## 4. Decomposition Quality
**Well-scoped**: The sub-tasks are logical, ordered well, and appropriately scoped. The justification for ordering (why we need two commits on the branch, one before `gh pr create` and one after) is brilliant and shows deep understanding of the toolchain.
**Issues**: Sub-task 3 specifies moving the "Linear -> Done" update after the merge. However, if the PR creation is skipped entirely (due to the "nothing to commit" edge case), the sequence must ensure the Linear update still runs. This control flow nuance should be explicitly stated in the decomposition.

## 5. Evaluation Coverage
**Covered**: The `rg` tests cover the text-replacement ACs perfectly, and the end-to-end test ensures the flow actually works. The edge cases are well thought out.
**Gaps**:
- Missing a test scenario for when a finalize branch already exists *and* a PR is already open for it.
- Missing a test scenario for the "empty diff" path to verify that the Linear task still transitions to Done when no PR is created.

## 6. Codebase Alignment
**Violations**: None found.
**Alignment**: The specification aligns perfectly with the `.claude/commands/` architecture. *Note regarding instructions:* The prompt requested verifying alignment with the Rust `Backend` trait and `BackendErrorKind` patterns. Because this task exclusively modifies Markdown files acting as agent prompt scripts (and not the underlying Rust application code), those Rust-specific patterns are inapplicable here. The specification correctly ignores them.

## 7. Blind Spots
- **Pre-existing Pull Requests**: If a previous run aborted and left a finalize branch that is subsequently reused, a PR might already exist for that branch. Running `gh pr create` will fail. The script needs to check for an existing PR (e.g., `gh pr list --head docs/clo-XX-finalize`) before attempting creation.
- **CI Wait Timeout**: `gh pr checks --watch` can theoretically hang indefinitely if GitHub Actions experiences an outage or a runner gets stuck. A timeout or manual intervention fallback might be necessary.
- **Completion Summary without PR**: If the PR is skipped due to an empty diff, the completion summary will not have a `finalize_pr_url`. The spec should clarify how this field is rendered (e.g., "N/A" or omitted entirely) in that specific case.

## 8. Verdict
APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Handle Pre-existing PRs**: Update the "Finalize branch already exists" edge case to also check for an existing PR (using `gh pr list`) before running `gh pr create` to avoid CLI errors.
2. **Clarify Empty Diff Control Flow**: Explicitly state in the Decomposition and ACs that if PR creation is skipped due to an empty diff, the Linear status update (Step 5) must still execute before completing. Define how the completion summary handles the missing `finalize_pr_url`.
3. **Resolve Constraint Phrasing**: Clarify the "Escalate when: The finalize branch already exists" constraint to explicitly say "Prompt the user with options to reuse or delete the branch", matching the intent in the Evaluation section.
4. **Clarify CI Failure State**: Update AC4 to explicitly state that if `CI Gate` fails, the merge is aborted and the Linear task status update is not performed.
