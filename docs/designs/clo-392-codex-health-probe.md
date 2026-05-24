# Design: CLO-392 - FR-13: Codex health probe + version-aware unusable-flag matrix

## Problem

The `CodexBackend::health_check` introduced in CLO-371 is a PATH-only stub (`which::which`). At runtime, users with an older Codex CLI (e.g. v0.118.0) receive opaque failures when newer flags like `--output-schema` or `--ignore-user-config` are injected by the workflow engine. There is no upfront validation; the first indication of a version mismatch is a non-zero exit code from `codex exec` deep inside a step. This wastes tokens and time. We need an active probe that runs `codex --version` at warmup, parses the version, and surfaces a list of unusable flags per the canonical flag matrix so the engine can emit a clear warning before any step executes.

## Goals / Non-goals

**Goals**
- Replace the PATH-only `health_check` with an active probe: spawn `codex --version` with a 2 s timeout, parse the version string.
- Extract the canonical flag-version matrix from `docs/investigations/codex-quick-ref.md` into a `const` table inside `src/backend/codex.rs`.
- Populate `HealthStatus.version` (as a plain `String`) and `HealthStatus.unusable_flags` with every flag the detected version does **not** support.
- Cache the result through `BACKEND_CACHE` (CLO-388); repeat checks within a single run are free.
- Log a warning at warmup when a workflow step requests an unusable flag, naming the flag and minimum required version.
- Cover the version matrix with unit tests for boundary versions: missing binary, `< 0.118.0`, `0.118.0`, `0.119.0`, `0.122.0`.

**Non-goals**
- Changing the `Backend` trait or `HealthStatus` struct shape (both landed in CLO-371/CLO-388).
- Implementing background polling or persistent on-disk health cache.
- Adding a new dependency crate for semver parsing (manual `major.minor.patch` parsing is sufficient).
- Warning at *step execution* time (warmup-time warning is the first gate; step-time failure mode remains unchanged).

## Architecture

