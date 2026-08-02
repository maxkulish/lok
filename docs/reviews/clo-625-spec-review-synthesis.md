# Spec Review Synthesis: clo-625

**Synthesized**: 2026-08-02
**Pipeline**: lok spec-review

---

## Source Status

| Reviewer | Result |
|---|---|
| Gemini | Success |
| Ollama (Codex/glm-5.2) | **REVIEW_FAILED** — empty output, process returned only startup banner |
| Claude fallback | Skipped (external reviewer succeeded) |

Single valid source. No cross-validation was possible, so nothing below carries multi-reviewer confirmation — treat every finding as unverified by a second opinion.

## Agreement (High Confidence)

Not applicable — only one reviewer produced output.

## Disagreement (Needs Human Decision)

Not applicable — no second position to compare against.

## Findings (Single Reviewer: Gemini)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Pre-existing PR not handled.** If a prior run left a `docs/clo-XX-finalize` branch that gets reused, a PR may already be open for it. `gh pr create` will hard-fail. Spec should check `gh pr list --head <branch>` before creating. | High |
| 2 | **Empty-diff control flow undefined.** When aggregation files are already current there is nothing to commit, so no PR should open — but the Linear "Done" transition (moved after merge in sub-task 3) must still run. Spec never states this branch of the flow. | High |
| 3 | **AC4 has no failure path.** Spec says the command waits for `CI Gate` and merges, but doesn't say what happens if `CI Gate` fails: merge must abort and the Linear status update must not fire. | High |
| 4 | **Missing AC / test for the empty-diff case** — no evaluation verifies the task still reaches Done when no PR is created. | Medium |
| 5 | **Missing test for "branch exists AND PR already open"** — the compound edge case, follow-on to #1. | Medium |
| 6 | **Constraint/evaluation contradiction.** Constraints say "Escalate when: the finalize branch already exists"; Evaluation says the command "must say so and offer reuse or delete rather than failing". Wording should be unified to "prompt the user with reuse/delete options". | Medium |
| 7 | **`gh pr checks --watch` can hang indefinitely** on a GitHub Actions outage or stuck runner. No timeout or manual-intervention fallback specified. | Medium |
| 8 | **Completion summary rendering when PR is skipped** — `finalize_pr_url` will be absent on the empty-diff path; spec doesn't say whether it renders as "N/A" or is omitted. | Low |

Gemini found **no codebase-alignment violations**, and correctly noted that the requested Rust `Backend` trait / `BackendErrorKind` alignment check is inapplicable here — this task only touches Markdown agent-prompt scripts under `.claude/commands/`, so it rightly ignored those patterns.

It also called out genuine strengths: the problem statement maps the Linear description onto the actual cause (GitHub Ruleset 20153405) and enumerates every failing push site; AC6/AC7 test end-to-end against the live ruleset; and the decomposition's justification for needing two commits on the branch (one before `gh pr create`, one after) reflects real understanding of the toolchain.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Derived from a single reviewer. Findings 1–3 are behavioral gaps in the spec, not defects in the approach — the design holds, the error and no-op paths just aren't written down.

## Priority Actions

1. **Handle pre-existing PRs** — in the "finalize branch already exists" edge case, check `gh pr list --head docs/clo-XX-finalize` before `gh pr create`; reuse the existing PR if one is open. (#1, #5)
2. **Define the empty-diff path end to end** — state in both Decomposition and ACs that if there is nothing to commit, PR creation is skipped *and* the Linear status update (Step 5) still executes; add a matching evaluation scenario. (#2, #4)
3. **Add the CI-failure path to AC4** — on `CI Gate` failure, abort the merge and leave the Linear task in its pre-merge state. (#3)
4. **Reword the escalation constraint** to "prompt the user with options to reuse or delete the branch", matching the Evaluation section. (#6)
5. **Bound the CI wait** — give `gh pr checks --watch` a timeout or a documented manual-intervention fallback. (#7)
6. **Specify `finalize_pr_url` rendering** in the completion summary when no PR was created. (#8)

Worth noting: the Ollama reviewer failed at startup rather than on the content (Codex read stdin, printed its banner, and produced nothing). If a second opinion matters for this spec, that run is probably worth retrying before acting on the list above.
