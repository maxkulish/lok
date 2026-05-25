# Pre-PR validation: clo-393

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-25
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned findings; flagged JSON contract gap |
| Gemini | OK | Returned findings; only LOW-severity polish notes |
| Claude fallback | SKIPPED | At least one external reviewer succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- **JSON mode breaks contract when no backends are enabled** (src/main.rs:749-756). The empty-entries branch prints the colored string `"No backends configured."` *regardless* of `--output`, so `lok doctor --output json` can emit a non-JSON string. Design AC2 promises a valid JSON array; machine consumers will choke. Fix: in JSON mode, always route through `print_doctor_json(&entries)` (which yields `[]` when empty); only the table branch should print the human-readable "No backends configured." line.
- **Integration test masks the bug above** (tests/integration.rs:226-235). The `else` branch accepts `"No backends configured."` as a valid JSON-mode output, contradicting ST6's intent. Tighten the assertion: in JSON mode, stdout must parse as `serde_json::Value` and `is_array()` must be true — no fallback to the human string.
- **Trailing whitespace in `docs/status/clo-393-workflow.yaml:72`** trips `git diff --check`, which is part of the pre-merge hygiene the workflow guards. Strip it.

## Out of Scope / Deferred
- **Unknown `--output` values silently fall back to table** (src/main.rs:752). Real UX papercut, but no existing AC requires strict validation and changing `output: String` to a `clap::ValueEnum` is a small but separable hardening change. Worth a follow-up ticket, not blocking.
- **Notes column collapses to `"..."` on terminals narrower than ~46 chars** (Gemini Finding 1). Graceful, non-panicking, and well outside any realistic terminal width. Defer.
- **Panic on poisoned `BACKEND_CACHE` RwLock** (src/main.rs:717, Gemini Finding 2). Lock poisoning in this CLI requires another thread to have panicked while holding the write lock, which itself would already have aborted the run. Accept current behavior; revisit if/when the cache gains concurrent writers.

## False Positives / Tooling Artifacts
- Codex's AC5 non-verification note is a tooling artifact (read-only sandbox prevented `cargo` runs), not a finding against the code. The pre-merge gate still needs to be executed locally before PR; that is the orchestrator's job, not a code defect.

## Recommendation
PROCEED_WITH_FIXES. Three bounded edits, all in this PR's scope: (1) in `src/main.rs` Doctor handler, dispatch on `output` before the empty check so JSON mode always emits `[]` instead of the yellow string; (2) tighten `test_doctor_json_output` in `tests/integration.rs` to require a parsed JSON array — drop the "No backends configured." escape hatch; (3) strip trailing whitespace from `docs/status/clo-393-workflow.yaml:72`. Re-run `cargo fmt --check && cargo clippy -- -D warnings && cargo test` and `git diff --check main...HEAD` to confirm before opening the PR.
