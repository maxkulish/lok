# CLO-653 Implementation Plan: Key BACKEND_CACHE on configuration, not name alone

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-653
**Design Document**: docs/designs/clo-653-backend-cache-key.md
**Discovery Report**: docs/discovery/clo-653.md
**Created**: 2026-08-07
**Overall Progress**: 0% (0/113 tasks completed — 26 tasks across 7 phases, 113 checkboxes including subtasks)

---

## Architecture Context

`BACKEND_CACHE` memoizes constructed backends by name alone, so a second consumer asking for `"ollama"` with a different configuration silently receives the first one's instance. This replaces the `String` key with an owned `BackendKey { name, config, retry }`, gives each provider the key it was cached under so `is_available` can find itself, and closes two defects the design review surfaced: a construction race that erases completed health probes, and a name-only health read feeding a hard `UnknownModel` error.

The library crate is only `pub mod backend`. `main.rs:24` aliases `engine` as `backend`, so `engine.rs`, `workflow.rs` and `conductor.rs` are binary-only. That splits the work into a small public surface and a larger internal one.

Ordering is not arbitrary. Phase 1 must be observed failing before Phase 2 exists, and Phase 2 breaks compilation across the tree until Phase 4 lands — expect a red tree in between and do not treat it as a regression.

---

## Tasks

### Phase 1: Failing test first

- [x] Task 1: Build a recording HTTP server fixture
  - [x] Subtask 1.1: Add a dev-dependency for a local mock server (`wiremock`, or a hand-rolled `hyper` listener if the dependency budget is tight — check `Cargo.toml` for an existing option before adding one)
  - [x] Subtask 1.2: Write a helper in `src/backend/mod.rs` tests that starts a server on an ephemeral port and records every request body it receives
  - [x] Subtask 1.3: Confirm the helper works against a single `OllamaBackend` before using it to prove anything

- [x] Task 2: Write the proof-of-defect test
  - [x] Subtask 2.1: Add `two_configs_same_name_honour_their_own_endpoint_and_model` — two servers, two `BackendConfig`s differing in `command` (endpoint) and `model`
  - [x] Subtask 2.2: Call `query` on each returned backend and assert each server received exactly one request carrying its own model
  - [x] Subtask 2.3: Take `acquire_test_lock` and clear the cache, matching the existing convention

- [x] Task 3: Observe it failing
  - [x] Subtask 3.1: Run against unmodified `main`; confirm it fails because both requests reach the first server
  - [x] Subtask 3.2: Record the exact failure output in the workflow YAML — pointer inequality would not have proved this, so the failure mode itself is the evidence

### Phase 2: The key

- [ ] Task 4: Make the key types hashable
  - [ ] Subtask 4.1: Add `PartialEq, Eq, Hash` to the `BackendConfig` derive (`src/backend/config.rs:10`)
  - [ ] Subtask 4.2: Add `PartialEq, Eq, Hash` to the `RetryPolicy` derive (`src/backend/retry.rs:18`)

- [ ] Task 5: Introduce `BackendKey`
  - [ ] Subtask 5.1: Define `BackendKey { name: String, config: BackendConfig, retry: RetryPolicy }` with `Clone, Debug, PartialEq, Eq, Hash`
  - [ ] Subtask 5.2: Add `new(&str, &BackendConfig, &RetryPolicy)` — borrow, not move; `RetryPolicy` is not `Copy` and every caller reads it again
  - [ ] Subtask 5.3: Add a `name()` accessor
  - [ ] Subtask 5.4: Write rustdoc covering why `RetryPolicy` participates and the ambient-credential bound

- [ ] Task 6: Rekey the cache
  - [ ] Subtask 6.1: Change `BACKEND_CACHE` to `HashMap<BackendKey, CachedBackend>` (`src/backend/mod.rs:430`)
  - [ ] Subtask 6.2: Update `get_backend_cache`'s return type (`mod.rs:433`)
  - [ ] Subtask 6.3: Build the key once at the top of `create_backend` and use it for the read

