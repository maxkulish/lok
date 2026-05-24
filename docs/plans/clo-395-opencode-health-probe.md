# Plan: CLO-395 — FR-12b: opencode health probe + Google auth detection

## Summary

Replace the `which`-only stub in `GeminiBackend::health_check` with a multi-step probe. Single file change: `src/backend/gemini.rs`.

## Implementation steps

### Step 1: Add probe helper functions

Add 5 private async/sync helper functions to `impl GeminiBackend`:

1. `async fn probe_version(command: &str) -> Result<String, BackendError>`
   - Run `command --version` with `tokio::time::timeout(1s)`
   - On timeout → `BackendError::Unavailable { message: "opencode --version timed out after 1s" }`
   - Parse first line of stdout, trim → return version string
   - On spawn failure → `BackendError::Unavailable`

2. `fn detect_auth_from_file() -> Option<String>`
   - Resolve `$HOME/.local/share/opencode/auth.json`
   - Read and parse as JSON
   - If `"google"` key exists:
     - If `google.type == "oauth"` → return `Some("oauth".into())`
     - If `google.type == "api"` → return `Some("api-key".into())`
     - Otherwise → return `Some("oauth".into())` (default to oauth for google key presence)
   - Return `None` on any error (file missing, invalid JSON, no google key)

3. `fn detect_auth_from_env() -> Option<String>`
   - Check `std::env::var("GEMINI_API_KEY")` or `std::env::var("GOOGLE_API_KEY")`
   - If either is `Ok` and non-empty → return `Some("api-key".into())`
   - Return `None`

4. `async fn detect_auth_from_cli(command: &str) -> Result<Option<String>, BackendError>`
   - Run `command auth list` with `tokio::time::timeout(2s)`
   - On timeout → return `Ok(None)`
   - Parse output lines for "Google":
     - If found under "Credentials" section → `"oauth"`
     - If found under "Environment" section → `"api-key"`
   - Return `Ok(None)` if no Google entry found

5. `async fn probe_models(command: &str) -> Result<Vec<ModelInfo>, BackendError>`
   - Run `command models google` with `tokio::time::timeout(3s)`
   - On timeout → return `Ok(vec![])`
   - Parse each non-empty line as a model name
   - Return `Vec<ModelInfo>` with names populated

### Step 2: Rewrite `health_check()`

Replace the current stub (lines 640-653) with:

```rust
async fn health_check(&self) -> std::result::Result<super::HealthStatus, super::BackendError> {
    // 1. PATH check
    let cmd = which::which(&self.command).map_err(|_| super::BackendError::Unavailable {
        message: format!("Gemini backend command '{}' not found on PATH", self.command),
    })?;
    let cmd_str = cmd.to_string_lossy().to_string();

    // 2. Version probe (1s timeout)
    let version = match Self::probe_version(&cmd_str).await {
        Ok(v) => Some(v),
        Err(e) => {
            return Ok(super::HealthStatus {
                available: false,
                diagnostic: Some(format!("Version probe failed: {}", e)),
                ..super::HealthStatus::new_unavailable()
            });
        }
    };

    // 3. Auth detection (priority: file → env → CLI)
    let (auth_method, diagnostic) =
        if let Some(method) = Self::detect_auth_from_file() {
            (Some(method), None)
        } else if let Some(method) = Self::detect_auth_from_env() {
            (Some(method), None)
        } else {
            match Self::detect_auth_from_cli(&cmd_str).await {
                Ok(Some(method)) => (Some(method), None),
                Ok(None) | Err(_) => (
                    Some("none".to_string()),
                    Some("No Google auth detected. Run `opencode auth login` to set up credentials (not `opencode login`).".to_string()),
                ),
            }
        };

    let available = auth_method.as_deref() != Some("none");

    // 4. Model enumeration (best-effort, 3s timeout)
    let models = Self::probe_models(&cmd_str).await.unwrap_or_default();

    Ok(super::HealthStatus {
        available,
        version,
        mode: auth_method.clone(),
        auth_method,
        diagnostic,
        models,
        ..super::HealthStatus::new_available()
    })
}
```

### Step 3: Add unit tests

Add ~8 tests to the `#[cfg(test)] mod tests` block (after the existing tests, around line 985):

Following the Codex tempfile shell script pattern:

1. `test_gemini_health_check_success` — mock script returns version "1.15.10" + env var set
2. `test_gemini_health_check_no_auth` — no env vars, no auth.json → `available: false`, `mode: "none"`, diagnostic contains `opencode auth login`
3. `test_gemini_health_check_api_key_env` — `GEMINI_API_KEY` set → `mode: "api-key"`
4. `test_gemini_health_check_missing_binary` — command not on PATH → `Unavailable`
5. `test_gemini_health_check_version_timeout` — script sleeps 10s → `available: false`

For auth.json tests, create tempdir and write auth.json files:

6. `test_gemini_health_check_auth_file_oauth` — auth.json with google oauth
7. `test_gemini_health_check_auth_file_api` — auth.json with google api key

For models test:

8. `test_gemini_health_check_models_enumerated` — mock models script returns model names

### Step 4: Verify

- `cargo test` — all existing + new tests pass
- `cargo clippy -- -D warnings` — no warnings
- `cargo build` — compiles cleanly

## Files changed

| File | Changes |
|------|---------|
| `src/backend/gemini.rs` | Add 5 probe helper functions; replace `health_check()`; add 8 tests |

## Estimated effort

S — ~100 lines of new code, all localized to one file. Follows well-established patterns from Codex/Ollama/Claude probes.
