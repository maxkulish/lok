# CLO-391: FR-13a — Claude dual-mode health probe (Api vs Cli)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-391/fr-13a-claude-dual-mode-health-probe-api-vs-cli
**Status**: Finalized
**Finalized**: 2026-05-24
**Approved By**: Team (Gemini + Ollama AI review, user-approved)
**Author**: Mk Km
**Created**: 2026-05-24

---

## Summary

Implement two distinct `health_check` paths on `ClaudeBackend` — one for the HTTP API client variant (`ClaudeMode::Api`) and one for the `claude` CLI variant (`ClaudeMode::Cli`) — and add a `mode: Option<String>` discriminator to `HealthStatus` so the `lok doctor` renderer (FR-14, CLO-393) can display both rows. The API probe stays offline (validates env vars + config only); the CLI probe shells out to `claude --version` and `claude --help` with a 2s budget. Both results are independently cached in `HealthCache` via a mode-qualified cache key.

---

## Background

The existing `ClaudeBackend` already splits into `ClaudeMode::Api` (reqwest client + API key) and `ClaudeMode::Cli` (shell command) variants, but its `health_check()` returns a bare `HealthStatus::new_available()` / `Unavailable` with no version info, no `--output-format json` detection, and no `mode` discriminator. Meanwhile the Ollama backend (CLO-389) already probes `/api/version` and `/api/tags` and populates `HealthStatus.version` + `.models`.

PRD v5 §4 FR-13a requires:
- **Api**: validate `ANTHROPIC_API_KEY` non-empty and `model` non-empty; cheap, deterministic, no network call.
- **Cli**: run `claude --version` with a 2s timeout; parse semver; check `claude --help` for `--output-format json`; cache the help-output parse per-version.
- **Discriminator**: `HealthStatus.mode = Some("api") | Some("cli")`.

This follows directly from the discovery report (`docs/discovery/clo-391.md`) which scored the baseline at 6/10 — sound struct but health_check needs complete rewrite.

### Prior Research

Discovery explored two approaches:
- **A — Inline per-variant probes** (chosen): Two private methods `probe_api()` / `probe_cli()` dispatched on `&self.mode`.
- **B — Extract probe structs**: Premature abstraction; rejected.

The discovery also identified that `HealthCache` keys must be mode-qualified (`"claude::api"`, `"claude::cli"`) so both rows are independently cacheable, and that the `--help` flag-detection result must be memoized per-version to avoid re-parsing on every warmup.

---

## Architecture

### Component Overview

```
ClaudeBackend
  ├── ClaudeMode::Api { api_key, model, client }
  │     └── probe_api()  → HealthStatus { mode: "api", version: None }
  └── ClaudeMode::Cli { command, model }
        └── probe_cli()  → HealthStatus { mode: "cli", version: semver }

HealthStatus (extended)
  ├── available: bool
  ├── version: Option<String>
  ├── mode: Option<String>         ← NEW field
  ├── auth_method: Option<String>
  ├── capabilities: Option<Value>
  ├── unusable_flags: Vec<String>
  └── models: Vec<ModelInfo>

HealthCache (mode-aware keying)
  ├── "claude"         → HealthStatus (current, single)
  └── "claude::api"    → HealthStatus (NEW)
  └── "claude::cli"    → HealthStatus (NEW)
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/backend/context.rs` | Modified | Add `mode: Option<String>` field to `HealthStatus` |
| `src/backend/claude.rs` | Modified | Rewrite `health_check()` with `probe_api()` / `probe_cli()` |
| `src/backend/mod.rs` | None | Cache keying is handled by the warmup loop using `backend.name()` — must distinguish claude::api vs claude::cli |

### Dependencies

- **Internal**: `HealthCache` (CLO-388), `HealthStatus` struct, `which` crate, `tokio::process::Command`, `semver` crate
- **External**: `claude` CLI binary (optional), `ANTHROPIC_API_KEY` env var (optional)

---

## Detailed Design

### Implementation Approach

**Approach A**: Add a `mode` field to `HealthStatus`, then implement two private methods on `ClaudeBackend` and dispatch on `&self.mode`.

