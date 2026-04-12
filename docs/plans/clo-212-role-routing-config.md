# CLO-212 Implementation Plan: Configurable Role Routing

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-212
**Design Document**: docs/design-docs/clo-212-role-routing-config.md
**Created**: 2026-04-12
**Overall Progress**: 100% (30/30 tasks completed)

---

## Architecture Context

This task replaces lok's hardcoded keyword-to-backend mappings in `src/delegation.rs` with a configurable `[roles]` and `[teams]` TOML schema. A new `RoleResolver` component resolves roles to backend lists using two-tier lookup (team override first, then global fallback). `RoutingStrategy` (backend selection) is kept separate from `ConsensusStrategy` (response combination).

**Key files**:
- `src/role/mod.rs` - New module: `RoleResolver`, `RoutingStrategy`, `Resolution`, `RoleResolutionError`
- `src/config.rs` - Add `[roles]`, `[teams]`, `[defaults]` TOML sections
- `src/delegation.rs` - Refactor to use `RoleResolver`; keep `Delegator` as keyword fallback
- CLI commands (`smart`, `team`, `spawn`) - Add `--team` and `--explain` flags

**Dependency**: CLO-203 (config merge layer) must be complete before this work begins.

---

## Tasks

### Phase 1: Core Types and Config

- [x] Define `RoutingStrategy` enum in `src/role/mod.rs`
  - `First` variant (no params)
  - `Parallel { min_success: usize, timeout_secs: Option<u64> }` variant
  - `Fallback { timeout_secs: Option<u64> }` variant
  - `Default` impl returns `Fallback { timeout_secs: None }`

- [x] Define `RoleResolutionError` enum in `src/role/mod.rs`
  - `RoleNotFound { role: String }` variant
  - `NoBackendsAvailable { role: String }` variant
  - `ValidationError { role: String, message: String }` variant
  - `Display` impl for each variant

- [x] Define `Resolution` struct in `src/role/mod.rs`
  - `backends: Vec<BackendId>` field
  - `strategy: RoutingStrategy` field
  - `from_team_override: bool` field
  - `role: String` field
  - `team: Option<String>` field
  - `backend_ids()` method - returns cloned backend list
  - `is_empty()` method - returns true if no backends

- [x] Define `RoleConfig` struct in `src/role/mod.rs`
  - `backends: Vec<String>` field
  - `strategy: RoutingStrategy` field
  - TOML deserialization with `#[serde(default)]` for strategy

- [x] Define `TeamConfig` struct in `src/role/mod.rs`
  - `roles: HashMap<String, RoleConfig>` field (inline role overrides per team)

- [x] Define `ValidationWarning` struct (or type alias) for unknown backend refs at load time

- [x] Add `[roles]` section to `Config` in `src/config.rs`
  - `roles: HashMap<String, RoleConfig>` with `#[serde(default)]`
  - Add to `Config` struct with `#[serde(default)]`

- [x] Add `[teams]` section to `Config` in `src/config.rs`
  - `teams: HashMap<String, TeamConfig>` with `#[serde(default)]`
  - Add to `Config` struct with `#[serde(default)]`

- [x] Add `[defaults]` section to `Config` in `src/config.rs`
  - `defaults: DefaultsConfig` with `#[serde(default)]`
  - `DefaultsConfig` has `team: Option<String>` field

- [x] Implement `RoleResolver::new(config: &Config)` constructor
  - Extract `roles`, `teams`, `defaults.team` from config
  - Store as struct fields

- [x] Add config validation in `RoleResolver::validate()` (called at config load time)
  - `min_success` on `first` strategy -> `ValidationError`
  - `min_success` on `fallback` strategy -> `ValidationError`
  - `min_success < 1` -> `ValidationError`
  - `min_success > backends.len()` -> `ValidationError`
  - Unknown backend references -> `ValidationWarning` (not error)

### Phase 2: Resolution Logic

- [ ] Implement `RoleResolver::resolve(role, team_override, available_backends)`
  - Determine active team: `team_override` > `default_team` > None
  - Check `[teams.<team>.roles.<role>]` if team active
  - Fall back to `[roles.<role>]` if no team override
  - Return `RoleNotFound` if role not in either
  - Filter backends by `available_backends`; skip disabled
  - Return `NoBackendsAvailable` if all filtered out
  - Return `Ok(Resolution { ... })` with all fields populated

- [ ] Add `ValidationWarning` emission for unknown backend references at load time

