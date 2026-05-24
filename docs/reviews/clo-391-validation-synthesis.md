# Pre-PR validation: clo-391

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

I verified Codex's findings against the actual code and additionally ran clippy which surfaces three explicit AC violations.

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Reported FAIL with 5 findings; cargo test blocked by read-only sandbox |
| Gemini | REVIEW_FAILED | Gemini CLI refused (untrusted folder, `GEMINI_CLI_TRUST_WORKSPACE` not set) |
| Claude fallback | SKIPPED | Codex succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR

- **`cargo clippy -- -D warnings` fails — directly violates AC** (verified locally, not in Codex report). Three errors:
  - `src/backend/claude.rs:602` — `await_holding_lock` in `test_probe_cli_present_with_json_support`: `PATH_LOCK` (`std::sync::Mutex`) is held across the `probe_cli().await`. Drop the guard before `.await` (e.g. lock, snapshot needed state, drop, then await) or switch to `tokio::sync::Mutex`.
  - `src/backend/claude.rs:628` — same pattern in `test_probe_cli_present_without_json_support`. Same fix.
  - `src/backend/context.rs:193` — `bool_assert_comparison`: replace `assert_eq!(deserialized.available, false)` with `assert!(!deserialized.available)`.
- **Probe Commands leak on timeout** (Codex HIGH #2, confirmed). `src/backend/claude.rs:345` and `:416` spawn `tokio::process::Command` without `.kill_on_drop(true)`. The sibling `query_cli` at `:236` already sets it; the probes are inconsistent and a hung `claude --version`/`--help` keeps running past the 2s budget. Add `.kill_on_drop(true)` to both probe spawns.
- **`eprintln!` should be `tracing::warn!` / `tracing::debug!`** (Codex LOW, confirmed at `:427`, `:431`, `:446`). Design constraints section explicitly mandates "Log probe timeouts at `warn` level, version parse failures at `debug` level." Trivial swap, plus add a `warn!` for the silent `--help` timeout path inside `get_help_output`.

## Out of Scope / Deferred

- **`ANTHROPIC_API_KEY` unset never reaches `probe_api()` via warmup** (Codex HIGH #1). Real and accurate: `ClaudeBackend::new` (`claude.rs:76`) returns an error before the probe runs, so `warmup_backends` logs and caches nothing. However: (a) this is pre-existing constructor behavior not introduced by this change, (b) the probe itself meets every unit-level AC, (c) fixing it requires touching backend construction semantics that other callers depend on. Worth a follow-up ticket (likely as part of CLO-393 doctor renderer wiring) — do not block this PR.
- **`#[serde(default)]` on `mode`/`diagnostic`** (Codex MEDIUM). `HealthCache` is `RwLock<HashMap>` in-memory only; no persisted JSON to break. Design listed this as an escalation point only "if it appears." It hasn't appeared. Add later if/when HealthStatus gets persisted.
- **Multi-line semver parsing** (Codex LOW). `parse_semver_line(stdout.lines().next()...)` only scans line 1. Real `claude --version` output is single-line; design wording ("anywhere in output") is preference-level. Defer unless a real claude release breaks it.

## False Positives / Tooling Artifacts

- **Codex couldn't run `cargo test claude`** (sandbox `Operation not permitted` on `.cargo-lock`). I ran it locally: **13 passed, 0 failed** including all 6 new probe tests. Not a real gap.
- **Gemini review entirely empty**: CLI bailed out because the worktree isn't in Gemini's trusted-folders list (`GEMINI_CLI_TRUST_WORKSPACE` unset, no `--skip-trust`). Tooling environment issue, no signal about the code.

## Recommendation

PROCEED_WITH_FIXES — one bounded iteration covering: (1) drop the `PATH_LOCK` guard before `.await` in the two CLI probe tests (or switch to `tokio::sync::Mutex`), (2) replace `assert_eq!(..., false)` with `assert!(!...)` in `context.rs:193`, (3) add `.kill_on_drop(true)` on both probe spawns in `claude.rs` and the `get_help_output` spawn, (4) swap the three `eprintln!` calls for `tracing::warn!`/`debug!` and add a `warn!` on the silent `--help` timeout. Re-run `cargo test claude && cargo clippy --all-targets -- -D warnings` to confirm the AC line is green, then create the PR. The API-key-via-warmup gap should be filed as a separate follow-up against CLO-393.
