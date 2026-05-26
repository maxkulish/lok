# CLO-406: FR-15a — LOK_HEALTH_TTL env override for HealthCache TTL

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-406/fr-15a-lok_health_ttl-env-override-for-healthcache-ttl
**Status**: Draft
**Created**: 2026-05-26
**Author**: Mk Km

---

## Summary

Add support for `LOK_HEALTH_TTL` to control how long the backend health cache entries in `BACKEND_CACHE` remain valid.

Today, the health cache is retained for the lifetime of the process; `Backend::is_available()` only reads `HealthStatus.available` from cache and does not evict stale entries. For long-lived embedders, this can make `lok doctor` and preflight availability checks report stale state after backend auth/binary/network changes.

This design introduces a startup-resolved TTL for cache entries and makes stale entries unavailable until refreshed by the next warmup pass.

## Background and Discovery Linkage

This design is based on:

- Discovery report: `docs/discovery/clo-406.md`
- PRD: `docs/prds/clo-406-healthcache-ttl.md`
- Chosen approach: **Approach A** (add `checked_at` metadata to `CachedBackend` entries)

Discovery baseline score was **8/10** and found minimal prior-art risk.

## Prior Research / Approach Selection

### Considered approaches

1. **Approach A (Chosen):** Add probe timestamp to `CachedBackend` and validate TTL on reads.
2. **Approach B:** Separate health cache entry wrapper map.
3. **Approach C:** Store timestamps in `HealthStatus`.

**Chosen (A)** because it keeps TTL ownership inside `backend` cache module and reuses existing cache keying and call paths with the smallest surface area.

## Requirements

### Functional requirements (from ticket)

1. Read `LOK_HEALTH_TTL` once at startup.
2. Parse value as humantime only (`30s`, `5m`, `1h`) via `humantime::parse_duration`.
3. Unset/empty -> fallback to compile-time default TTL.
4. Invalid values -> one-line stderr warning and fallback default (no abort).
5. Integer-only values are rejected the same way (humantime parser handles this).
6. Log effective TTL once at startup at info level.
7. Stale cache entries must not be treated as available.
8. Unit tests for parser and TTL behavior.

### Constraints

- Env-only config (no CLI flag).
- `is_available()` must remain a synchronous cache lookup path.
- No cross-process persistence.
- Keep behavior unchanged for ordinary one-shot command execution.

## Architecture

### Current points of truth

- `src/backend/mod.rs`: owns `BACKEND_CACHE` and `CachedBackend`.
- `src/backend/context.rs`: defines `HealthStatus`.
- `src/main.rs`: calls `Engine::warmup_backends()` from all hot paths (`ask`, `run_workflow`, `doctor`).

### Proposed cache shape

Add one field to cache entries:

- `CachedBackend { backend: Arc<dyn Backend>, health: Option<HealthStatus>, checked_at: Option<std::time::Instant> }`

`checked_at` is set when warmup writes a probed status and indicates probe recency.

### TTL resolution lifecycle

- At module initialization, resolve `LOK_HEALTH_TTL` exactly once into a resolved `Duration`:
  - Try to read `LOK_HEALTH_TTL`
  - parse with `humantime::parse_duration`
  - on failure: log `warning:` to stderr and return default
  - on missing/empty: return default
- Log resolved TTL once (single line): `info: Health cache TTL: <duration>`.

A helper such as `health_cache_ttl() -> Duration` reads a `OnceLock<Duration>`.

### Availability decision

`Engine::is_backend_available(name)` changes from:

- `available` field only

to:

1. locate cached entry
2. if no `health` or no `checked_at` => `false`
3. if `(checked_at + ttl) < now` => `false` (optionally clean entry)
4. else return `status.available`

### Warmup behavior

`warmup_backends()` should re-probe entries that are:

- not present in cache
- present with `health == None`
- present and stale (`checked_at` older than TTL)

This makes stale entries self-healing on the next lifecycle warmup.

## Implementation

### A) TTL parser and startup logging (in `src/backend/mod.rs`)

- Add constants:
  - `const DEFAULT_HEALTH_CACHE_TTL: Duration = Duration::from_secs(60 * 30);` (baseline)
  - `const HEALTH_TTL_ENV: &str = "LOK_HEALTH_TTL"` (or literal)
- Add static cache:
  - `static HEALTH_CACHE_TTL: OnceLock<Duration>`
- Add parser:
  - `fn resolve_health_cache_ttl() -> Duration`
- Add info-level startup log call in discovery/eager first-read point (e.g., first call to `Engine::warmup_backends()`), or module init path.
- Add tests:
  - valid humantime parsed
  - empty/unset yields default
  - invalid string + invalid integer-like input fallback + warning path

### B) Cache entry metadata

- Extend:
  - `CachedBackend` with `checked_at: Option<Instant>` (in `src/backend/mod.rs`).
- Update all cache insert sites:
  - `create_backend`: health inserted with `health=None`, `checked_at=None`
  - `warmup_backends` update path: insert `checked_at=Some(Instant::now())` when writing `health`.

### C) TTL-aware availability checks

- Update `Engine::is_backend_available()` to call freshness helper:
  - `fn is_cache_entry_fresh(entry: &CachedBackend, ttl: Duration) -> bool`
- If stale or missing freshness metadata: return false.

### D) Stale-entry behavior policy

- Keep stale entries in map, but effectively unavailable on read.
- Future warmup refresh will overwrite status and `checked_at`.

### E) Observability

- Log startup TTL once in human-readable format.
- Keep warnings one-line as required.

### F) Tests (in `src/backend/mod.rs`)

Add/adjust tests to cover:

- parser utility behavior
- stale read returns false after artificially simulating old `checked_at`
- expired entries are re-probed on next warmup
- `LOK_HEALTH_TTL` invalid values do not break startup flow

### G) Documentation

Add one paragraph under `lok doctor` in:

- `docs/guides/lok-setup-guide.md`

Guidance:

- Increase TTL for unstable/probe-heavy environments.
- Decrease TTL for CI where backend auth/binary state changes between steps.

## Risks and mitigations

- **Test flakiness with timing:** avoid fixed-sleep fragile assertions by manually setting `checked_at` via test locks/mutating cache entries where possible.
- **Monotonic time assumptions:** `Instant` avoids wall-clock drift and DST issues.
- **Semantics drift:** long-lived embeddings may see temporary false negatives until next warmup; acceptable and explicit by design.

## Acceptance criteria mapping

- `LOK_HEALTH_TTL=10s lok doctor` verifies expiry after 10s (with back-to-back invocations and sleep).
- Unset/empty use default TTL.
- Integer-only/unparseable values produce warning + fallback.
- Startup logs one-line effective TTL.
- `cargo test` and `cargo clippy -- -D warnings` succeed.
- Setup guide includes TTL tuning guidance.

## Open questions

None.
