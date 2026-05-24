# Lessons: CLO-391 — Claude dual-mode health probe (Api vs Cli)

Durable rules extracted from the CLO-391 implementation, validation gate, and review cycle.

---

## L1 — `std::sync::Mutex` held across `.await` triggers `await_holding_lock`

**Source incident:** `cargo clippy --all-targets -- -D warnings` failed in pre-PR validation on `test_probe_cli_present_with_json_support` and `test_probe_cli_present_without_json_support`. Both held a `std::sync::Mutex` guard across an `await` on `probe_cli()`, triggering the `await_holding_lock` lint. The design doc did not mention this pattern.

**Rule:** In `tokio` async tests that need shared mutable state (e.g. `PATH` manipulation), prefer `tokio::sync::Mutex` over `std::sync::Mutex`. The guard can be held across `.await` points without clippy warnings and without risking runtime panics.

**How to apply:**
- If the test is `#[tokio::test]` and accesses a shared `Mutex`, use `tokio::sync::Mutex`.
- If `std::sync::Mutex` is used intentionally (e.g. the lock is released before `.await`), add an explicit `drop(guard)` and a comment explaining why `tokio::sync::Mutex` is unsuitable.

---

## L2 — Every `tokio::process::Command` under `tokio::time::timeout` needs `.kill_on_drop(true)`

**Source incident:** Codex flagged that `probe_cli` spawned `Command` for `claude --version` and `get_help_output` spawned another for `claude --help` without `.kill_on_drop(true)`. A hung child process would outlive the 2s timeout and leak. The sibling `query_cli` at `:236` already had the flag, making the probes inconsistent within the same file.

**Rule:** Every `tokio::process::Command` that is spawned inside a `timeout(...)` future must set `.kill_on_drop(true)`. Otherwise the process keeps running after the timeout future resolves, leaking resources and potentially producing late stdout that confuses later cache lookups.

**How to apply:**
- Run `grep -n "Command::new" src/backend/*.rs` and verify every spawn has `.kill_on_drop(true)`.
- If a command intentionally outlives the timeout (e.g. long-running background process), that must be a `spawn()` with its own handle management, not a `timeout()`.

---

## L3 — Logging-level guidance in the design doc must match the crate's actual dependencies

**Source incident:** The design document (approved by Gemini and Ollama) prescribed `"Log probe timeouts at warn level, version parse failures at debug level."` using `tracing::warn!` and `tracing::debug!`. The project has neither `tracing` nor `log` in `Cargo.toml`; the codebase uses `eprintln!` for diagnostics. The pre-PR synthesis flagged this as a MUST FIX, but the correct resolution was to keep `eprintln!` for consistency rather than add new dependencies.

**Rule:** Design documents must not reference logging macros or telemetry crates that are not already in `Cargo.toml`. If the design requires richer logging, the dependency change must be scoped as a separate ticket or the logging guidance must be reworded to match existing conventions.

**How to apply:**
- Before writing logging guidance in a design doc, run `cargo tree -p tracing` and `cargo tree -p log` to confirm availability.
- If neither crate is present, default to the project's existing convention (e.g., `eprintln!`, `println!`, or conditional compilation with `#[cfg(debug_assertions)]`).
- If richer observability is genuinely needed, open a dependency-addition ticket instead of smuggling it into an unrelated feature PR.
