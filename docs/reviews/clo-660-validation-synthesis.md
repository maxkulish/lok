# Pre-PR validation: clo-660

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-09-15
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned FAIL with one MEDIUM finding about evidence auditability. No defect found in the doc changes themselves. |
| Claude fallback | SKIPPED | Not run because the Codex review succeeded. |

## Verdict
PASS_WITH_NOTES

Independent re-check of the branch, run for this synthesis: Evaluations 3, 3b, 3c, 5, 6, 13, 14 and 15 all return the spec's expected result; the tag `v20260914.0.0` is on origin; `git diff --check` is clean; the workflow YAML parses; no new em dashes were added. The evidence directory the YAML refers to exists on disk at `/tmp/claude-501/-Users-mk-Code-orchestrator-lok--fix-clo-660-docs/10a87299-32d3-4dc3-a89c-f978beb9be14/scratchpad/clo-660-evidence/` and holds every file the YAML names. Both probe `check.log` files end in `Finished`, `doctest.log` reports 1 passed and 1 ignored, the before/after warning lists are byte-identical, the install probe's `bin/` holds `lok` and `lokomotiv`, and `owners.json` lists only `ducks`. The documented changes implement AC1 through AC15 as written.

## Must Fix Before PR
- **Resolve the evidence path placeholder in `docs/status/clo-660-workflow.yaml`.** The spec's verification method says to record each command's log path, and `evidence_dir: <session scratchpad>/clo-660-evidence` is a placeholder, not a path. Replace it with the absolute scratchpad path above so a reader can tell "kept outside the repo" from "never produced". While there, add the missing exit codes to the grep-style entries that only say "no output" (3b, 6, 10, 12), since the spec asks for an exit code per command. One YAML edit, one commit.

## Out of Scope / Deferred
- **Logs are not review-accessible from the PR.** The spec deliberately places `EV` in the session scratchpad and only requires the logs to survive until merge, so committing them was never in scope. If the project wants PR-reviewable evidence in future, that is a spec-template change, not a CLO-660 fix.
- **Known follow-ups the spec already records:** `.pi/agents/ops-reviewer.md:52` still tells the ops reviewer to run `cargo install lokomotiv`, `docs/DEPENDENCIES.md` still says nobody can push to `main` although the `CI Gate` required check was removed on 2026-08-07, and the frozen `Cargo.toml` `documentation` and `authors` fields wait on the naming decision. All three are tracked in the notes or the PROJECT.md naming row.

## False Positives / Tooling Artifacts
- **"None of the referenced logs exists in the checkout."** True but not a defect: the spec puts them outside the checkout by design, and they exist at the scratchpad path with the recorded results. Codex could not see the session scratchpad.
- **"No review-accessible evidence that the tag existed on origin or that the owner API returned only `ducks`."** Both re-verified here: `git ls-remote --tags origin v20260914.0.0` returns the tag, and the retained `owners.json` lists only `ducks`.
- **"Several entries record only summaries."** Evaluations 1, 2, 3, 3c, 7, 8 and 11 do carry exit codes; the remaining entries are greps whose result is their output, which the YAML records verbatim. The only genuine gap is covered by the Must Fix item.

## Recommendation
PROCEED_WITH_FIXES. One bounded fix: replace the `<session scratchpad>` placeholder in the workflow YAML with the absolute evidence path and add exit codes to the four "no output" entries, then commit. The documentation changes are correct against the spec and need no edits, and Codex's FAIL rests entirely on evidence the reviewer could not reach rather than on anything wrong in the branch.