- [ ] Task 7: Double-checked insert
  - [ ] Subtask 7.1: Import `std::collections::hash_map::Entry`
  - [ ] Subtask 7.2: Replace the unconditional `insert` (`mod.rs:405`) with `match lock.entry(key)` — `Occupied` returns the incumbent's `Arc` and discards the candidate, `Vacant` inserts
  - [ ] Subtask 7.3: Keep construction outside the lock; the write guard covers only the map operation
  - [ ] Subtask 7.4: Comment why, referencing that `set_mock_health` (`mod.rs:495`) already uses this shape

- [ ] Task 8: Migrate `set_mock_health`
  - [ ] Subtask 8.1: Change the signature to take `&BackendKey` (`mod.rs:491`) — public under `test-support`, so this is part of the break
  - [ ] Subtask 8.2: Keep the existing `entry().and_modify().or_insert()` body, which already preserves the cached backend

### Phase 3: Provider identity

- [ ] Task 9: Give each provider a key
  - [ ] Subtask 9.1: Add `key: Option<BackendKey>` to `OllamaBackend`, `CodexBackend`, `ClaudeBackend`, `GeminiBackend` and `BedrockBackend`
  - [ ] Subtask 9.2: Add `pub(crate) fn with_cache_key(self, key: BackendKey) -> Self` to each — crate-internal, so no caller can forge another entry's identity
  - [ ] Subtask 9.3: Leave all five `new(config)` signatures unchanged
  - [ ] Subtask 9.4: Wire `with_cache_key` into each arm of `create_backend`'s match

- [ ] Task 10: Rework `is_backend_available`
  - [ ] Subtask 10.1: Change it to take `&BackendKey` (`mod.rs:564`)
  - [ ] Subtask 10.2: Point all five provider `is_available` impls at `self.key`, using the closure form
  - [ ] Subtask 10.3: Confirm `RetryExecutor::is_available` (`retry.rs:146`) still delegates correctly and needs no key

- [ ] Task 11: Re-verify the behavioural change
  - [ ] Subtask 11.1: Re-grep for `.is_available()` on hand-built providers; confirm the design-time finding still holds against the tree as it now stands
  - [ ] Subtask 11.2: Confirm `conductor.rs:74` and `spawn.rs:74` still only use `api_details()`

### Phase 4: Binary-side call sites

- [ ] Task 12: Fix the warmup write-back
  - [ ] Subtask 12.1: Build the key alongside `retry_policy` in the `warmup_backends` loop (`engine.rs:107`)
  - [ ] Subtask 12.2: Use it for the skip-check at `engine.rs:100`
  - [ ] Subtask 12.3: Move the key into the future so the write-back is keyed identically, replacing `backend.name()` (`engine.rs:136-161`)
  - [ ] Subtask 12.4: Use `key.name()` in the warning messages so operator-visible output is unchanged

- [ ] Task 13: Rekey `get_cached_health`
  - [ ] Subtask 13.1: Change it to take `&BackendKey` (`engine.rs:195`)
  - [ ] Subtask 13.2: Build the key at `main.rs:795` from `backend_config` and `config.defaults` via `get_retry_policy`

- [ ] Task 14: Add the unambiguous health helper
  - [ ] Subtask 14.1: Write `unambiguous_cached_health(name) -> Option<HealthStatus>` in `engine.rs`, binary-only, returning `None` when zero or more than one entry matches
  - [ ] Subtask 14.2: Document that `None` means "cannot answer" and that it must never select an instance

- [ ] Task 15: Move the two `workflow.rs` reads
  - [ ] Subtask 15.1: `codex_unusable_flag_warnings` (`workflow.rs:104-108`) uses the helper and emits no warning on `None`
  - [ ] Subtask 15.2: Ollama model validation (`workflow.rs:222-224`) uses the helper and **skips the check** on `None`, never reaching `UnknownModel`
  - [ ] Subtask 15.3: Comment at the Ollama site that this path returns a hard error, which is why ambiguity must fail open

