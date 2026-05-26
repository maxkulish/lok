# Pre-PR validation: clo-406

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-26
**Pipeline**: lok pre-pr-validation
---

Both critical findings are verified by direct file inspection. The new sync tests at lines 2286/2309 call `acquire_test_lock()` (an async fn) without `.await`, so the lock is never acquired. The required `info: Health cache TTL: <duration>` startup log is missing — no instance of "Health cache TTL" exists anywhere in the source.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Reported FAIL; findings verified against source |
| Gemini | OK | Reported FAIL; findings verified against source |
| Claude fallback | SKIPPED | At least one external reviewer succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- **Sync TTL parser tests do not hold the test lock.** `src/backend/mod.rs:2286` and `:2309` call `let _guard = acquire_test_lock();` without `.await`. Since `acquire_test_lock()` is `async fn` (defined at `:875`), the call constructs a future and immediately drops it without ever acquiring the global `Mutex<()>`. Because `HEALTH_CACHE_TTL` is a process-wide `OnceLock` and these tests mutate `std::env::set_var(HEALTH_TTL_ENV, ...)`, they will race with every other test that reads env or warms the cache. Convert both to `#[tokio::test]` + `.await`, or guard env-mutating tests with a dedicated `std::sync::Mutex`. Both reviewers flagged this; Gemini correctly rated it HIGH.
- **Missing required startup TTL log (FR-15a.4).** Design doc line 81 and acceptance criteria line 180 require a single `info: Health cache TTL: <duration>` line at startup. `resolve_health_cache_ttl()` only emits the `warning:` path; nothing emits the success log. Add a `OnceLock`-backed log at the first eager call site (e.g., the top of `Engine::warmup_backends()` in `src/backend/mod.rs:511`), formatted via `humantime::format_duration(ttl)`.

## Out of Scope / Deferred
- **Invalid-value warning capture test.** Codex notes the parser tests don't capture the `warning:` stderr line for invalid input. Capturing stderr in unit tests adds complexity (custom writer or `gag` crate) for a one-line `eprintln!`. Adding manual integration coverage via `LOK_HEALTH_TTL=banana cargo run -- doctor` in the QA checklist is sufficient for this PR.
- **`{:?}` formatting of fallback duration in warning message** (Gemini LOW). Cosmetic; can ride with the startup log fix if convenient (use `humantime::format_duration`) but not blocking on its own.

## False Positives / Tooling Artifacts
- **Codex "pre-merge gate not verified — `target/debug/.cargo-lock` not openable in read-only sandbox."** Tooling artifact from the reviewer's sandbox; not a code defect. Local `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` must still pass before PR but it's not a finding against the change.

## Recommendation
PROCEED_WITH_FIXES. Two bounded fixes in `src/backend/mod.rs`: (1) convert `test_ttl_parser_valid` and `test_ttl_parser_invalid_fallback` to `#[tokio::test]` with `.await` on `acquire_test_lock()` (clearing `HEALTH_CACHE_TTL` is unnecessary since the tests call `resolve_health_cache_ttl()` directly, not the OnceLock helper); (2) add a one-shot info log of the resolved TTL at the start of `Engine::warmup_backends()` using `humantime::format_duration`. Then run the full gate (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`) before opening the PR.
