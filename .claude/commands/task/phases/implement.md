# Phase: Implement

**Purpose**: Execute the implementation plan phase by phase, tracking commits and pushing to remote. Run external model validation before transitioning to PR.

**Entry conditions**: `current_phase: implement`

---

## Status: pending or in_progress

1. Update state: `phases.implement.status: in_progress`
2. **Invoke**: `/plan:implement CLO-XX`

3. After each phase completion within `/plan:implement`:
   - Update workflow state:
     - `phases.implement.last_phase_completed: [phase name]`
     - Add commit SHA to `phases.implement.commits[]`
   - **Push to remote**:
     ```bash
     git push origin feat/clo-XX-short-desc
     ```
   - Add history entry: `phase_completed` with details of phase name
   - Add history entry: `pushed_to_remote`

4. When `/plan:implement` reaches 100%:
   - Add history entry: `implementation_complete`
   - **Continue to Validation Gate** (Step 5)

---

## Step 5: Validation Gate (Codex + synthesis)

**After implementation is complete, before creating a PR**, run external model validation to catch issues Claude may have blind spots for.

### Run the gate through `lok`

The pipeline lives in `.lok/workflows/pre-pr-validation.toml`. Do **not** reinvent it inline:

```bash
# arg.1 = design doc, arg.2 = plan file, arg.3 = Linear task ID
lok workflow run pre-pr-validation docs/design-docs/clo-XX-design.md docs/plans/clo-XX-plan.md CLO-XX
```

**Anti-pattern**: hand-rolling `codex exec` in a shell block here. That bypasses the workflow's output validators, its Claude fallback, and the synthesis step, and it hardcodes models that drift. `CODEX_MODEL` (default `gpt-5.6-sol`) is the only override.

The workflow writes:

- `docs/reviews/clo-XX-codex-validation.md`
- `docs/reviews/clo-XX-validation-synthesis.md`
- `docs/reviews/clo-XX-claude-fallback-validation.md` (only when Codex fails)

Google models were removed from every review leg on 2026-08-03. The `gemini` backend still ships in lok; it is simply not part of this repo's review tooling.

### Display Results

```
VALIDATION GATE RESULTS (CLO-XX)
=================================

Codex (gpt-5.6-sol):
  Verdict: [PASS | PASS_WITH_NOTES | FAIL]
  Report: docs/reviews/clo-XX-codex-validation.md
  Key Findings: [top 3 findings]

Synthesis (binding):
  Verdict: [PASS | PASS_WITH_NOTES | FAIL]
  Report: docs/reviews/clo-XX-validation-synthesis.md
  Must Fix Before PR: [count]

Options:
1. [proceed]  - Continue to PR creation
2. [fix]      - Address findings before PR (required if FAIL)
3. [pause]    - Pause workflow

Your choice:
```

### Decision Handling

The **synthesis** verdict is the binding one, not the raw reviewer's:

- `PASS`: proceed to the PR phase
- `PASS_WITH_NOTES`: apply the `Must Fix Before PR` items in **one** bounded fix iteration, re-run the pre-merge gate, then proceed
- `FAIL`: stop and escalate to the user. Do not transition to PR

Maximum validation fix iterations: **1**. If fixes reveal more issues, record them and ask the user.

### Fallback

If the Codex leg fails, the workflow runs a Claude fallback reviewer and the synthesis still produces a verdict. If the workflow itself exits non-zero or the synthesis report is missing, treat the gate as failed - do not transition phases and do not hand-write review files.

### Update State

- `phases.implement.codex_validated: true`
- `phases.implement.codex_verdict: [verdict]`
- `phases.implement.codex_report: docs/reviews/clo-XX-codex-validation.md` (the fallback report when Codex failed)
- `phases.implement.validation_synthesis_report: docs/reviews/clo-XX-validation-synthesis.md`
- `phases.implement.validation_synthesis_verdict: [verdict]`
- Add history entry: `codex_validation_complete`

### Transition to PR

- `phases.implement.status: complete`
- `workflow.current_phase: pr`
- `workflow.status: in_progress`
- **Continue to PR phase**

---

## YAML Checkpoint (Required before transition)

Before signaling completion to the dispatcher, verify:
- `phases.implement.status: complete`
- `phases.implement.commits` is non-empty
- History contains `implementation_complete`
- `phases.implement.codex_validated` is set (true if ran, false if skipped/unavailable)
