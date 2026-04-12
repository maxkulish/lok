# CLO-212: Add Configurable Role Routing with [roles]/[teams] Config

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-212
**Status**: Design
**Author**: Mk Km
**Created**: 2026-04-12

---

## Summary

Replace lok's hardcoded keyword-to-backend mappings with a configurable `[roles]` and `[teams]` TOML schema. A new `RoleResolver` component resolves roles to backend lists using a two-tier lookup (team override first, then global fallback). Execution strategies (`First`, `Parallel`, `Fallback`) are defined as a `RoutingStrategy` enum separate from `ConsensusStrategy`. The existing `Delegator` is preserved as the fallback when no config exists, guaranteeing backward compatibility.

---

## Background

lok's `src/delegation.rs` currently hardcodes backend profile-to-task-category mappings. Backend profiles and routing decisions are baked into source code, making it impossible to customize routing without code changes.

The goal is to make routing data-driven via `lok.toml` configuration while preserving all existing behavior as the default.

### Prior Research

Discovery phase (2026-04-12) produced a comprehensive report identifying 4 killer assumptions and validating the approach against prior art from TensorZero, API7/APISIX, AWS hedging patterns, K8s node affinity, LiteLLM, and RouteLLM.

**Key findings from discovery**:

| Finding | Implication |
|---------|-------------|
| `ConsensusStrategy` and routing strategy are different concerns being conflated | Define `RoutingStrategy` (backend selection) separate from `ConsensusStrategy` (response combination) |
| `[teams.frontend]` with flat string lists creates ambiguous merge semantics | Use inline tables `[teams.<name>.roles.<role_name>]` for consistent structure |
| CLI surface undefined | Add `--team <name>` flag and `--explain` to `smart`, `team`, `spawn` commands |
| `min_success` defaults undefined | Default to `backends.len()` for parallel; reject for `first` and `fallback` at config validation time |

---

## Architecture

### Component Overview

`RoleResolver` is a new component that reads the `[roles]` and `[teams]` config sections and returns a `Resolution` struct describing which backends to invoke and with which strategy.

```
                    ┌─────────────────────────────────────────────┐
                    │                  Config                     │
                    │  [roles]        [teams]      [defaults]      │
                    └────────────┬──────────────┬────────────────┘
                                 │             │
                                 ▼             ▼
                    ┌──────────────────────────────┐
                    │        RoleResolver          │
                    │  two_tier_resolve(role, team) │
                    │  returns Resolution          │
                    └──────────────┬───────────────┘
                                   │
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
        ┌──────────┐        ┌────────────┐        ┌──────────┐
        │  Smart   │        │   Team     │        │  Spawn   │
        │ command  │        │  command   │        │ command  │
        └──────────┘        └────────────┘        └──────────┘
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/delegation.rs` | Modified | Refactor to use `RoleResolver`; keep `Delegator` as keyword fallback |
| `src/config.rs` | Modified | Add `[roles]` and `[teams]` TOML sections with `#[serde(default)]` |
| `src/role/mod.rs` | New | New module: `RoleResolver`, `RoutingStrategy`, `Resolution` struct |
| `src/role/strategies.rs` | New | Strategy implementations: `First`, `Parallel`, `Fallback` |
| CLI commands (`smart`, `team`, `spawn`) | Modified | Add `--team <name>` and `--explain` flags |

### Dependencies

- **Internal**: `Config` (from CLO-203 config merge layer), `Backend` trait, existing `ConsensusStrategy`
- **External**: `toml` crate (already in use), `futures` (already in use via ` FuturesUnordered`)

---

## Detailed Design

### Implementation Approach

**Step 1: Define `RoutingStrategy` enum** (separate from `ConsensusStrategy`)

```rust
/// Controls backend selection, not response combination.
/// Composes with ConsensusStrategy: routing selects backends, consensus combines outputs.
///
/// RoleResolver does NOT execute requests. It returns a Resolution containing
/// the backend IDs and strategy. The Conductor/Orchestration layer interprets
/// the Resolution and executes the actual HTTP requests. This separation keeps
/// routing logic pure and easy to test without mocking async HTTP clients.
pub enum RoutingStrategy {
    /// Use the first available backend that responds.
    /// Terminal errors (401, 400) short-circuit immediately; transient errors (429, 500, timeout)
    /// do not trigger fallback within this strategy (use Fallback strategy for that).
    First,
    /// Fire requests to all backends in parallel; return when min_success respond.
    /// Remaining in-flight requests are cancelled once min_success is reached.
    /// All backends are wrapped in a timeout (configurable via `timeout_secs` or global default).
    Parallel { min_success: usize, timeout_secs: Option<u64> },
    /// Try backends in order, stopping at first successful response.
    /// Only transient errors (429, 500,  timeout) trigger the next backend.
    /// Terminal errors (401, 400) short-circuit the fallback chain immediately.
    Fallback { timeout_secs: Option<u64> },
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        RoutingStrategy::Fallback { timeout_secs: None }
    }
}
```