The change is contained to `src/backend/codex.rs`. No other module is modified except for a new log site in the existing warmup path (`src/backend/mod.rs::Engine::warmup_backends` already logs warnings for `Err(BackendError)` and `Ok(status)`; we will add a secondary warning scan after cache population in `src/workflow.rs` or keep it inside the backend probe.

```text
┌─────────────────────────────────────┐
│  Engine::warmup_backends()          │
│  (src/backend/mod.rs)               │
│       │                             │
│       ▼                             │
│  Backend::health_check()            │
│  (src/backend/codex.rs)            │
│       │                             │
│       ├── which::which("codex")     │
│       │      missing ──► unavailable  │
│       │                             │
│       ├── tokio::process::Command   │
│       │   .arg("--version")         │
│       │   .timeout(2s)             │
│       │                             │
│       ├── parse stdout              │
│       │   (e.g. "codex-cli 0.119.0")│
│       │                             │
│       ├── evaluate FLAG_MATRIX      │
│       │   via compare_versions()    │
│       │                             │
│       └── return HealthStatus {       │
│             version: Some(v),       │
│             unusable_flags, ... }    │
└─────────────────────────────────────┘
```

New types and constants inside `src/backend/codex.rs` (module-private):

```rust
/// One entry in the flag matrix.
struct FlagRequirement {
    flag: &'static str,
    min_version: (u32, u32, u32), // (major, minor, patch)
}

const FLAG_MATRIX: &[FlagRequirement] = &[...];
```

The `compare_versions` helper is a pure function: `fn compare_versions(installed: (u32, u32, u32), required: (u32, u32, u32)) -> bool` returning `true` when the installed version is `>=` the required version.

`HealthStatus::unusable_flags` is a `Vec<String>` whose values are the long flag names from the matrix (e.g. `"--output-schema"` rather than the short `-o`). This matches the field names used in the TOML step config so the warning message can quote the exact config key that is unsupported.

## Public API surface

No new public API. The following signatures remain unchanged; only their bodies change.

**Before (current)**
```rust
// src/backend/codex.rs
async fn health_check(&self) -> Result<HealthStatus, BackendError> {
    if which::which(&self.command).is_ok() {
        Ok(HealthStatus::new_available())
    } else {
        Err(BackendError::Unavailable { ... })
    }
}
```

**After (proposed)**
```rust
// src/backend/codex.rs
use tokio::process::Command; // NOT std::process::Command

async fn health_check(&self) -> Result<HealthStatus, BackendError> {
    // 1. PATH probe (fast failure) — synchronous but only during warmup
    let cmd = which::which(&self.command).map_err(|_| BackendError::Unavailable { ... })?;

    // 2. Version probe with 2 s timeout
    let output = tokio::time::timeout(
        Duration::from_secs(2),
        Command::new(&cmd).arg("--version").output(),
    ).await.map_err(|_| BackendError::Unavailable { ... })??;

    // 3. Validate exit status before parsing
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BackendError::Unavailable {
            backend: "codex",
            reason: format!("codex --version exited {:?}: {stderr}", output.status.code()),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let (major, minor, patch) = parse_version(&stdout)
        .map_err(|e| BackendError::Unavailable { ... })?;

    let unusable = FLAG_MATRIX
        .iter()
        .filter(|req| !compare_versions((major, minor, patch), req.min_version))
        .map(|req| req.flag.to_string())
        .collect();

    Ok(HealthStatus {
        available: true,
        version: Some(format!("{major}.{minor}.{patch}")),
        unusable_flags: unusable,
        ..HealthStatus::new_available()
    })
}
```

`HealthStatus` (already public in `src/backend/context.rs`) gains data through existing fields only:
- `version: Option<String>` — populated with `"0.119.0"`-style plain string.
- `unusable_flags: Vec<String>` — populated with unsupported flag names.

## Assumptions

- `codex --version` prints a line containing a semver-like triplet (e.g. `codex-cli 0.119.0`). We only need the triplet, not a full SemVer parser. **Confidence: high.** Verified against CLO-373 fixture data (`tests/fixtures/codex/README.md` records `0.130.0`).
- The flag matrix from `docs/investigations/codex-quick-ref.md` is authoritative and does not change within a single `lok` release. **Confidence: high.** Verified by issue description and PRD v5 §4.
- The `BACKEND_CACHE` write lock will not be poisoned during warmup because `warmup_backends` is called exactly once per `lok` invocation from a single `async` task. **Confidence: high.** Based on CLO-388 architecture (single caller, no threads).
- Tests that mock the global `BACKEND_CACHE` will run under a shared test mutex to avoid concurrent state leakage. **Confidence: medium.** Lesson from CLO-389 L1; verified by adding `acquire_test_lock()` guard in new tests.
- `which::which` performs synchronous filesystem operations, but this is acceptable because `health_check` is only called during warmup from a single async task. **Confidence: high.** Existing behavior from CLO-371; no concurrent backend creation.
- The flag matrix inside `src/backend/codex.rs` must be kept in sync with `docs/investigations/codex-quick-ref.md` on every lok release that changes Codex CLI version requirements. **Confidence: high.** Verified: matrix is static per release.

## Test plan

**Unit tests in `src/backend/codex.rs`**

| Test name | Input version | Expected unusable flags (subset) |
|-----------|--------------|----------------------------------|
| `test_codex_health_missing_binary` | N/A (missing) | `available: false` |
| `test_codex_health_ancient_version` | `0.117.5` | `--json`, `--model`, `-s`, `--output-schema`, `-o`, `--ephemeral`, `--ignore-user-config`, `--ignore-rules` |
| `test_codex_health_0_118` | `0.118.0` | `--output-schema`, `-o`, `--ephemeral`, `--ignore-user-config`, `--ignore-rules` |
| `test_codex_health_0_119` | `0.119.0` | `--ignore-user-config`, `--ignore-rules` |
| `test_codex_health_0_122` | `0.122.0` | (empty) |
| `test_codex_health_unparseable_version` | `nightly` | `available: false`, reason in notes |
| `test_codex_health_timeout` | slow command (mock) | `available: false` |
| `test_codex_health_bad_exit` | exits non-zero | `available: false`, stderr in notes |

Testing strategy: `parse_version` and `compare_versions` are pure functions and tested directly with string inputs. The spawning and timeout paths are tested with a `CodexBackend` constructed with a fake command path using a shell wrapper that echoes a fake version string.

**Cache integration test**
- `test_codex_health_cached`: Call `health_check` twice; second call should not spawn a subprocess (verified by asserting no new mock command invocations, using the shared test lock from CLO-389).

**Integration / pre-PR gate**
- `cargo test` — all existing + new tests pass.
- `cargo clippy -- -D warnings` — clean.

## Migration / rollout

This change is purely additive to backend internals:
- No new CLI flags.
- No workflow TOML schema changes.
- `Backend::health_check` signature is unchanged.
- Existing behavior for backends other than Codex is untouched.

Rollback: If a critical bug is found, reverting this PR restores the PATH-only stub without breaking any caller because the `Backend` trait contract is unchanged.

## Open questions

1. ~~Should the warning at warmup be emitted inside `CodexBackend::health_check` or in `workflow.rs` after cache read?~~
   **Resolved:** Warnings must be emitted in `workflow.rs` (or the engine's validation phase), not inside the backend. The backend provides the `unusable_flags` list; the engine reconciles it against the workflow steps. This keeps the backend decoupled from step configurations.

2. **`codex --version` output format — prefix string?**
   CLO-373 fixtures show `codex-cli 0.130.0`, but the exact prefix may vary by install method (npm vs. binary). The parser should scan the entire first line for the first `\d+\.\d+\.\d+` substring and fail closed if none is found.

3. **Should the matrix be a `const` slice or a `lazy_static`/`OnceLock` map keyed by flag?**
   A `const` slice is sufficient today (8 entries, static). If the matrix grows to dozens of entries or needs runtime overrides, a map would be preferable. The design recommends the `const` slice for simplicity.
