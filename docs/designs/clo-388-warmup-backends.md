# Design: CLO-388 - FR-9a + FR-10 + FR-15: Engine warmup + HealthCache + sync is_available cache-only

## Problem

Currently, `Backend::is_available()` is a synchronous, uncached method that is called dynamically. It often performs file-system searches or command existence checks via `which::which`, invoking syscalls on every step and query. Furthermore, this synchronous interface makes it impossible to perform real server connectivity checks (e.g., pinging Ollama's local port) without introducing blocking wait-times or spawning heavy blocking calls in the synchronous runtime.

Additionally:
- `OllamaBackend::is_available()` unconditionally returns `true` (a lie).
- Downstream steps run the risk of attempting to use unavailable or misconfigured backends, leading to late-stage runtime failures instead of early-stage diagnostic rejections.

## Goals / Non-goals

Goals:
- Introduce `HealthStatus` and `ModelInfo` structs with appropriate fields for future per-backend probes.
- Implement `Engine::warmup_backends()` to probe all enabled backends in parallel asynchronously via `futures::future::join_all` before execution begins.
- Implement a thread-safe `HealthCache` using standard library `OnceLock<RwLock<HashMap<String, HealthStatus>>>`.
- Convert `is_available()` on all 5 backends (Ollama, Gemini, Codex, Claude, Bedrock) to be **cache-only**, performing memory lookup rather than system/process syscalls.
- Override `health_check()` on each backend to run its real/original checks asynchronously.
- Ensure `lok run`, `lok ask`, and `lok doctor` trigger `warmup_backends()` at their entrypoints.
- Maintain seamless test-suite compatibility and provide test-helpers to clear or mock cache entries.

Non-goals:
- Implementing the detailed per-backend network probes or local model-list extraction (FR-11/11a/12/13/13a). These will be implemented in subsequent tickets and will build cleanly on top of the caching and warmup infrastructure introduced here.
- Introducing external caching daemon or persistence across different `lok` invocations.

## Architecture

All changes are scoped to the existing backend and template/CLI orchestration files. No new library dependencies are required.

Data flow:

```
[CLI Entry (lok run, ask, doctor)]
         │
         ▼
[Engine::warmup_backends()]
         │ (fans out in parallel)
         ├───────────────────────┬───────────────────────┐
         ▼                       ▼                       ▼
 [Codex::health_check]  [Gemini::health_check]  [Ollama::health_check]  ...
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 ▼
                     [Populate Shared HealthCache]
                                 │
                                 ▼
                   [Runtime Exec / get_backends()]
                                 │
                                 ▼
                    [Backend::is_available()]
                    (pure memory cache lookup)
```

Touched locations:

| File | Change |
|------|--------|
| `src/backend/context.rs` | Update `HealthStatus` struct and define `ModelInfo`. |
| `src/backend/mod.rs` | Implement `HEALTH_CACHE` static, `Engine::warmup_backends()`, and test helpers. |
| `src/backend/ollama.rs` | Update `is_available()` to cache-only; override `health_check()`. |
| `src/backend/gemini.rs` | Update `is_available()` to cache-only; override `health_check()`. |
| `src/backend/codex.rs` | Update `is_available()` to cache-only; override `health_check()`. |
| `src/backend/claude.rs` | Update `is_available()` to cache-only; override `health_check()`. |
| `src/backend/bedrock.rs` | Update `is_available()` to cache-only; override `health_check()`. |
| `src/main.rs` | Wire `Engine::warmup_backends()` to `Ask`, `Doctor`, and `run_workflow` entrypoints. |

## Public API Surface

### `src/backend/context.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthStatus {
    pub available: bool,
    pub version: Option<String>,
    pub auth_method: Option<String>,
    pub capabilities: Option<serde_json::Value>,
    pub unusable_flags: Vec<String>,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelInfo {
    pub name: String,
    pub modified_at: Option<String>,
    pub size: Option<u64>,
    pub digest: Option<String>,
}

impl HealthStatus {
    pub fn new_available() -> Self {
        Self {
            available: true,
            version: None,
            auth_method: None,
            capabilities: None,
            unusable_flags: Vec::new(),
            models: Vec::new(),
        }
    }

    pub fn new_unavailable() -> Self {
        Self {
            available: false,
            version: None,
            auth_method: None,
            capabilities: None,
            unusable_flags: Vec::new(),
            models: Vec::new(),
        }
    }
}
```

### `src/backend/mod.rs`

```rust
use std::sync::{OnceLock, RwLock};
use std::collections::HashMap;

pub static HEALTH_CACHE: OnceLock<RwLock<HashMap<String, HealthStatus>>> = OnceLock::new();

pub fn get_health_cache() -> &'static RwLock<HashMap<String, HealthStatus>> {
    HEALTH_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub struct Engine;

impl Engine {
    pub async fn warmup_backends(config: &Config) -> Result<()> {
        let mut futures = Vec::new();

        for (name, backend_config) in &config.backends {
            if !backend_config.enabled {
                continue;
            }

            let retry_policy = get_retry_policy(backend_config, &config.defaults);
            match create_backend(name, backend_config, retry_policy) {
                Ok(backend) => {
                    futures.push(async move {
                        (backend.name().to_string(), backend.health_check().await)
                    });
                }
                Err(e) => {
                    eprintln!("{} Failed to construct backend {}: {}", "warning:".yellow(), name, e);
                }
            }
        }

        if futures.is_empty() {
            return Ok(());
        }

        let results = futures::future::join_all(futures).await;

        let cache = get_health_cache();
        if let Ok(mut lock) = cache.write() {
            for (name, res) in results {
                match res {
                    Ok(status) => {
                        lock.insert(name, status);
                    }
                    Err(e) => {
                        eprintln!("{} Health check failed for backend {}: {}", "warning:".yellow(), name, e);
                        lock.insert(name, HealthStatus::new_unavailable());
                    }
                }
            }
        }

        Ok(())
    }

    pub fn is_backend_available(name: &str) -> bool {
        let cache = get_health_cache();
        if let Ok(lock) = cache.read() {
            lock.get(name).map(|s| s.available).unwrap_or(false)
        } else {
            false
        }
    }
}
```

### Backend `is_available` and `health_check` implementations

Each backend will carry an identical, lightweight memory lookup for `is_available()`:
```rust
fn is_available(&self) -> bool {
    super::Engine::is_backend_available(self.name())
}
```

And each backend will override `health_check()` to carry its live, async check:
* **Gemini**:
  ```rust
  async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
      if which::which(&self.command).is_ok() {
          Ok(HealthStatus::new_available())
      } else {
          Err(BackendError::Unavailable {
              message: format!("Gemini command {} not found", self.command),
          })
      }
  }
  ```
* **Codex**:
  ```rust
  async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
      if which::which(&self.command).is_ok() {
          Ok(HealthStatus::new_available())
      } else {
          Err(BackendError::Unavailable {
              message: format!("Codex command {} not found", self.command),
          })
      }
  }
  ```
* **Claude**:
  ```rust
  async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
      let available = match &self.mode {
          ClaudeMode::Api { api_key, .. } => !api_key.expose_secret().is_empty(),
          ClaudeMode::Cli { command, .. } => which::which(command).is_ok(),
      };
      if available {
          Ok(HealthStatus::new_available())
      } else {
          Err(BackendError::Unavailable {
              message: format!("Claude backend is not available"),
          })
      }
  }
  ```
* **Ollama**:
  ```rust
  async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
      Ok(HealthStatus::new_available())
  }
  ```
* **Bedrock**:
  ```rust
  async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
      let available = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
          || std::env::var("AWS_PROFILE").is_ok()
          || std::path::Path::new(&format!(
              "{}/.aws/credentials",
              std::env::var("HOME").unwrap_or_default()
          ))
          .exists();
      if available {
          Ok(HealthStatus::new_available())
      } else {
          Err(BackendError::Unavailable {
              message: format!("Bedrock backend is not available"),
          })
      }
  }
  ```

---

## Test Plan

### Test Helpers and Mock Setup
We will add `clear_health_cache()` and `set_mock_health(backend_name, status)` as test-helpers inside `src/backend/mod.rs` so that tests can isolate their execution without running the actual warmup pipeline.

### Unit Tests
1. **Cache Read/Write Integrity**: Test that `set_mock_health` properly updates availability, and `is_backend_available` matches the cached value.
2. **Warmup Pipeline Execution**: Run `Engine::warmup_backends` against a mocked config and assert the cache is populated with parallel execution.
3. **Sycall Prevention Assertion**: A custom unit test inside `src/backend/mod.rs` that wires up a Mock Backend whose system-probing logic causes a panic if accessed via `is_available()`, proving that `is_available` is completely cache-only and makes zero system calls.

### Manual Verification
1. `cargo test` to verify zero regression across existing suites (which will use pre-seeded/mocked cached states in test setup, or whose test configs can trigger warmup).
2. `cargo clippy` and `cargo fmt`.
