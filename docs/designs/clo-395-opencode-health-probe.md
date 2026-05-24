# Design: CLO-395 — FR-12b: opencode health probe + Google auth detection

## Problem

`GeminiBackend::health_check` (line 640 of `src/backend/gemini.rs`) is a dead stub: it returns `HealthStatus::new_available()` if `which::which("opencode")` succeeds, otherwise `BackendError::Unavailable`. It populates none of the HealthStatus fields that other backends (Claude, Codex, Ollama) fill: `version`, `mode`, `auth_method`, `models`, `diagnostic`. Users whose opencode is on PATH but hasn't completed `opencode auth login` see the backend as "available" and hit opaque failures during actual queries. The `lok doctor` command (FR-14, CLO-393) renders a useless row for gemini: "available: true" with no version, no auth mode, no model list.

This is the last backend without a real health probe. Claude (CLO-391) has a dual API/CLI probe with version detection. Codex (CLO-392) has version parsing and flag matrix validation. Ollama (CLO-389) has HTTP API probing with model enumeration. The gemini backend needs equivalent coverage now that CLO-394 completed the opencode migration.

## Goals / Non-goals

**Goals**

- Replace the `which`-only stub with a multi-step probe that detects opencode version, Google auth method, and available Gemini models.
- Populate `HealthStatus.version` from `opencode --version` output.
- Populate `HealthStatus.mode` with `"oauth" | "api-key" | "none"` per FR-12b acceptance criteria.
- Populate `HealthStatus.auth_method` with the detected method.
- When no auth is detected, set `available: false` and populate `diagnostic` with a note prompting `opencode auth login` (not `opencode login`).
- Read auth.json directly from `~/.local/share/opencode/auth.json` as the primary auth detection path (matches acceptance criteria note: "If `opencode auth list` is slow, prefer reading the auth.json file directly").
- Check `GEMINI_API_KEY` / `GOOGLE_API_KEY` environment variables as secondary auth detection (opencode honors these as fallback).
- Optionally enumerate Gemini models via `opencode models google` (best-effort, with timeout).
- Honor a 1s timeout budget for `--version` (acceptance criteria).
- Write unit tests covering: binary missing from PATH, version timeout, google oauth present, api-key fallback, no auth, model enumeration.

**Non-goals**

- No changes to the `Backend` trait, `HealthStatus` struct, `HealthCache`, or `warmup_backends` pipeline — all are complete from prior tasks.
- No changes to `src/main.rs` doctor checks — already updated for opencode in CLO-394.
- No changes to `GeminiBackend::query` or `build_argv` — the probe is independent of query logic.
- No opencode daemon, serve, or HTTP health endpoint (future probe option noted in Linear issue).
- No multi-mode keying (unlike Claude with "api" | "cli", gemini is single-mode).

## Architecture

All changes are localized to one file; no new modules are introduced.

```
                   +----------------------------+
                   | GeminiBackend::health_check |   src/backend/gemini.rs
                   +------------+---------------+
                                |
                   ┌────────────┼────────────┐
                   v            v            v
              probe_version  detect_auth  probe_models
              (--version)    (file/env)   (models google)
                    |            |            |
                    v            v            v
              +-----------------------------------+
              |         HealthStatus              |
              |  version: "1.15.10"               |
              |  mode: "api-key"                  |
              |  auth_method: "api-key"           |
              |  models: [ModelInfo...]           |
              |  diagnostic: None                 |
              |  available: true                  |
              +-----------------------------------+
```

**Data flow**