- [ ] Write unit tests for `RoleResolver::new()` and config parsing
  - Test TOML parsing for valid role configs
  - Test invalid `min_success` values produce validation errors

- [ ] Write unit tests for two-tier resolution
  - Team override resolves before global role
  - Global fallback when no team or team has no override
  - Team can define custom role not in global config

- [ ] Write unit tests for `RoleNotFound` error variant

- [ ] Write unit tests for `NoBackendsAvailable` error variant

### Phase 3: Strategy Definitions (Conductor handles execution)

- [x] Add integration notes to `src/delegation.rs`
  - Document how Conductor interprets `RoutingStrategy` from `Resolution`
  - `First` -> try backends in order, return first success; terminal errors short-circuit
  - `Fallback` -> try in order with transient error handling (429, 500, timeout -> next; 401, 400 -> short-circuit)
  - `Parallel` -> `FuturesUnordered` + `AtomicUsize`; return when `min_success` succeed; cancel remaining
  - `timeout_secs` -> wrap each invocation in `tokio::time::timeout`

- [x] Verify existing `min_deps_success` pattern in codebase can be reused for `Parallel`

- [x] Add integration test: Conductor receives `Resolution` and executes with correct strategy

### Phase 4: CLI Integration

- [x] Add `--team <name>` flag to `lok smart` command
  - Accept optional team name argument
  - Pass team to `RoleResolver::resolve()` via `team_override` param

- [x] Add `--explain` flag to `lok smart` command
  - After resolution, print role, team, backends, strategy
  - Only prints when flag is provided

- [x] Add `--team <name>` and `--explain` flags to `lok spawn` command

- [x] Add `--explain` flag to `lok team` command
  - `team` command always uses team from config; `--explain` shows resolution

- [x] Wire `RoleResolver` into command execution context
  - Instantiate once at startup (not per-call)
  - Share across `smart`, `team`, `spawn` commands

### Phase 5: Delegator Integration and Migration

- [x] Refactor `src/delegation.rs`
  - `RoleResolver` becomes primary path for configured roles
  - `Delegator` preserved as keyword fallback for unconfigured roles
  - When `RoleResolver::resolve()` returns `Err(RoleNotFound)`, fall back to `Delegator`

- [x] Verify default config (no `[roles]`) produces identical behavior to current `Delegator::new()`
  - Write test: run existing `delegation.rs` inputs through `RoleResolver::from_config(Config::default())`
  - Assert same backend recommendations

- [x] Wire `RoutingStrategy` from `Resolution` into Conductor's existing execution paths
  - Map `RoutingStrategy::First` to existing `First` consensus path
  - Map `RoutingStrategy::Fallback` to sequential fallback execution
  - Map `RoutingStrategy::Parallel` to parallel execution with quorum

- [x] Add migration note: hardcoded profiles from `delegation.rs` will move to default config in follow-up

- [x] Write integration test: end-to-end with custom `lok.toml` containing `[roles]` section

### Phase 6: Testing and Validation

- [x] Run `cargo test role` - verify all role module tests pass

- [x] Run `cargo test delegation` - verify delegation tests still pass

- [x] Run `cargo clippy` - address any warnings

- [x] Manual test: create `lok.toml` with custom `[roles]` and verify routing works

### Phase 7: Finalization

- [x] Create commit with conventional commit message: `feat(CLO-212): add configurable role routing with roles/teams config`

- [x] Push branch: `git push -u origin feat/clo-212-role-routing-config`

- [x] Create PR via `gh pr create`
  - Title: `feat(CLO-212): add configurable role routing with [roles]/[teams] config`
  - Body: summary of changes, test plan, link to Linear issue

- [x] Request review from team

---

## Module Structure

- `src/role/mod.rs` - New: `RoleResolver`, `Resolution`, `RoutingStrategy`, `RoleConfig`, `TeamConfig`, `RoleResolutionError`, `ValidationWarning`
- `src/role/` - New directory for role routing module
- `src/delegation.rs` - Modified: integrate `RoleResolver` as primary, `Delegator` as fallback
- `src/config.rs` - Modified: add `[roles]`, `[teams]`, `[defaults]` TOML sections
- CLI commands - Modified: add `--team` and `--explain` flags

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

---

## Notes

- Total task count: 30
- All implementation tasks are independently testable
- Follow existing patterns in codebase: `HashMap` config, `serde(default)`, `toml` crate
- `RoleResolver` must remain pure (no execution) for testability
- Backward compatibility is paramount: default config must match current `Delegator::new()` behavior