**`HealthStatus` field addition** (`src/backend/context.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthStatus {
    pub available: bool,
    pub version: Option<String>,
    pub mode: Option<String>,          // NEW — "api" | "cli" | None
    pub diagnostic: Option<String>,    // NEW — human-readable failure reason (e.g. "ANTHROPIC_API_KEY not set")
    pub auth_method: Option<String>,
    pub capabilities: Option<serde_json::Value>,
    pub unusable_flags: Vec<String>,
    pub models: Vec<ModelInfo>,
}
```

Both `new_available()` and `new_unavailable()` preserve `mode: None` by default.

**`probe_api()`** (`src/backend/claude.rs`):

```rust
async fn probe_api(&self) -> Result<HealthStatus, BackendError> {
    match &self.mode {
        ClaudeMode::Api { api_key, model, .. } => {
            if api_key.expose_secret().trim().is_empty() || model.trim().is_empty() {
                let diag = if api_key.expose_secret().trim().is_empty() {
                    "ANTHROPIC_API_KEY not set or empty"
                } else {
                    "model config is empty"
                };
                return Ok(HealthStatus {
                    available: false,
                    mode: Some("api".into()),
                    diagnostic: Some(diag.into()),
                    ..Default::default()
                });
            }
            Ok(HealthStatus { available: true, mode: Some("api".into()), diagnostic: None, ..Default::default() })
        }
        _ => Err(BackendError::Config { message: "not API mode".into() }),
    }
}
```

**`probe_cli()`** (`src/backend/claude.rs`):

```rust
async fn probe_cli(&self) -> Result<HealthStatus, BackendError> {
    match &self.mode {
        ClaudeMode::Cli { command, .. } => {
            // 1. Check binary exists
            let path = match which::which(command) {
                Ok(p) => p,
                Err(_) => return Ok(HealthStatus { available: false, mode: Some("cli".into()), ..Default::default() }),
            };

            // 2. Run claude --version with 2s budget
            // Log at warn level on timeout, debug on version parse failure.
            let version_output = Command::new(&path)
                .arg("--version")
                .timeout(Duration::from_secs(2))
                .output()
                .await;

            // Log at warn level if --help times out (non-fatal).
            let version = match version_output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Parse semver from first line using regex to find X.Y.Z anywhere
                    parse_semver_line(stdout.lines().next().unwrap_or(""))
                }
                Err(e) => {
                    tracing::warn!("claude --version failed: {:?}", e);
                    None
                }
                _ => None,
            };

            // 3. Check --help for --output-format json (cached per version)
            let help_cache_entry = get_help_output(&path, version.as_deref()).await;
            let supports_json = help_cache_entry
                .as_deref()
                .map(|help| help.contains("--output-format") && help.contains("json"))
                .unwrap_or(false);

            // 4. Build unusable_flags if json not supported
            let mut unusable_flags = Vec::new();
            if !supports_json {
                unusable_flags.push("--output-format json".into());
            }

            Ok(HealthStatus {
                available: true,
                version,
                mode: Some("cli".into()),
                unusable_flags,
                ..HealthStatus::new_available()
            })
        }
        _ => Err(BackendError::Config { message: "not CLI mode".into() }),
    }
}
```

**Version-to-help-output memoization**:

```rust
/// Per-version cache for claude --help output so we don't re-parse on every warmup.
use std::sync::OnceLock;
use std::collections::HashMap;

static HELP_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

async fn get_help_output(path: &Path, version: Option<&str>) -> Option<String> {
    let cache = HELP_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = version {
        // SAFETY: std::sync::Mutex is safe here because the lock is held for a very
        // short, non-async operation (HashMap lookup). Do NOT hold this lock across
        // an `.await` boundary — that would deadlock the tokio runtime.
        if let Some(cached) = cache.lock().unwrap().get(v) {
            return Some(cached.clone());
        }
    }
    // Run claude --help
    let output = Command::new(path)
        .arg("--help")
        .timeout(Duration::from_secs(2))
        .output()
        .await
        .ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if let Some(v) = version {
        cache.lock().unwrap().insert(v.to_string(), text.clone());
    }
    Some(text)
}
```

**Cache keying** — The `name()` method returns `"claude"` for both variants. To independently cache both, the warmup loop (`src/backend/mod.rs`) keys on `backend.name()`. For dual-mode support, `ClaudeBackend::name()` returns `"claude"` still (same backend), but the health check results are stored with a key that includes mode. The simplest approach: add a `cache_key()` method or qualify the name in `health_check` results.

Alternative: Change `ClaudeBackend::name()` to `"claude::api"` / `"claude::cli"` when the backend is in a specific mode. This would break `is_backend_available("claude")` lookups. Instead, the warmup loop or the cache entry key should be mode-aware.

**Simplest solution**: The `Backend` trait's `name()` stays `"claude"`. The `health_check` override stores results under a mode-qualified key. Since `HEALTH_CACHE` is `OnceLock<RwLock<HashMap<String, HealthStatus>>>`, we can store entries for both `"claude::api"` and `"claude::cli"`. The `warmup_backends` loop calls `backend.health_check()` which returns a `HealthStatus` with a `mode` field. The cache entry is stored under a mode-qualified key derived from `backend.name() + "::" + mode`. But that requires the warmup loop to be mode-aware.

Actually, looking more carefully at the warmup code — `warmup_backends` creates ONE backend per config entry (one `ClaudeBackend`). Since `ClaudeBackend` can only be in one mode at a time (either Api OR Cli, determined by whether `config.command` is set), there's only one cache entry per ClaudeBackend instance. The `mode` field on the HealthStatus is what the doctor renderer uses to disambiguate. **No cache key change needed** — the cache stores the result with `mode` populated.

### API/Interface Design

| Function/Method | Parameters | Returns | Description |
|---|---|---|---|
| `probe_api()` | `&self` | `Result<HealthStatus, BackendError>` | Offline API probe: check key + model |
| `probe_cli()` | `&self` | `Result<HealthStatus, BackendError>` | CLI probe: version + flag support |
| `get_help_output()` | `path, version` | `Option<String>` | Memoized `claude --help` output |
| `parse_semver_line()` | `line: &str` | `Option<String>` | Parse "Claude CLI 0.42.0" → "0.42.0" |

---

## Implementation Plan

### ST1: Add `mode` and `diagnostic` fields to `HealthStatus`

- **File**: `src/backend/context.rs`
- Add `pub mode: Option<String>` and `pub diagnostic: Option<String>` fields
- `new_available()` / `new_unavailable()` set both to `None`
- Serialization tests round-trip with `mode` and `diagnostic` set

### ST2: Implement `probe_api()` and `probe_cli()`

- **File**: `src/backend/claude.rs`
- Replace `health_check()` override with dispatch on `&self.mode`
- Implement `probe_api()` — check `api_key` non-empty, `model` non-empty
- Implement `probe_cli()` — `which::which`, `claude --version`, `claude --help`, memoize per version
- Add `parse_semver_line()` helper
- Add `get_help_output()` with `OnceLock<Mutex<HashMap<String, String>>>`

### ST3: Help-output version memoization

- **File**: `src/backend/claude.rs`
- Thread-safe `OnceLock` + `Mutex<HashMap<version_string, help_output>>`
- Cached per version string — if version parsing fails, do not cache (re-run on next warmup)

### ST4: Unit tests

- **File**: `src/backend/claude.rs` (tests module)
- 6 test cases:
  1. Api with key — available: true, mode: "api"
  2. Api without key — available: false, mode: "api"
  3. Api with empty model — available: false, mode: "api"
  4. Cli present + json supported — available: true, mode: "cli", version parsed, no unusable flags
  5. Cli present + json missing — available: true, mode: "cli", unusable_flags includes --output-format json
  6. Cli absent — available: false, mode: "cli"

### ST5: `cargo test` + `cargo clippy`

- Ensure all tests pass with `-D warnings`

---

## Constraints

**Must**:
- API probe must NOT make any network call (offline only)
- CLI probe must use a 2s timeout for both `--version` and `--help`
- `HealthStatus.mode` must be `Some("api")` or `Some("cli")` exactly when the backend is in that mode
- `HealthStatus.diagnostic` should be populated with a human-readable failure reason when `available: false`
- Cache the `--help` output per-version to avoid re-parsing on every warmup
- Use existing `BackendError` variants (Unavailable, Config) — no new error types
- Log probe timeouts at `warn` level, version parse failures at `debug` level

**Must-not**:
- Must not block the tokio runtime with sync `std::process::Command` — use `tokio::process::Command`
- Must not break existing `ClaudeBackend` consumers (query_api, query_cli, api_details)
- Must not change `Backend` trait signature

**Prefer**:
- Prefer building `HealthStatus` with struct literal syntax over mutating `new_available()`
- Prefer `which::which` over manual PATH scanning for binary existence check
- Prefer regex to find `X.Y.Z` anywhere in `claude --version` output rather than fixed-position parsing (output format may change)

**Escalate when**:
- If `HealthStatus` field addition breaks serialization of existing cached entries (add serde default)
- If `ClaudeBackend::name()` needs to change for cache disambiguation (not expected)

---

## Acceptance Criteria

- [ ] `cargo test` passes with all existing + 6 new test cases
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `HealthStatus.mode` round-trips through serde JSON serialization
- [ ] Api probe: `ANTHROPIC_API_KEY` unset → available: false, mode: "api"
- [ ] Api probe: model empty → available: false, mode: "api"
- [ ] Api probe: both set → available: true, mode: "api", version: None
- [ ] Cli probe: `claude` not on PATH → available: false, mode: "cli"
- [ ] Cli probe: `claude --version` succeeds → version parsed as semver string
- [ ] Cli probe: `claude --help` contains `--output-format json` → no unusable flags
- [ ] Cli probe: `claude --help` lacks `--output-format json` → unusable_flags populated

**Verification method**: `cargo test claude && cargo clippy -- -D warnings`

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | Api with valid env | `available: true, mode: Some("api"), version: None` | Set `ANTHROPIC_API_KEY=sk-test`, run `cargo test claude::probe_api` |
| 2 | Api without key | `available: false, mode: Some("api")` | Unset env, run test |
| 3 | Api with empty model | `available: false, mode: Some("api")` | Config with model="" |
| 4 | Cli found + json flag | `available: true, mode: Some("cli"), version: semver` | Mock `claude` binary with --version and --help that includes --output-format |
| 5 | Cli found + no json flag | `available: true, unusable_flags: ["--output-format json"]` | Mock `claude` binary without --output-format in --help |
| 6 | Cli not found | `available: false, mode: Some("cli")` | Ensure `claude` not on PATH |

**Edge cases to cover**:
- `claude --version` returns non-semver (e.g., "Claude CLI (claude 3.5)") → use regex to find `X.Y.Z` anywhere in output; graceful fallback to None
- `claude --help` times out (>2s) → treat as unsupported, log at warn level
- `claude --help` output format changes → --output-format detection is best-effort; failure must not block availability
- `ANTHROPIC_API_KEY` is set but blank or whitespace → trim before check
- `model` configured to empty string → same as unconfigured
- `claude` binary updated in-place (same path, new version) → help cache keyed by version string handles this correctly

---

## Testing Strategy

- **Unit Tests**: 6 new tests in `src/backend/claude.rs` covering all API and CLI probe scenarios
- **Integration Tests**: Existing `test_health_check_default_returns_ok_when_unavailable` pattern (already passing)
- **Manual Testing**: Run `cargo run -- doctor` with both modes configured and verify output (requires CLO-393)

---

## Open Questions

- [ ] Should the per-version help cache use `HashMap<String, String>` or `LruCache`? (HashMap is fine — version count is O(1))

---

## References

- [CLO-391 Linear](https://linear.app/cloud-ai/issue/CLO-391/fr-13a-claude-dual-mode-health-probe-api-vs-cli)
- [PRD v5 §4 FR-13a](docs/prds/prd-phase-2-predictable-cli-execution-v5.md)
- [Discovery Report](docs/discovery/clo-391.md)
- [CLO-388 Warmup + HealthCache](docs/plans/clo-388-warmup-backends.md)
- [CLO-389 Ollama Probe](docs/plans/clo-389-ollama-probe.md)
- [CLO-393 Doctor renderer](https://linear.app/cloud-ai/issue/CLO-393/fr-14-lok-doctor-healthstatus-renderer-table--json)