1. `health_check()` is called during `warmup_backends` or on-demand.
2. Step 1 — PATH check: `which::which(self.command)` — immediate, no tokio timeout needed. On failure, return `BackendError::Unavailable`.
3. Step 2 — Version: `tokio::time::timeout(Duration::from_secs(1), Command::new(&cmd).arg("--version").output())`. On success, parse the first line of stdout (e.g., `"1.15.10\n"`). On timeout, mark `available: false` with diagnostic. On non-zero exit, mark `available: false` with diagnostic.
4. Step 3 — Auth detection (priority order):
   a. **Auth file**: Read `~/.local/share/opencode/auth.json`. If the JSON contains a `"google"` key, extract the auth type:
      - If `"type": "oauth"` → `mode: "oauth"`, `auth_method: "oauth"`
      - If `"type": "api"` → `mode: "api-key"`, `auth_method: "api-key"` (opencode stores API keys as credentials)
      - Otherwise → continue to next check
   b. **Environment**: Check `std::env::var("GEMINI_API_KEY")` or `std::env::var("GOOGLE_API_KEY")`. If either is present → `mode: "api-key"`, `auth_method: "api-key"`.
   c. **CLI probe**: Run `opencode auth list` with 2s timeout. Parse the output for "Google" entries:
      - If found under "Credentials" section → `mode: "oauth"`, `auth_method: "oauth"`
      - If found under "Environment" section → `mode: "api-key"`, `auth_method: "api-key"`
   d. **None**: No Google auth detected → `mode: "none"`, `available: false`, `diagnostic: Some("No Google auth detected. Run `opencode auth login` to set up credentials (not `opencode login`).")`.
5. Step 4 — Model enumeration (best-effort, 3s timeout): Run `opencode models google` with `tokio::time::timeout`. On success, parse each line as a model name and create `ModelInfo { name, modified_at: None, size: None, digest: None }`. On timeout/failure, leave `models: Vec::new()` and optionally append to `diagnostic`.

**Auth detection semantics**

