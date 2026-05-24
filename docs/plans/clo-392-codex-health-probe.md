# Plan: CLO-392 FR-13: Codex health probe + version-aware unusable-flag matrix

## Context
- **Design:** [docs/designs/clo-392-codex-health-probe.md](../designs/clo-392-codex-health-probe.md)
- **Reviews:** [docs/reviews/clo-392-design-synthesis.md](../reviews/clo-392-design-synthesis.md)
- **Linear:** https://linear.app/cloud-ai/issue/CLO-392

---

## Sub-tasks

### ST1 Implement pure version helpers (`parse_version` + `compare_versions`)
**Files:** `src/backend/codex.rs`
**Description:**
- Add module-private `fn parse_version(s: &str) -> Result<(u32, u32, u32), BackendError>` that scans the input for the first `\d+\.\d+\.\d+` triplet.
- Add module-private `fn compare_versions(installed: (u32, u32, u32), required: (u32, u32, u32)) -> bool`.
- Add `struct FlagRequirement { flag: &'static str, min_version: (u32, u32, u32) }`.
- Add `const FLAG_MATRIX: &[FlagRequirement]` seeded from `docs/investigations/codex-quick-ref.md` (8 entries).
**Acceptance:** `cargo test parse_version` passes (7 cases: happy path, leading noise, nightly input, `0.117.5`, `0.118.0`, `0.119.0`, `0.122.0`).
**Estimate:** S

### ST2 Wire `health_check` with `tokio::process::Command` + timeout, exit-code + stderr validation
**Files:** `src/backend/codex.rs`
**Description:**
- Switch `health_check` from PATH-only stub to active probe using `tokio::process::Command`.
- Add 2 s timeout via `tokio::time::timeout`.
- Check `output.status.success()` before version parsing; on failure return `Unavailable` with stderr in the note.
- Integrate `FLAG_MATRIX` / `compare_versions` to populate `HealthStatus.unusable_flags`.
- Populate `HealthStatus.version` as `Option<String>`.
**Acceptance:** `cargo test codex_health --features test` passes (integration tests with fake shell wrapper for version spoofing + timeout/bad-exit paths).
**Estimate:** M

### ST3 Add cache-aware integration test
**Files:** `src/backend/codex.rs`, `src/backend/context.rs`
**Description:**
- Add `test_codex_health_cached`: invoke `health_check` twice and assert no duplicate process spawns.
- Use the shared test lock pattern (`acquire_test_lock()`) from CLO-389 to guard `BACKEND_CACHE`.
**Acceptance:** `cargo test codex_health_cached` passes.
**Estimate:** S

### ST4 Emit warmup warnings in the engine layer
**Files:** `src/backend/mod.rs`, `src/workflow.rs`
**Description:**
- After `warmup_backends` populates the cache, scan each workflow step for Codex flags that appear in `health_status.unusable_flags`.
- Log a warning with the flag name and minimum required version (from `FLAG_MATRIX`).
- Keep the backend strictly decoupled from step configurations; the engine does the reconciliation.
**Acceptance:** `cargo test warmup_warning` passes (mocked backend with fake unusable flags + asserting warn logs).
**Estimate:** M

---

## Pre-merge gate
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`

## Risks
- `tokio::process::Command` may behave differently on Windows; CI covers Windows but design assumes Unix. If CI fails, fallback to a platform-aware wrapper.
- The `which::which` call is synchronous blocking inside async context. Acceptable for warmup (single caller, no threads), but if a refactor later calls `health_check` from multiple tasks, a hang risk exists.
- If Codex CLI changes `--version` output format in a future release, `parse_version` may fail closed (safe default). No action needed now, but matrix maintenance must match parser tolerance.