**Step 2: Define `Resolution` and `RoleResolutionError` structs**

```rust
/// Result of role resolution: which backends to invoke and with which strategy.
/// Returned by RoleResolver::resolve(). The caller (Conductor) interprets the
/// Resolution and executes actual backend requests.
pub struct Resolution {
    /// Ordered list of backends to invoke.
    pub backends: Vec<BackendId>,
    /// Execution strategy for these backends.
    pub strategy: RoutingStrategy,
    /// Whether this resolution came from a team override.
    pub from_team_override: bool,
    /// Role name that was resolved.
    pub role: String,
    /// Team used for this resolution, if any.
    pub team: Option<String>,
}

impl Resolution {
    /// Returns the ordered list of backend IDs for consumers to use directly.
    pub fn backend_ids(&self) -> Vec<BackendId> {
        self.backends.clone()
    }

    /// Returns true if no backends are available after filtering.
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }
}

/// Errors that can occur during role resolution.
#[derive(Debug, Clone)]
pub enum RoleResolutionError {
    /// Role was not found in global config or any team override.
    RoleNotFound { role: String },
    /// All configured backends are disabled or unavailable.
    NoBackendsAvailable { role: String },
    /// Config validation error (e.g., min_success out of bounds).
    ValidationError { role: String, message: String },
}

impl std::fmt::Display for RoleResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoleResolutionError::RoleNotFound { role } => {
                write!(f, "role '{}' not found in any configuration", role)
            }
            RoleResolutionError::NoBackendsAvailable { role } => {
                write!(f, "no available backends for role '{}'", role)
            }
            RoleResolutionError::ValidationError { role, message } => {
                write!(f, "validation error for role '{}': {}", role, message)
            }
        }
    }
}
```

**Step 3: Define config schema**

```toml
# Global role definitions
[roles.code_review]
backends = ["claude", "gemini"]
strategy = { Fallback = {} }

[roles.security_audit]
backends = ["gemini", "claude"]
strategy = { Parallel = { min_success = 1, timeout_secs = 30 } }

# Team-specific overrides (project-level ./lok.toml)
[teams.frontend.roles.code_review]
backends = ["claude"]
strategy = { First = {} }

# Defaults
# Omit [defaults.team] or leave unset for no team override;
# CLI --team flag overrides this for a single call
```

**Step 4: `RoleResolver::resolve()` with two-tier lookup and team override**

```rust
pub struct RoleResolver {
    roles: HashMap<String, RoleConfig>,
    teams: HashMap<String, TeamConfig>,
    default_team: Option<String>,
}

impl RoleResolver {
    /// Resolve a role to a Resolution.
    ///
    /// Resolution order:
    /// 1. If `team_override` is provided, use that team; else use `default_team`; else no team
    /// 2. If a team is active and `[teams.<team>.roles.<role>]` exists, use team override
    /// 3. Else if `[roles.<role>]` exists, use global role
    /// 4. Filter out backends not in `available_backends`
    /// 5. Disabled backends are skipped
    ///
    /// The `--team` CLI flag overrides `[defaults.team]` config. Passing `team_override`
    /// is equivalent to setting `[defaults.team]` temporarily for this one resolution.
    pub fn resolve(
        &self,
        role: &str,
        team_override: Option<&str>,
        available_backends: &[BackendId],
    ) -> Result<Resolution, RoleResolutionError> {
        // 1. Determine active team: team_override > default_team > None
        // 2. If active team exists in [teams], check for role override
        // 3. If team override found, use it; else fall back to [roles.<role>]
        // 4. If role not found in either, return RoleNotFound
        // 5. Filter backends by availability; if none remain, return NoBackendsAvailable
    }

    /// Validate that all backend references in roles/teams are valid.
    /// Called at config load time.
    pub fn validate(&self, known_backends: &[BackendId]) -> Vec<ValidationWarning> {
        // Returns warnings for unknown backends, errors for malformed config
    }
}
```

