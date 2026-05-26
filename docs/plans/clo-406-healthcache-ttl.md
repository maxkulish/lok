# Plan: CLO-406 — FR-15a: LOK_HEALTH_TTL env override for HealthCache TTL

## Context
- Design: [docs/design-docs/clo-406-healthcache-ttl.md](../design-docs/clo-406-healthcache-ttl.md)
- Discovery: [docs/discovery/clo-406.md](../discovery/clo-406.md)
- PRD: [docs/prds/clo-406-healthcache-ttl.md](../prds/clo-406-healthcache-ttl.md)
- Linear: https://linear.app/cloud-ai/issue/CLO-406/fr-15a-lok-health-ttl-env-override-for-healthcache-ttl

---

## Sub-tasks

### ST1: Extend `CachedBackend` with probe timestamp

**Files:** `src/backend/mod.rs`
**Depends on:** none

**Changes:**
- Add `checked_at: Option<std::time::Instant>` to `CachedBackend` (`lines ~414–417`).
- Update all `CachedBackend` construction sites:
  - `create_backend()` (`line ~388–392`): insert with `checked_at: None` alongside `health: None`.
  - `Engine::warmup_backends()` (`line ~545–549`): overwrite with `checked_at: Some(Instant::now())` when storing probed health.
- Update `set_mock_health()` test helper (`line ~451–463`): also set/overwrite `checked_at: Some(Instant::now())` on insert and modify paths.
- Update `Engine::warmup_backends()` skip-logic (`line ~480–488`): change `entry.health.is_some()` to `is_cache_entry_fresh(entry, ttl)` (or equivalent helper) so stale entries get re-probed.

**Acceptance:** `cargo test -- backend::` compiles and passes.

**Estimate:** S

### ST2: Add `LOK_HEALTH_TTL` env parser and startup resolution

**Files:** `src/backend/mod.rs` (or `src/backend/cache_ttl.rs` if structurally preferred)
**Depends on:** ST1

**Changes:**
- Add compile-time default: `const DEFAULT_HEALTH_CACHE_TTL: Duration = Duration::from_secs(60 * 30);`.
- Add env key constant: `const HEALTH_TTL_ENV: &str = "LOK_HEALTH_TTL";`.
- Add static: `static HEALTH_CACHE_TTL: OnceLock<Duration> = OnceLock::new();`.
- Add resolver:
  ```rust
  fn resolve_health_cache_ttl() -> Duration {
      // read env var, parse via humantime::parse_duration,
      // warn + fallback on failure
  }
  ```
- Add public accessor:
  ```rust
  pub fn health_cache_ttl() -> Duration {
      *HEALTH_CACHE_TTL.get_or_init(resolve_health_cache_ttl)
  }
  ```
- Eagerly resolve TTL on first `Engine::warmup_backends()` call, logging exactly one line to stdout:
  - `info: Health cache TTL: 30m` (humanized format)

**Acceptance:** `cargo test -- backend::` passes. Running `LOK_HEALTH_TTL=10s cargo run -- doctor 2>/dev/null` shows startup TTL line (manual verification).

**Estimate:** S

### ST3: TTL-aware availability checks

**Files:** `src/backend/mod.rs`
**Depends on:** ST2

**Changes:**
- Add private helper:
  ```rust
  fn is_cache_entry_fresh(entry: &CachedBackend, ttl: Duration) -> bool {
      entry.checked_at.map(|t| t.elapsed() <= ttl).unwrap_or(false)
  }
  ```
- Update `Engine::is_backend_available(name: &str)` (`line ~558–567`):
  - After locating `entry.health`, also verify `is_cache_entry_fresh(entry, ttl)`.
  - Missing / stale entry → `false`.
- Update `Engine::warmup_backends()` skip logic (already adjusted in ST1 for re-probe on stale):
  - Confirm stale entries are re-probed by this point.
  - Entry with `health=None` (but in map) is also re-probed.

**Acceptance:** `cargo test -- backend::` passes.

**Estimate:** S

### ST4: Unit tests for TTL parser and cache expiry

**Files:** `src/backend/mod.rs` (test module, end of file)
**Depends on:** ST3

**Changes:**
- TTL parser tests (add to existing `#[cfg(test)]` block):
  1. `test_ttl_parser_valid` — `"10s"`, `"5m"`, `"1h"` → match expected `Duration`.
  2. `test_ttl_parser_unset` — env absent → `DEFAULT_HEALTH_CACHE_TTL`.
  3. `test_ttl_parser_empty` — `LOK_HEALTH_TTL=""` → default.
  4. `test_ttl_parser_invalid_humantime` — `"banana"` → default + captured warning.
  5. `test_ttl_parser_integer_only` — `"3600"` → default + warning (humantime rejects this).
- Cache expiry test:
  6. `test_is_backend_available_expired` — insert fresh entry, artificially backdate `checked_at` by `ttl + 1s`, assert `is_backend_available` returns `false`.
- Cache refresh test:
  7. `test_warmup_reprobes_stale` — pre-populate stale entry (old `checked_at`), run `warmup_backends`, assert `checked_at` is updated.
- Ensure test parallelism safety via existing `acquire_test_lock()` pattern.

**Acceptance:** `cargo test -- backend::` passes (including all new tests).

**Estimate:** M

### ST5: Add setup-guide TTL tuning note

**Files:** `docs/guides/lok-setup-guide.md`
**Depends on:** none (content only)

**Changes:**
- Add a new subsection `### HealthCache TTL` inside the existing `## lok doctor` section (`~line 870`):
  - Explain `LOK_HEALTH_TTL` env override with a short example (`LOK_HEALTH_TTL=10s lok doctor`).
  - One paragraph: raise TTL for flaky or slow probes, lower it for CI where backend auth/binaries change between steps.

**Acceptance:** Text review; no code changes.

**Estimate:** S

### ST6: Pre-merge gate verification

**Files:** all changed files.
**Depends on:** ST5 (all prior)

**Changes:**
- Format, lint, and run full test suite.

**Acceptance:**
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

**Estimate:** S

---

## Pre-merge gate
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Risks

- **Cache shape churn in existing tests:** Adding `checked_at` to `CachedBackend` will break any tests that use `CachedBackend` literal construction or directly assert on struct fields. Mitigation: update all construction sites in one pass during ST1.
- **Timing flakiness:** Tests asserting on `Instant` durations can flake if the test host is under load. Mitigation: backdate `checked_at` artificially (via direct field mutation in tests), rather than `sleep`ing in async tests.
- **Env leakage between tests:** `HEALTH_CACHE_TTL` uses a `OnceLock`; without careful isolation, setting `LOK_HEALTH_TTL` in one test could bleed into another. Mitigation: only set env var inside a single test after acquiring the test lock, and ensure `OnceLock` is reset or test order isolates the env behavior (or write unit tests that directly test `resolve_health_cache_ttl` via an internal test-only path).
