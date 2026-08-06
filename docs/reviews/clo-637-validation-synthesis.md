# Pre-PR validation: clo-637

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-08-06
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Completed with verdict FAIL; ran without live GitHub API access, so PR-#84-hosted evidence was marked UNVERIFIED rather than checked |
| Claude fallback | SKIPPED | Codex review succeeded |

## Verdict

PASS_WITH_NOTES

Codex's FAIL rested on its two MEDIUM findings. On verification, one (bot-identity substring match) is behavior the approved spec explicitly mandates and the first validation synthesis already ruled non-blocking; the other (workflow-state phase pointer) is real but a bounded metadata edit. The implementation itself matches the spec closely - dual-shape gate, fail-closed timeout, freshness-plus-commit pairing in every path, identical logic in both files - and PR #84's body contains the live probe evidence and raw JSON snapshots the spec's verification method requires (I fetched it directly; Codex could not). No design divergence, no pivot. One bounded fix iteration covers everything.

## Must Fix Before PR

- **Workflow state misdispatches on resume** - confirmed at HEAD `f2f3f06`: `docs/status/clo-637-workflow.yaml:9` still says `current_phase: implement` with `implement.status: in_progress`, while `phases.pr` is in progress with PR #84 and two addressed review cycles in history. The orchestrator dispatches from this pointer, so resuming CLO-637 would re-enter implementation. Fix: set `current_phase: pr`, mark implement complete, add `d2e30a6`/`f2f3f06` to the recorded lineage, and append the live re-review gate outcome for head `d2e30a6` once it lands (history's last entry says that poll - the first live exercise of the fix itself - was still running).
- **Minor doc alignment in `docs/DEPENDENCIES.md`** (bundle into the same iteration; the branch already touches this file): the CLO-637 bullet at line 45 still states Qodo "submits no new review" flatly - pre-existing CLO-633 prose, but it now contradicts the spec's central two-shape finding; and the line-2 header's "ten open tasks" doesn't reconcile with nine Ready-table rows plus CLO-637 (in review) and CLO-649 (filed but absent from the table). Two one-line wording fixes.

## Out of Scope / Deferred

- **Bot identity anchored to a `qodo` substring** (both gate files). Real hardening opportunity, but the spec's edge cases explicitly constrain the implementation to *keep* `test("qodo")` (spec line 85), Codex itself acknowledged this, and the first validation synthesis already judged it non-blocking - changing it now would deviate from the approved spec, not conform to it. Practical exposure is low on a single-maintainer repo: an attacker needs a qodo-substring login plus the exact completion phrase plus the full 40-char head SHA. Defer: file a follow-up to require the exact app login and `.user.type == "Bot"`, landing with CLO-623's script extraction so it's fixed once in tested code rather than twice in markdown.

## False Positives / Tooling Artifacts

- **`?since=` prefilter "stricter than the client gate"** - the stated failure scenario (completion comment created in the same second as the `/agentic_review` request) is unreachable: a Qodo pass takes 3-4 minutes and every observed completion landed ≥2m43s after its request. Even granting the strictly-exclusive reading of GitHub's `since` docs, a boundary miss degrades to the 600s fail-closed timeout the spec mandates, never to a false pass. No change needed.
- **Live AC1-AC4 evidence "UNVERIFIED" / missing from the branch** - a limitation of Codex's sandbox, not of the change. I fetched PR #84's body: it contains the verification summary (10/10 probes, live `wait_for_bot_review` exercise, `bash -n` + shellcheck) and the raw JSON snapshots of the PR #71/#80 reviews and comments records, satisfying the spec's evidence requirement.
- **"Uncommitted workflow-status update"** - since resolved: the working tree is clean; the update Codex saw was committed as `f2f3f06`. The residual substance (stale phase pointer, unrecorded gate outcome) is captured in Must Fix above.

## Recommendation

PROCEED_WITH_FIXES. One bounded iteration before the PR transition: (1) correct `docs/status/clo-637-workflow.yaml` - `current_phase: pr`, implement phase closed with `d2e30a6`/`f2f3f06` recorded, and the live gate outcome for head `d2e30a6` appended once the poll resolves; (2) amend `docs/DEPENDENCIES.md` - reword the CLO-637 bullet to the two-shape rule and reconcile the open-task count (add CLO-649 to the Ready table or adjust the header). Additionally, file a follow-up Linear issue for the bot-identity hardening (exact Qodo/Copilot app logins + `.user.type == "Bot"`) targeted at CLO-623's script extraction - it should not be patched inline here against the spec's explicit constraint. No user decision is required.