**Step 5: Config validation rules**

| Strategy | Valid `min_success` | Notes |
|----------|---------------------|-------|
| `first` | Rejected | Not applicable - takes first available |
| `fallback` | Rejected | Not applicable - stops at first success |
| `parallel` | Required; defaults to `backends.len()` | Must be >= 1 and <= backends.len() |

**Step 6: CLI integration**

| Command | New Flags | Behavior |
|---------|-----------|----------|
| `lok smart` | `--team <name>`, `--explain` | Pass team to `RoleResolver`, `--explain` prints resolution |
| `lok team` | `--explain` | Always uses team from config; `--explain` shows resolution |
| `lok spawn` | `--team <name>`, `--explain` | Same as `smart` |

**Step 7: Parallel execution implementation**

**Note**: `RoleResolver` does NOT execute requests. It returns a `Resolution` containing the backend IDs and strategy. The `Conductor` layer interprets the `Resolution` and executes actual backend requests. This keeps routing logic pure and testable.

The `Conductor`'s handling of `RoutingStrategy` in the `Resolution`:

- `First`: Invoke backends in order, return first successful response. Terminal errors short-circuit immediately.
- `Fallback`: Invoke backends in order; transient errors (429, 500, timeout) trigger the next backend; terminal errors (401, 400) short-circuit the chain immediately.
- `Parallel { min_success, timeout_secs }`: Fire all backends concurrently using `FuturesUnordered` + `AtomicUsize`. Return as soon as `min_success` backends succeed, cancelling remaining in-flight requests. If `timeout_secs` is set, wrap each invocation in `tokio::time::timeout`.

```rust
// Conductor interprets Resolution and dispatches requests
async fn execute_resolution(
    resolution: Resolution,
    cx: &mut Context<'_>,
) -> Result<Vec<QueryOutput>, AggregatedError> {
    match resolution.strategy {
        RoutingStrategy::First => { /* try each in order */ }
        RoutingStrategy::Fallback { timeout_secs } => { /* try in order with transient error handling */ }
        RoutingStrategy::Parallel { min_success, timeout_secs } => {
            // Fire all concurrently via FuturesUnordered
            // Track successes with AtomicUsize
            // Return when min_success succeed
            // Cancel remaining futures
        }
    }
}
```

---

## Code Structure

```
src/
  role/
    mod.rs          # RoleResolver, Resolution, RoutingStrategy, RoleConfig, TeamConfig
    strategies.rs    # First, Parallel, Fallback strategy implementations
  delegation.rs     # Refactor: RoleResolver as primary, Delegator as keyword fallback
  config.rs         # Add [roles], [teams], [defaults] sections
  cli/
    smart.rs        # Add --team, --explain flags
    team.rs         # Add --explain flag
    spawn.rs        # Add --team, --explain flags
```

---

## API/Interface Design

| Function/Method | Parameters | Returns | Description |
|-----------------|------------|---------|-------------|
| `RoleResolver::new(config: &Config)` | `&Config` | `Self` | Constructor from config |
| `RoleResolver::resolve(role, team_override, available)` | `&str`, `Option<&str>`, `&[BackendId]` | `Result<Resolution, RoleResolutionError>` | Two-tier resolve with optional CLI team override |
| `RoleResolver::validate(known: &[BackendId])` | `&[BackendId]` | `Vec<ValidationWarning>` | Config validation at load time |
| `Resolution::backend_ids()` | none | `Vec<BackendId>` | Backend list for caller |
| `Resolution::is_empty()` | none | `bool` | True if no backends available after filtering |
| `RoutingStrategy::default()` | none | `RoutingStrategy` | Defaults to `Fallback { timeout_secs: None }` |
| `RoleResolutionError` | enum | — | `RoleNotFound`, `NoBackendsAvailable`, `ValidationError` variants |

---

## Implementation Plan

### Phase 1: Core Types and Config

