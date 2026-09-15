# Pre-PR validation: clo-632

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-09-15
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Verdict FAIL, 3 findings. Could not run cargo lanes (read-only sandbox); AC9 was re-run here instead |
| Claude fallback | SKIPPED | Codex review succeeded |

## Verdict
PASS_WITH_NOTES

The core gate matches the spec: all four keys checked from the raw value, violations accumulated, user value always wins via strip-before-merge, user and explicit layers untouched, single load path at `src/config.rs:675`, positive control in the end-to-end test. The one confirmed defect is a bounded escaping fix.

## Must Fix Before PR
- **Escape project-controlled backend names in the error (CONFIRMED).** `src/config.rs:590` formats `backends.{name}.{key}` with the raw TOML table key. Reproduced against the built binary with `[backends."\u001b[31mcodex"] command = "./evil"`: exit 1, and stderr contains the raw `\x1b` byte. This violates the spec's "Must" constraint that project-file text reaches the terminal only escaped. Fix: `format!("backends.{}.{key}", name.escape_default())`. Ordinary names such as `codex` are unchanged, so every existing `contains("backends.codex.command")` assertion still passes. Add one case to `project_gated_key_override_rejected` with an escaped name asserting the error has no raw `\u{1b}`.
- **Mark the README "Command Wrapper (NixOS/Docker)" examples as user-config-only.** AC8 requires it for every non-default gated example. The block at `README.md:501` sets three `command_wrapper` values with no marker. One sentence or comment line suffices.
- **Strip trailing whitespace at `docs/reviews/clo-632-spec-review-claude-fallback.md:29`.** `git diff --check main...HEAD` exits 2 on it.
- **Commit `docs/status/clo-632-workflow.yaml`** (30 uncommitted lines) so the PR carries the decision log the spec references.

## Out of Scope / Deferred
- Bedrock `enabled = true` spending ambient AWS credentials from the project layer (spec already assigns a follow-up ticket).
- Project `.lok/workflows/` shadowing user workflows by name (spec explicitly excludes; docs correctly say the gate covers config keys only).
- Setup guide still shows `api_key_env` on bedrock, which bedrock never reads (spec: record as follow-up, do not fix here).
- PR description needs a **Behaviour change** heading per the spec's rollout note. This is a PR-phase task, not a code fix.

## False Positives / Tooling Artifacts
- **"AC9 could not be reconfirmed."** Tooling limit on Codex's side. Re-run here on the branch: `cargo fmt --check` clean, `clippy --locked --all-targets -D warnings` clean, `cargo test --locked` 157 lib + all integration tests pass including `project_config_trust`, bedrock clippy clean, `clippy --no-default-features --lib --tests` clean.
- **Bedrock test lane flake.** The first `cargo test --features bedrock` run had 4 failures in codex/claude health-check tests. Rerun in isolation: 159 passed. Same lane on main: 159 passed. The branch does not touch `src/backend/`, and the failing run overlapped two other concurrent cargo invocations. Not a branch regression.
- AC7 verified: the repo's own `lok.toml` loads with an empty HOME, exit 0.

## Recommendation
PROCEED_WITH_FIXES. One bounded iteration: (1) escape the backend name with `escape_default` in the violation string and add one test case with an escaped name; (2) add a user-config-only note to the README Command Wrapper block; (3) remove the trailing whitespace in the claude-fallback review doc; (4) commit the workflow yaml. Then re-run `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` and `git diff --check main...HEAD` before the PR transition.