The `mode` field discriminator captures the credential source:
- `"oauth"` — Google provider credentials stored via `opencode auth login` (OAuth or API key stored in opencode's credential store).
- `"api-key"` — `GEMINI_API_KEY` or `GOOGLE_API_KEY` environment variable present (opencode's fallback auth path).
- `"none"` — No auth detected; `available: false`.

This is slightly different from the acceptance criteria which says `"oauth" | "api-key" | "none"`. In practice, the auth.json file stores API keys as `"type": "api"` (not oauth), but from the user's perspective, credentials stored via `opencode auth login` are considered the "oauth" flow. We use `mode: "api-key"` only for environment variable detection.

**Types**

- No new types. `HealthStatus`, `ModelInfo`, `BackendError`, `CachedBackend` are all unchanged.
- New private helper functions in `src/backend/gemini.rs`:
  - `probe_version(command: &str) -> Result<String, BackendError>` — spawns `--version`, returns trimmed version string.
  - `detect_auth_from_file() -> Option<String>` — reads auth.json, returns `"oauth"` or `"api-key"`.
  - `detect_auth_from_env() -> Option<String>` — checks env vars, returns `"api-key"`.
  - `detect_auth_from_cli(command: &str) -> Result<Option<String>, BackendError>` — runs `auth list`, returns auth mode.
  - `probe_models(command: &str) -> Result<Vec<ModelInfo>, BackendError>` — runs `models google`, parses model names.

**Out-of-scope source paths** (intentionally untouched): `src/backend/mod.rs`, `src/backend/context.rs`, `src/config.rs`, `src/main.rs`, all other backend files.

## Design decisions

### Why auth.json first (not CLI)?
The Linear issue notes explicitly say: "If `opencode auth list` is slow, prefer reading the auth.json file directly." A single `fs::read_to_string` + `serde_json::from_str` is ~50µs vs. a subprocess spawn (~10-50ms). The auth.json path is also deterministic: opencode stores all credentials in that single file.

### Why not parallel probes?
The three probes are sequential: version → auth → models. This respects the 1s version timeout budget (we don't race 3 subprocesses), and version failure gates auth detection (if opencode isn't working, auth is moot). The total budget is worst-case ~6s, which is acceptable for warmup (runs once at engine start, parallelized across all backends).

### Why `available: false` when no auth?
The Claude API probe already marks `available: false` when no API key is configured. Consistency with that pattern, plus the diagnostic note guides the user to the fix (`opencode auth login`). Without auth, the backend cannot query Gemini models, so marking it unavailable prevents wasted warmup time and provides immediate feedback in `lok doctor`.

### Why not `#[cfg(windows)]` for auth.json path?
The project currently targets macOS/Linux (Homebrew install instruction, CI runs on macOS). Windows support can be added later if needed; the path `~/.local/share/opencode/auth.json` resolves via `dirs::home_dir()` which works on all platforms. For now, we use `home_dir()?.join(".local/share/opencode/auth.json")` which handles the platform correctly.

Actually, to avoid adding a `dirs` dependency, we use `std::env::var("HOME")` and construct the path directly. On macOS/Linux this resolves correctly.

### Model enumeration shape
`opencode models google` outputs one model name per line (plain text). We parse each non-empty line as `ModelInfo { name: line.trim().to_string(), modified_at: None, size: None, digest: None }`. The Ollama backend already populates `ModelInfo` with richer metadata from API responses; gemini models only have names from this CLI output. This is acceptable — the `models` field is informational for `lok doctor`.

## Dependencies

- **Internal**: `HealthStatus` struct (already exported), `BackendError::Unavailable`, `ModelInfo`, `Backend` trait, `tokio::process::Command`, `which` crate.
- **External**: `opencode` binary (v1.14+ for `--version`, `auth list`, `models google` subcommands).
- **No new Cargo.toml dependencies** — we reuse `serde_json` (for auth.json parsing), `tokio` (for Command + timeout), `which` (for PATH probe), all already in Cargo.toml.

## Files changed

| File | Change |
|------|--------|
| `src/backend/gemini.rs` | Replace `health_check()` stub with multi-step probe; add private probe/detect helper functions; add ~7 unit tests following Codex tempfile pattern. |

### New functions in `src/backend/gemini.rs`

```rust
/// Run `opencode --version` with a 1s timeout budget.
/// Returns the trimmed version string (e.g. "1.15.10").
async fn probe_version(command: &str) -> Result<String, BackendError>;

/// Detect Google auth from `~/.local/share/opencode/auth.json`.
/// Returns "oauth" if a google key exists (regardless of type),
/// "api-key" if google.type == "api".
fn detect_auth_from_file() -> Option<String>;

/// Detect Google auth from GEMINI_API_KEY / GOOGLE_API_KEY env vars.
/// Returns "api-key" if either is set.
fn detect_auth_from_env() -> Option<String>;

/// Detect Google auth by parsing `opencode auth list` output.
/// Returns "oauth" or "api-key" depending on which section "Google" appears in.
async fn detect_auth_from_cli(command: &str) -> Result<Option<String>, BackendError>;

/// Run `opencode models google` with a 3s timeout, returning model names.
async fn probe_models(command: &str) -> Result<Vec<ModelInfo>, BackendError>;
```

## Test plan

All tests follow the Codex tempfile shell script pattern (no actual opencode on CI).

| Test | What it validates |
|------|------------------|
| `gemini_health_check_success` | Version probe succeeds with mock script returning "1.15.10" |
| `gemini_health_check_no_auth` | No auth.json, no env vars → `mode: "none"`, `available: false`, diagnostic mentions `opencode auth login` |
| `gemini_health_check_api_key_env` | `GEMINI_API_KEY` env var set → `mode: "api-key"` |
| `gemini_health_check_auth_file_oauth` | auth.json with `"google": {"type": "oauth"}` → `mode: "oauth"` |
| `gemini_health_check_auth_file_api` | auth.json with `"google": {"type": "api"}` → `mode: "api-key"` |
| `gemini_health_check_version_timeout` | Mock script sleeps 10s → timeout error |
| `gemini_health_check_missing_binary` | Command not on PATH → `BackendError::Unavailable` |
| `gemini_health_check_models_enumerated` | `models google` mock returns model names → `models` populated |

## Risks

1. **opencode CLI output format drift**: `--version` and `models google` output format is plain text and stable in v1.14+. `auth list` output could change — mitigated by prioritizing auth.json file read.
2. **Auth.json path on Linux**: Verified `~/.local/share/opencode/auth.json` on macOS and Linux. Windows uses `%LOCALAPPDATA%/opencode/auth.json` — not in scope.
3. **Auth.json structure**: opencode stores credentials under provider keys. The `"type"` field discriminates between `"oauth"` and `"api"`. Both are valid Google auth mechanisms from the user's perspective — we treat both as `mode: "oauth"` for consistency with the acceptance criteria terminology, unless the type is `"api"` which maps to `"api-key"`.