- [ ] Define `RoutingStrategy` enum in `src/role/mod.rs` with `First`, `Parallel { min_success, timeout_secs }`, `Fallback { timeout_secs }`
- [ ] Define `Resolution` struct in `src/role/mod.rs`
- [ ] Define `RoleResolutionError` enum with `RoleNotFound`, `NoBackendsAvailable`, `ValidationError` variants
- [ ] Define `RoleConfig` and `TeamConfig` structs in `src/role/mod.rs`
- [ ] Add `[roles]` and `[teams]` sections to `Config` in `src/config.rs` with `#[serde(default)]`
- [ ] Add `[defaults.team]` option
- [ ] Implement `RoleResolver::new()` from config
- [ ] Add config validation: reject `min_success` on `first`/`fallback`, validate bounds for `parallel`
- [ ] Write unit tests for `RoleResolver::new()` and config parsing

### Phase 2: Resolution Logic

- [ ] Implement `RoleResolver::resolve()` with two-tier lookup and optional `team_override`
- [ ] Implement backend availability filtering (skip disabled/unknown)
- [ ] Emit `ValidationWarning` for unknown backend references at load time
- [ ] Write unit tests for two-tier resolution: team override, global fallback, custom team roles
- [ ] Write unit tests for backend filtering and error variants

### Phase 3: Strategy Definitions (Conductor handles execution)

- [ ] Phase 3 is handled by existing Conductor layer - no strategy execution code needed in `RoleResolver`
- [ ] `Resolution` returned by `RoleResolver::resolve()` carries `RoutingStrategy`; Conductor interprets it
- [ ] Add integration notes to delegation.rs: how Conductor maps `RoutingStrategy` to existing async patterns

### Phase 4: CLI Integration

- [ ] Add `--team <name>` flag to `smart`, `team`, `spawn` commands
- [ ] Add `--explain` flag to print resolution (role, team, backends, strategy)
- [ ] Wire `RoleResolver` into command execution context (resolver instantiated once at startup)
- [ ] Write integration tests for CLI flags

### Phase 5: Delegator Integration and Migration

- [ ] Refactor `delegation.rs`: `RoleResolver` as primary, `Delegator` as keyword fallback
- [ ] Wire `RoutingStrategy` from `Resolution` into Conductor's existing execution paths
- [ ] Verify default config (no `[roles]`) produces identical behavior to current `Delegator::new()`
- [ ] Write a test that runs existing `delegation.rs` test inputs through `RoleResolver::from_config(Config::default())` and verifies same backend recommendations
- [ ] Add migration note: hardcoded profiles from `delegation.rs` will move to default config in a follow-up

### Phase 6: Testing & Validation

- [ ] All existing `cargo test` passes
- [ ] `cargo clippy` clean
- [ ] Manual testing: verify role routing works with custom `lok.toml`

---

## Constraints

**Must**:
- Preserve backward compatibility: default config must produce identical behavior to current `Delegator::new()`
- `RoutingStrategy` must be a distinct concept from `ConsensusStrategy` - they compose, they do not merge
- Backend references in roles are validated at config load time; unknown backends produce warnings, not errors
- `min_success` for parallel must be rejected for `first` and `fallback` strategies at validation time
- All new `Config` fields must use `#[serde(default)]` to preserve backward compatibility with existing `lok.toml`

**Must-not**:
- Must not block the tokio runtime with sync operations
- Must not break the `Backend` trait interface
- Must not add telemetry or analytics dependencies
- Must not break the existing CLI interface contract

**Prefer**:
- Prefer existing error types over new ones
- Prefer `HashMap` config patterns already used in the codebase
- Prefer `FuturesUnordered` for parallel execution (already in use for `min_deps_success`)

**Escalate when**:
- If `Backend` trait changes are needed to support role routing
- If the `ConsensusStrategy` enum needs new variants for role routing
- If `Delegator` keyword classification cannot be preserved as fallback

---

## Acceptance Criteria

- [ ] `[roles.code_review]` and `[roles.security_audit]` config sections parse correctly from TOML
- [ ] `[teams.frontend.roles.code_review]` team override config parses and resolves before global role
- [ ] `RoleResolver::resolve("code_review", None, &["claude", "gemini"])` returns `Ok(Resolution { backends: ["claude", "gemini"], strategy: Fallback, ... })`
- [ ] `RoleResolver::resolve("code_review", Some("frontend"), &["claude"])` uses team override backends
- [ ] `RoleResolver::resolve("unknown_role", None, &["claude"])` returns `Err(RoleResolutionError::RoleNotFound)`
- [ ] `RoleResolver::resolve("role", None, &[])` returns `Err(RoleResolutionError::NoBackendsAvailable)` when all backends filtered out
- [ ] `RoutingStrategy::Parallel { min_success: 1, timeout_secs: Some(30) }` includes timeout in resolution
- [ ] `min_success = 5` on a role with 2 backends produces `ValidationError` during config validation
- [ ] `min_success` on `first` or `fallback` strategy produces `ValidationError` during config validation
- [ ] `cargo test role` passes with 0 failures
- [ ] `cargo test delegation` passes with 0 failures
- [ ] `cargo clippy` reports no warnings

