# PRD: FR-15a — LOK_HEALTH_TTL env override for HealthCache TTL

## Overview

`CLO-406` enables a process-level TTL override for the backend health cache introduced by `CLO-388` so health checks do not stay permanently in memory for long-lived embedders of lok.

## Problem

`Engine::warmup_backends()` caches backend health results in-memory for the life of the process, and `Backend::is_available()` reads from that cache synchronously. In long-running hosts this means stale backend status can persist indefinitely, causing diagnostics and availability checks to be misleading when backend auth, binary availability, or connectivity changes.

`LOK_HEALTH_TTL` should provide an explicit, startup-only TTL policy for this cache while preserving existing one-shot behavior unless explicitly configured.

## Goals

1. Read `LOK_HEALTH_TTL` once at process startup.
2. Parse `LOK_HEALTH_TTL` with humantime format only (`"30s"`, `"5m"`, `"1h"`).
3. Reject invalid values with a one-line stderr warning and fall back to the compiled-in default TTL.
4. Keep parser and effective TTL decision unit-tested.
5. Log the effective TTL once at startup (`info:` line).
6. Preserve current default behavior for unset/empty env and one-shot CLI usage.
7. Add a setup-guide note describing when to raise or lower TTL.

## Requirements

### FR-15a.1 Env parsing and defaulting

- Add a startup-time parser in `src/backend/mod.rs` (module owning `BACKEND_CACHE`) that:
  - Reads `LOK_HEALTH_TTL` exactly once.
  - Accepts humantime strings only (`humantime::parse_duration`).
  - Rejects integer-only values and other invalid inputs.
  - Emits a warning to stderr (`warning:` prefix) when parsing fails.
  - Falls back to a compile-time default TTL (new `const` in cache-owning module).

### FR-15a.2 Cache expiry semantics

- Store/check a probe timestamp for each cached backend entry in `BACKEND_CACHE`.
- `Engine::is_backend_available()` returns `false` when cache age > effective TTL.
- Expired entries should not be treated as available on read.
- Expired entries can remain in cache until next warmup refresh or be removed during availability read.

### FR-15a.3 Resolution lifecycle

- TTL is resolved once at startup (no re-read during process lifetime).
- `warmup_backends()` always re-probes enabled entries that are missing or expired based on the currently resolved TTL policy.

### FR-15a.4 Observability

- Log one startup line:
  - `info: Health cache TTL: <duration>` (or equivalent one-line format)
- Keep warning messages one-line and parseable.

### FR-15a.5 Tests

- Add unit tests for TTL parser utility:
  1. valid humantime → parsed duration,
  2. unset/empty → default,
  3. invalid string/integer-like → default + warning capture path.
- Add/adjust cache tests in `src/backend/mod.rs` to cover expiry behavior and availability reads.

### FR-15a.6 Documentation

- Add a short note in `docs/guides/lok-setup-guide.md` under `lok doctor` covering when to increase TTL (flaky probes / remote changes between steps) vs lower it (CI with backend state churn).

## Acceptance criteria

- `LOK_HEALTH_TTL=10s lok doctor` triggers cache expiry behavior after the configured interval.
- Unset or empty `LOK_HEALTH_TTL` applies default TTL.
- Integer-only values are rejected with warning and fallback default.
- Invalid values do not alter startup exit code.
- `cargo test` and `cargo clippy -- -D warnings` pass.
- `docs/guides/lok-setup-guide.md` includes the new TTL usage guidance.

## Non-goals

- No CLI flag for TTL (env-only for this ticket).
- No persistence of health cache across process restarts.
- No cache invalidation in subcommands beyond `is_available`-read and warmup refresh behavior.
