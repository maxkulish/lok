# Pre-PR validation: clo-406

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-26
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

MEDIUM: Missing required startup TTL log.
The design requires one `info: Health cache TTL: <duration>` line at startup, but `health_cache_ttl()` only resolves the `OnceLock` and never logs the effective value. It is called lazily from freshness reads/warmup, not from an eager startup/logging path. See [src/backend/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-406-health/src/backend/mod.rs:496) and [src/backend/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-406-health/src/backend/mod.rs:511).

MEDIUM: TTL env tests do not actually acquire the shared async test lock.
The new sync tests call `let _guard = acquire_test_lock();` without `.await`, so they just create and drop a future; no lock is held while mutating `LOK_HEALTH_TTL`. That makes env-sensitive tests race-prone, especially because `HEALTH_CACHE_TTL` is a process-wide `OnceLock`. See [src/backend/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-406-health/src/backend/mod.rs:875) and [src/backend/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-406-health/src/backend/mod.rs:2284).

LOW: Invalid-value warning behavior is not actually tested.
The plan called for invalid string/integer tests to cover the warning path. Current tests only assert fallback duration; they do not capture or assert the one-line `warning:` output from [src/backend/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-406-health/src/backend/mod.rs:480).

## Missing Items

- Effective TTL startup log acceptance criterion is not implemented.
- Parser tests are incomplete for the warning-output acceptance criterion.
- Pre-merge gate is not verified here: `cargo fmt --check` passed, but `cargo test --no-run` could not run in this read-only sandbox because Cargo could not open `target/debug/.cargo-lock`.

## Recommendations

- Add a `OnceLock`-backed logging path that resolves and logs the TTL exactly once, ideally at the start of `Engine::warmup_backends()`, using `humantime::format_duration(ttl)` for readable output.
- Convert the new env-mutating parser tests to `#[tokio::test]` and `acquire_test_lock().await`, or use a separate synchronous mutex for environment tests.
- Add a warning-capture test for invalid values, including `"3600"`.
- Run the full required gate in a writeable environment: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