- [ ] Task 16: Update test helpers
  - [ ] Subtask 16.1: `Engine::is_backend_available` (`engine.rs:178`, `#[cfg(test)]`) takes a key
  - [ ] Subtask 16.2: `assert_probed` (`engine.rs:457`) takes a key
  - [ ] Subtask 16.3: `MockSyscallBackend` (`engine.rs:1115`) carries a key
  - [ ] Subtask 16.4: Migrate the roughly thirty call sites that insert or assert on bare names; the compiler enumerates them

### Phase 5: Documentation

- [ ] Task 17: Remove the stale constraint
  - [ ] Subtask 17.1: Rewrite the backend-cache section of the crate docs (`src/lib.rs:73-79`)
  - [ ] Subtask 17.2: Rewrite the `create_backend` caching rustdoc (`mod.rs:344-353`)
  - [ ] Subtask 17.3: Rewrite the `BACKEND_CACHE` known-constraint block (`mod.rs:420-430`)

- [ ] Task 18: Document what replaced it
  - [ ] Subtask 18.1: Document the ambient-credential bound — `api_key_env` holds a name, not a value; `ClaudeBackend` captures `env::var` at construction and `BedrockBackend` loads ambient AWS config — with distinct `api_key_env` names as the host-side answer
  - [ ] Subtask 18.2: Record the deferred host-owned-handle decision (Approach C) and why it is a superset rather than an alternative
  - [ ] Subtask 18.3: Note that `acquire_test_lock` stays, because the global stays
  - [ ] Subtask 18.4: Document the `set_mock_health` signature change for `test-support` consumers

- [ ] Task 19: Retire the standing constraint
  - [ ] Subtask 19.1: Remove the `BACKEND_CACHE` bullet from `docs/DEPENDENCIES.md` standing constraints, leaving the lib/bin one in place

### Phase 6: Testing & Validation

- [ ] Task 20: Add the remaining evaluation tests
  - [ ] Subtask 20.1: Test 2 — same endpoint, different model; two recorded requests carrying different models
  - [ ] Subtask 20.2: Test 3 — different `defaults.max_retries`; observably different attempt counts against a 500-returning server
  - [ ] Subtask 20.3: Test 4 — concurrent construction with a probe in between leaves `health: Some(..)` and returns one `Arc`
  - [ ] Subtask 20.4: Test 5 — warmup leaves exactly one probed entry per configured backend
  - [ ] Subtask 20.5: Test 6 — `is_available` true after warmup for a config-keyed entry
  - [ ] Subtask 20.6: Test 7 — provider built via `new()` alone reports `is_available() == false`
  - [ ] Subtask 20.7: Test 8 — `unambiguous_cached_health` answers with one config cached
  - [ ] Subtask 20.8: Test 9 — returns `None` with two configs cached, run repeatedly to defeat `HashMap` ordering
  - [ ] Subtask 20.9: Test 10 — two healthy Ollama configs with different inventories cause validation to skip, not to error

- [ ] Task 21: Cover the edge cases
  - [ ] Subtask 21.1: Cache cleared between `create_backend` and the warmup write-back; the write-back still lands
  - [ ] Subtask 21.2: `health: None` versus `Some(unavailable)` distinction still drives warmup's skip logic
  - [ ] Subtask 21.3: Same key, different ambient `ANTHROPIC_API_KEY` — assert the documented shared-instance behaviour so the bound is pinned

- [ ] Task 22: Extend the public-surface test
  - [ ] Subtask 22.1: In `tests/backend_public_api.rs`, construct a `BackendKey`, reach the cache via `get_backend_cache`, and call `set_mock_health` in its new form — proving all three migrations from outside the crate

