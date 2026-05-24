# Review Synthesis: CLO-392

**Synthesized**: 2026-05-24
**Pipeline**: lok design-review (manual fallback)
**Reviewers**: Gemini 3.1 Pro, Codex/Ollama (glm-5:cloud)

---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | OK | APPROVE_WITH_SUGGESTIONS — 5 actionable items |
| Ollama/Codex | OK | APPROVE_WITH_SUGGESTIONS — 6 actionable items |
| Claude fallback | SKIPPED | Not needed; both external reviewers succeeded |

---

## Agreement (High Confidence)

Both reviewers independently identified these concerns:

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Missing exit status check** — `codex --version` output is parsed without verifying `output.status.success()`. A crashing wrapper or broken binary could yield garbage or stderr in stdout. | High |
| 2 | **Warning location** — Warnings for unusable flags should be emitted in `workflow.rs` (engine layer), not inside `CodexBackend::health_check`, to keep the backend decoupled from step configurations. | Medium |
| 3 | **Version parser robustness** — Parser should scan the entire output for the first `\d+\.\d+\.\d+` triplet, not anchor to start/end, to tolerate wrapper-script warnings or prefixes. | Medium |

---

## Disagreement (Needs Human Decision)

None. Both reviewers are in full agreement on all substantive points.

---

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 4 | **Synchronous IO in async context** — `which::which` blocks the Tokio executor; should be wrapped in `spawn_blocking` or documented as acceptable (only once per warmup). | Gemini | Low |
| 5 | **Use `tokio::process::Command`** — Verify the implementation uses the async `tokio::process::Command`, not `std::process::Command`, or the timeout wrapper is ineffective. | Gemini | Low |
| 6 | **Log stderr on failure** — Capture stderr from the version probe and include it in `BackendError::Unavailable` notes for diagnostics. | Ollama | Low |
| 7 | **Flag matrix maintenance ownership** — Add a note about who keeps the matrix in sync when new Codex CLI versions are released. | Ollama | Low |
| 8 | **Add non-zero exit code test case** — Test the path where the binary exists but `--version` returns a non-zero exit code. | Ollama | Low |
| 9 | **Code snippet artifacts** — The "After" snippet in the design doc has truncation artifacts and malformed `compare_versions` signature; fix before implementation. | Ollama | Cosmetic |

---

## Consolidated Verdict

**Overall: APPROVE_WITH_SUGGESTIONS**

Both reviewers independently reached the same verdict. The design is sound, well-scoped, and ready to implement after addressing the high-severity exit-code check and the medium-severity warning-location / parser-robustness items.

---

## Priority Actions

1. **(High)** Add `output.status.success()` check before parsing version; fail closed on non-zero exit.
2. **(High)** Implement the warmup warning in `workflow.rs` (or `Engine::warmup_backends`), not inside the backend probe.
3. **(Medium)** Make `parse_version` scan the entire first line for the first `major.minor.patch` triplet.
4. **(Medium)** Capture and log stderr when `codex --version` fails, for diagnostic context.
5. **(Low)** Document that `which::which` is synchronous blocking but acceptable here (warmup-only,single-threaded).
6. **(Low)** Add a note about matrix maintenance (sync with `docs/investigations/codex-quick-ref.md` on release).
7. **(Low)** Add test case: binary exists but `--version` exits non-zero.
8. **(Cosmetic)** Fix design doc code snippets (truncation, `compare_versions` signature).
