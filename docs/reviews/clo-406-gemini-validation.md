# Pre-PR validation: clo-406

**Reviewer**: Gemini (gemini-3.5-flash)
**Validated**: 2026-05-26
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

### 1. [HIGH] Test Isolation & Concurrency Leak in TTL Parser Unit Tests
- **Location**: `src/backend/mod.rs` (Lines 2284–2318)
- **Problem**: The new unit tests `test_ttl_parser_valid` and `test_ttl_parser_invalid_fallback` are synchronous `#[test]` functions, yet they attempt to acquire the global test lock via `let _guard = acquire_test_lock();`. Since `acquire_test_lock()` is an `async fn` returning a `Future`, calling it without `.await` does not actually block or acquire the lock. This causes these tests to run concurrently with other tests while modifying global process environment variables (`std::env::set_var`/`remove_var`), leading to thread safety issues and test flakiness.
- **Impact**: Breaking the test suite reliability and violating test isolation standards.

### 2. [MEDIUM] Missing Startup Effective TTL Log
- **Location**: `src/backend/mod.rs` (Lines 476–498)
- **Problem**: The design document and PRD both state as a functional requirement: *"Log effective TTL once at startup at info level (`info: Health cache TTL: <duration>`)".* However, neither `resolve_health_cache_ttl()` nor `health_cache_ttl()` contains this output statement.
- **Impact**: Fails to meet Functional Requirement **FR-15a.4 (Observability)**.

### 3. [LOW] Technical Debt / Debug Formatting in Warnings
- **Location**: `src/backend/mod.rs` (Line 489)
- **Problem**: When `resolve_health_cache_ttl()` encounters an invalid override value, it outputs the fallback default TTL using debug representation: `{:?}`.
- **Impact**: Reduces warning readability.

## Recommendations

1. Fix test isolation by converting parser unit tests to `#[tokio::test]` with `.await` on lock acquisition
2. Add info-level startup log with humantime-formatted TTL duration
3. Format fallback TTL in warnings using `humantime::format_duration`