- [ ] Task 23: Run the full CI contract
  - [ ] Subtask 23.1: `cargo fmt --check`
  - [ ] Subtask 23.2: `cargo clippy --locked --all-targets -- -D warnings`
  - [ ] Subtask 23.3: `cargo test --locked`
  - [ ] Subtask 23.4: `cargo clippy --locked --all-targets --features bedrock -- -D warnings`
  - [ ] Subtask 23.5: `cargo test --locked --features bedrock`
  - [ ] Subtask 23.6: `cargo build --locked --lib --no-default-features`
  - [ ] Subtask 23.7: `cargo test --locked --lib --no-default-features`
  - [ ] Subtask 23.8: `cargo clippy --locked --lib --tests --no-default-features -- -D warnings`
  - [ ] Subtask 23.9: `RUSTDOCFLAGS='-D missing_docs' cargo doc --locked --no-deps --lib --all-features` — will reject `BackendKey` and its methods if undocumented
  - [ ] Subtask 23.10: MSRV 1.83 `cargo check --locked --all-targets`
  - [ ] Subtask 23.11: Bedrock MSRV 1.88 `cargo check --locked --all-targets --features bedrock`

- [ ] Task 24: Measure the hashing cost
  - [ ] Subtask 24.1: Time 10k `create_backend` calls on a warm cache; compare against the same loop with a pre-built key
  - [ ] Subtask 24.2: Record the number. If key construction exceeds 5% of `create_backend` wall time, file a follow-up to hoist the key at call sites — no design change needed, since all nine already hold the inputs

- [ ] Task 25: Manual verification
  - [ ] Subtask 25.1: `cargo run -- doctor` before and after; the health table must show the same backends with the same statuses. Treat this as a smoke test, not acceptance evidence — the output is not deterministic enough for that

### Phase 7: Finalization

- [ ] Task 26: Create the PR
  - [ ] Subtask 26.1: Verify commits follow `fix(CLO-653): description`
  - [ ] Subtask 26.2: Push `fix/clo-653-backend-cache`
  - [ ] Subtask 26.3: Open the PR, calling out the three signature breaks and the one behavioural change explicitly in the body
  - [ ] Subtask 26.4: Note in the PR that the library surface is unpublished, so the break reaches no consumer, and link CLO-660
  - [ ] Subtask 26.5: Link the PR to CLO-653 and request review

---

## Module Structure

**Library (public surface changes here):**
- `src/backend/mod.rs` — `BackendKey`, cache map type, `create_backend`, `is_backend_available`, `set_mock_health`
- `src/backend/config.rs` — derives on `BackendConfig`
- `src/backend/retry.rs` — derives on `RetryPolicy`
- `src/backend/{ollama,codex,claude,gemini,bedrock}.rs` — key field, builder, `is_available`
- `src/lib.rs` — crate docs

**Binary only (free to change):**
- `src/engine.rs` — warmup keying, `get_cached_health`, `unambiguous_cached_health`, test helpers
- `src/workflow.rs` — the two name-only health reads
- `src/main.rs` — key construction at the `doctor` call site

**Tests:**
- `tests/backend_public_api.rs` — external-consumer proof

**Docs:**
- `docs/DEPENDENCIES.md` — retire the standing constraint

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage will be recalculated based on completed tasks.

---

## Notes

- Phase 1 must be observed failing before Phase 2 begins, and it must fail on *configuration*, not on pointer identity. `OllamaBackend`'s `base_url` and `model` are private, so two distinct `Arc`s prove allocation and nothing else
- The tree will not compile between Phase 2 and Phase 4. That is expected — rekeying the map breaks every reader at once. Do not partially revert to chase green
- A cache write must never replace `health: Some(..)` with `health: None`. That is the defect Task 7 exists to prevent
- Ambiguity in a name-only health read must fail open, never pick. The Ollama path rejects user workflows
- Do not fix CLO-655, CLO-656 or CLO-660 on this branch, however tempting while the review pipeline is broken
- Out of scope, recorded in the design: threading the real `BackendKey` into workflow validation, and Approach C's host-owned cache handle
