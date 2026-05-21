# Plan: CLO-388 — FR-9a + FR-10 + FR-15: Engine warmup + HealthCache + sync is_available cache-only

## Context
- Design: [docs/designs/clo-388-warmup-backends.md](../designs/clo-388-warmup-backends.md)
- Discovery: [docs/discovery/clo-388.md](../discovery/clo-388.md)
- Linear: https://linear.app/cloud-ai/issue/CLO-388/

---

## Sub-tasks

### ST1: Update HealthStatus and define ModelInfo in src/backend/context.rs

**Files:** `src/backend/context.rs`

**Changes:**
- Replace the placeholder `pub struct HealthStatus;` with the full `HealthStatus` struct and `ModelInfo` struct, deriving `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`, `Eq`.
- Implement constructor helpers `new_available()` and `new_unavailable()` on `HealthStatus`.

**Acceptance:** `cargo build` compiles without errors.

**Estimate:** S

---

### ST2: Implement HEALTH_CACHE, Engine and test helpers in src/backend/mod.rs

**Files:** `src/backend/mod.rs`

**Changes:**
- Add `pub static HEALTH_CACHE` as a `std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<String, HealthStatus>>>`.
- Add `get_health_cache() -> &'static std::sync::RwLock<std::collections::HashMap<String, HealthStatus>>`.
- Implement test helpers `clear_health_cache()` and `set_mock_health(backend_name, status)`.
- Define `pub struct Engine;` with:
  - `pub async fn warmup_backends(config: &crate::config::Config) -> Result<()>` fanning out via `futures::future::join_all`.
  - `pub fn is_backend_available(name: &str) -> bool`.
- Re-export `ModelInfo` and `Engine` from the `backend` module.

**Acceptance:** `cargo build` compiles without errors.

**Estimate:** M

---

### ST3: Migrate all 5 backends to cache-only is_available() and override health_check()

**Files:**
- `src/backend/ollama.rs`
- `src/backend/gemini.rs`
- `src/backend/codex.rs`
- `src/backend/claude.rs`
- `src/backend/bedrock.rs`

**Changes:**
- For each backend:
  - Change `is_available(&self) -> bool` to return `super::Engine::is_backend_available(self.name())` (or `crate::backend::Engine::is_backend_available(self.name())`).
  - Implement/override `async fn health_check(&self) -> Result<HealthStatus, BackendError>` executing their real active checks (which were previously in `is_available()`).

**Acceptance:** `cargo build` compiles without errors.

**Estimate:** M

---

### ST4: Wire Engine::warmup_backends() into CLI entry points

**Files:** `src/main.rs`

**Changes:**
- Inside `Commands::Ask`: Call `backend::Engine::warmup_backends(&config).await?;` before retrieving backends.
- Inside `Commands::Doctor`: Call `let _ = backend::Engine::warmup_backends(&config).await;` at the beginning of doctor's run.
- Inside `run_workflow`: Call `backend::Engine::warmup_backends(config).await?;` at workflow execution entry.

**Acceptance:** `cargo build` compiles cleanly, and running `cargo run -- ask "test"` works (does not block on empty cache if warmup has populated it).

**Estimate:** S

---

### ST5: Add Unit Tests in src/backend/mod.rs

**Files:** `src/backend/mod.rs` (test module)

**Changes:**
- Add tests verifying cache read/write, `is_backend_available()` defaults to `false` on empty cache.
- Add an assertion test with a mock backend that panics on file/syscall access if `is_available()` is called, asserting that `is_available()` performs no syscalls/filesystem pings.
- Add a warmup-pipeline integration test ensuring multiple mock backends can be warmed up in parallel and that they correctly populate the shared cache.
- Update any existing tests to handle cache pre-population if they directly call `is_available()`.

**Acceptance:** `cargo test` passes.

**Estimate:** M

---

### ST6: Verification & Pre-merge gate

**Files:** All changed files.

**Changes:**
- Verify all requirements:
  - Format checks.
  - Clippy warnings.
  - Complete test suite.

**Acceptance:**
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

**Estimate:** S