**Verification method**: Run `cargo test role delegation && cargo clippy`

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | Parse `[roles.code_review]` from valid TOML | `RoleConfig { backends: [...], strategy: Fallback { timeout_secs: None } }` | Add test with inline TOML, parse, assert |
| 2 | Team override resolves before global role | team override backends returned | Test with both team + global config |
| 3 | `Parallel` strategy fires all backends | All backend futures created | Mock backend tracker, verify all fired |
| 4 | `Fallback` stops at first success | Second backend never invoked | Mock backend, fail first, succeed second, verify second not called |
| 5 | Default config matches current delegation | Same backend as `Delegator::new()` | Run existing delegation test inputs through resolver with default config |
| 6 | Unknown backend produces warning | `ValidationWarning` emitted at load time | Configure role with unknown backend, assert warning emitted |
| 7 | `min_success` bounds validated | `RoleResolutionError::ValidationError` returned | Test `min_success = 0`, `min_success = 99` on 2-backend role |
| 8 | `RoleNotFound` returned for unknown role | Error variant correctly constructed | Call resolve with non-existent role, assert `RoleNotFound` variant |
| 9 | `NoBackendsAvailable` when all filtered | Error variant returned when backends list empty | Mock available backends that exclude all role backends |

**Edge cases to cover**:
- Role configured with backends that are all disabled: resolver returns `NoBackendsAvailable` error; caller falls back to `Delegator`
- Team override defines a role not in global config: team override is used standalone (valid - teams can define custom roles)
- `parallel` strategy with `min_success = backends.len()` (all must succeed): behaves like `join_all` with cancellation on full success
- User has `[roles]` config but no `[teams]`: two-tier resolve skips team lookup, uses global directly
- `Fallback` with terminal error (401): short-circuits immediately without trying remaining backends
- `Fallback` with transient error (429): tries next backend in order
- `--team` flag provided with no `[teams.<name>]` config: `RoleNotFound` error returned
- Unknown backend in role config: `ValidationWarning` emitted at load time; backend skipped during resolution

---

## Testing Strategy

- **Unit Tests**: Test `RoleResolver` methods directly with mocked `Config`. Test each `RoutingStrategy` implementation with fake async backends. Test config validation edge cases.
- **Integration Tests**: Test CLI integration by running commands with test `lok.toml` files in temp dirs. Verify `--explain` output contains expected role/team/backend info.
- **Manual Testing**: Load a `lok.toml` with custom roles, run `lok smart "code review" --explain --team frontend`, verify correct backend selected.

---

## Open Questions

~~Should `--team` flag be additive with `[defaults.team]` config (flag overrides config) or exclusive?~~
**Resolved**: `--team` CLI flag overrides `[defaults.team]` for a single call. The flag takes precedence. This is resolved above in `RoleResolver::resolve()` semantics.

~~Does `RoleResolver::resolve()` need to handle the case where no backends are available, or is that caller responsibility?~~
**Resolved**: `RoleResolver::resolve()` returns `Err(RoleResolutionError::NoBackendsAvailable)` when all backends are filtered out. The caller (Conductor) decides what to do (fall back to Delegator or return error).

~~What is the behavior when a role references a backend that exists in user config but not project config?~~
**Resolved**: Unknown backends produce `ValidationWarning` at config load time, not errors. During resolution, unknown/disabled backends are skipped.

- [ ] What is the default timeout value for `timeout_secs` when not specified? (Global default in Config, or infinite?)
- [ ] Should `RoleResolver` be instantiated once at startup or created per-call? (Affects whether `default_team` is mutable via CLI)

---

## References

- [Linear Task CLO-212](https://linear.app/cloud-ai/issue/CLO-212)
- [PRD: llm-mux port](docs/prds/prd-llm-mux-port.md)
- [Discovery Report](docs/prds/discovery-report-2026-04-12-clo-212.md)
- [Dependency: CLO-203 (Config merge layer)](https://linear.app/cloud-ai/issue/CLO-203)