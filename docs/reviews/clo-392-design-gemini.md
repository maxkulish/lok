# Design Review: CLO-392

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-05-24
**Pipeline**: lok design-review (manual fallback)

---

# Design Review: CLO-392 (Codex Health Probe)

**Verdict:** APPROVE_WITH_SUGGESTIONS

The design is well-structured, additive, and correctly identifies the need for proactive version detection. The strategy of leveraging a `const` matrix and keeping the `Backend` trait interface unchanged is solid. The test plan provides comprehensive coverage of version bounds and timeout scenarios.

Below are key findings and a prioritized list of actionable items based on the review criteria.

### 1. Missing Exit Status Check (Blind Spot / Operational Readiness)
In the proposed implementation snippet, `Command::new(...).output()` is awaited and then `stdout` is parsed immediately. If the command runs but fails (e.g., a node wrapper script fails, or the binary is not actually codex and exits with an error), `output()` will still return `Ok(Output)` but with a non-zero exit status, potentially containing error text or an empty string in `stdout`.
* **Actionable Item:** Add an explicit check for `output.status.success()`. If it returns `false`, gracefully fail the probe and return `BackendError::Unavailable` (or log the stderr for debugging) before trying to parse the version.

### 2. Resolution to Open Question 1 (Architecture Quality)
The document asks whether the warning should be emitted inside `CodexBackend::health_check` or in `workflow.rs` (the engine).
* **Actionable Item:** It **must** be implemented in `workflow.rs` (or the engine's validation phase). `health_check` evaluates the backend's inherent capabilities and should remain completely decoupled from specific workflow configurations. Passing workflow step configurations into the backend to check flags would create a leaky abstraction. The backend provides the `unusable_flags` state; the engine reconciles it against the steps.

### 3. Version Parsing Robustness (Code Quality)
The assumption that `codex --version` outputs clean text like `codex-cli 0.119.0` is generally safe, but Node-based CLIs or wrapper scripts frequently emit warning logs or deprecation notices to `stdout` before the actual version string. 
* **Actionable Item:** Ensure `parse_version` is designed to scan the entire output for the first substring matching the `\d+\.\d+\.\d+` pattern, rather than strictly anchoring the parse logic to the beginning or end of the string.

### 4. Synchronous IO in Async Context (Concurrency Safety)
The design uses `which::which(&self.command)` inside the `async fn health_check`. `which` performs synchronous filesystem operations (checking paths and file modes). While this is usually fast, on network file systems or highly concurrent environments, blocking the Tokio executor thread is an anti-pattern.
* **Actionable Item:** Wrap the `which` call in `tokio::task::spawn_blocking`, or explicitly document that this known synchronous blocking is acceptable here because it only occurs once during the warmup phase. (Given it was existing behavior, it is acceptable, but should be addressed for strict concurrency safety).

### 5. Timeouts and Subprocess Reaping (Operational Readiness)
The 2-second timeout using `tokio::time::timeout` is correct. If the timeout is reached, Tokio drops the `Child` future, which kills the process.
* **Actionable Item:** This is the correct and safe behavior for async commands, but verify that you are strictly importing `tokio::process::Command` (as stated in the architecture diagram) and not accidentally using `std::process::Command` in the implementation snippet, as `std::process::Command::output` would block the thread regardless of the `tokio::time::timeout` wrapper.
