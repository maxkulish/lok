# PRD Discovery Report: CLO-212 - Configurable Role Routing

**Date**: 2026-04-12
**PRD**: docs/prds/prd-llm-mux-port.md (Phase 6)
**PRD Score (baseline)**: 69% - Needs Iteration
**Discovery Debt**: 4 killer assumptions
**Verdict**: Ready to Build (with design refinements)

---

## Prior Art Summary

| Source | Type | Relevance | Implication |
|--------|------|-----------|-------------|
| TensorZero Gateway | Rust+TOML+fallback | High | Closest analog for TOML config with sequential routing; lacks roles, parallel-with-threshold, two-tier |
| API7/APISIX priority routing | Pattern | High | Priority groups with automatic failover maps directly to `fallback` strategy |
| AWS Hedging pattern | Pattern | High | Validates `parallel` strategy: fire to all backends, take fastest successful response |
| K8s node affinity (required vs preferred) | Pattern | Medium | Maps to two-tier resolution: required (team override) vs preferred (global fallback) |
| LiteLLM routing | Product | Medium | Order+fallback with RBAC; lacks TOML, parallel strategy, min_success, role-based backend lists |
| RouteLLM cost-threshold | Research | Low | Threshold concept applicable but ML-based routing is out of scope |
| FaaS orchestration patterns | Pattern | Low | Formal vocabulary for strategies: XOR-split (first), AND-split (parallel), sequential (fallback) |

**Key Gap**: No existing system combines TOML-based role definitions + three execution strategies + min_success + two-tier resolution. Lok occupies a unique niche.

---

## Persona Review Summary

### Consensus Concerns (raised by 2+ personas)

1. **Strategy semantics conflict with ConsensusStrategy** - raised by Engineer, Strategist
   - Existing `ConsensusStrategy` (First, Synthesis, Vote, WeightedVote) controls response selection
   - New routing strategies (First, Parallel, Fallback) control backend selection
   - These are different concerns being conflated
   - **Fix**: Name new concept `RoutingStrategy` explicitly. Routing decides which backends to invoke; consensus decides how to combine their outputs. They compose.

2. **Team override TOML schema is ambiguous** - raised by Engineer, Adversarial
   - `[teams.frontend]` with `roles.code_review = ["claude"]` creates inconsistent shapes vs `[roles.code_review]`
   - Merge semantics unclear: does team override replace entire role entry or just backends?
   - **Fix**: Use inline tables `[teams.frontend.roles.code_review]` for consistent structure. Define team override as full replacement of the role entry for that team.

3. **Integration with existing commands is undefined** - raised by Strategist, User Advocate
   - PRD doesn't specify how role routing integrates with `smart`, `team`, `spawn` commands
   - No `--team` flag, no `--explain` flag, no `lok roles list` command
   - **Fix**: Define CLI surface. Add `--team <name>` flag and `--explain` to relevant commands. `RoleResolver` returns a `Resolution` struct that can be displayed.

4. **`min_success` default and validation is undefined** - raised by Adversarial, Engineer
   - No default value specified for `parallel` strategy
   - Should `min_success` be rejected for `first` and `fallback` strategies?
   - **Fix**: Default `min_success` to `backends.len()` for parallel. Reject `min_success` for `first` and `fallback` at config validation time.

### Unique Perspectives

- **Engineer**: `deny_unknown_fields` on Config needs careful handling when adding new sections. Must add `#[serde(default)]` to new fields. Also, four call sites construct `Delegator::new()` independently - all must be updated.

- **User Advocate**: No visibility into routing decisions. Users need `lok roles list` and `--explain` flag. Also, `backends` references in roles should be validated against `[backends]` map at config load time.

- **Strategist**: Consider splitting into CLO-212a (config schema + role resolver) and CLO-212b (integration with execution strategies). The MVP delivers config-driven routing; execution strategy integration is separate value.

- **Adversarial**: `strategy = "parallel"` is ambiguous - does it mean concurrent requests or concurrent processes? Must define execution model. Also, disabled backends referenced in roles need availability filtering in the resolver.

---

## Assumption Map

### Killer Assumptions (validate before building)

| # | Assumption | Importance | Certainty | Suggested Validation |
|---|-----------|-----------|-----------|---------------------|
| 1 | Role routing can be cleanly separated from consensus/response selection | 5 | 3 | Write the `RoutingStrategy` enum and `Resolution` struct before implementation. Verify that `smart`, `team`, and `spawn` commands can consume a `Resolution` without changing `ConsensusStrategy`. |
| 2 | Default config with no `[roles]` section produces identical behavior to current `Delegator::new()` | 5 | 3 | Write a test that runs the existing `delegation.rs` test inputs through `RoleResolver::from_config(Config::default())` and verifies same primary backend recommendations. |
| 3 | Two-tier resolution (team -> global) has a clear activation mechanism | 4 | 2 | Define `--team <name>` CLI flag and `[defaults.team]` config. Verify that project-level config can set a default team while CLI flag overrides it. |
| 4 | `parallel` strategy with `min_success` can use existing async patterns (FuturesUnordered + AtomicUsize) without major architectural changes | 4 | 4 | Prototype the quorum execution using `FuturesUnordered`. Verify it integrates with the existing `Backend` trait and `QueryOutput`. |

### Foundation Assumptions (high importance, high certainty)

| # | Assumption | Importance | Certainty |
|---|-----------|-----------|-----------|
| 5 | Config merge system (CLO-203) correctly deep-merges `[roles]` HashMaps | 5 | 5 |
| 6 | `#[serde(default)]` on new Config fields preserves backward compatibility | 5 | 5 |
| 7 | Existing `ConsensusStrategy` does not need modification for role routing | 4 | 4 |

### Discovery Debt Score: 4 / 11 total assumptions

**Rating**: Low-to-moderate debt. The 4 killer assumptions are addressable through design decisions and targeted tests, not fundamental research.

---

## Stress Test Results

1. **Failure Scenario**: User configures `strategy = "parallel"` with `min_success = 3` and 3 backends, but 2 backends are slow. The system waits for all 3 to respond, effectively becoming a blocking `join_all`. The timeout kicks in at the global level, killing the entire operation.
   - **Missing in PRD**: Per-role timeout and early termination semantics for parallel strategy
   - **Fix**: Define that parallel strategy returns as soon as `min_success` backends respond, canceling remaining in-flight requests. Add optional `timeout_secs` per role.

2. **Failure Scenario**: A project's `lok.toml` defines `[roles.code_review]` referencing a backend that only exists in a teammate's user-level config. Another teammate without that backend in their user config gets a config validation error because the backend name is unknown.
   - **Missing in PRD**: Backend availability is context-dependent (user-level vs project-level)
   - **Fix**: `RoleResolver::resolve()` should accept `available_backends` and filter. Unknown backends produce a warning, not an error. Disabled backends are skipped.

3. **Failure Scenario**: The `[teams]` concept is added but never activated because there's no CLI flag or config default to select a team. Users see the config section but can't use it.
   - **Missing in PRD**: Team activation mechanism
   - **Fix**: Add `--team <name>` CLI flag on relevant commands and `[defaults.team]` config option. If no team is specified, only `[roles]` is used.

---

## Recommended PRD Changes (prioritized)

1. **[MUST FIX]**: Rename routing concept from overloading "strategy" to `RoutingStrategy` enum. Keep `ConsensusStrategy` for response selection. They compose: routing selects backends, consensus combines outputs.

2. **[MUST FIX]**: Define TOML schema for team overrides as `[teams.<name>.roles.<role_name>]` inline tables, not flat string lists. This allows future extension (strategy override at team level) and makes the structure regular.

3. **[MUST FIX]**: Define CLI surface: `--team <name>` flag on `smart`, `team`, `spawn` commands. Add `defaults.team` config option. Add `--explain` flag for routing visibility.

4. **[MUST FIX]**: Define `min_success` defaults and validation rules. Default to `backends.len()` for parallel. Reject for `first` and `fallback`. Define early termination for parallel when threshold met.

5. **[SHOULD FIX]**: Add `RoleResolver::resolve(available_backends)` that filters by availability and returns a `Resolution` struct. Unknown backends produce warnings, disabled backends are skipped.

6. **[SHOULD FIX]**: Rewrite acceptance criteria as observable behaviors, not implementation checkboxes. Example: "When I run `lok smart "security audit"` with a custom `[roles.security_audit]` config, the configured backend is used."

7. **[COULD FIX]**: Add `roles.fallback = true|false` config option to allow disabling keyword classification for users who want only explicit role matching.

8. **[COULD FIX]**: Add `strategy = "default"` as a valid value that uses the built-in `Delegator` logic, providing an escape hatch for experimentation.

---

## Blind Spots Identified

- **How does `Delegator` (keyword classification) interact with `RoleResolver`?** The PRD says "keep keyword classification as fallback" but doesn't specify the resolution order: does the resolver check configured roles first, then fall back to keyword classification? Or does keyword classification run in parallel and the results are merged?
- **What happens to `Conductor::build_system_prompt()` hardcoded backend descriptions?** The PRD doesn't address unifying these with role config.
- **Parallel execution requires async runtime.** The current `smart` and `suggest` commands appear to be synchronous. Does role routing require making these async?
- **`TaskConfig.backends` already exists in config.** How does `[roles.code_review].backends` relate to `[tasks.hunt].backends`? Are they different concepts (task-level vs role-level) or should they be unified?

---

## Recommended Approach

**Approach: Config-Driven Role Routing with Clean Separation** (Approach 1 from PRD, refined)

The PRD's proposed approach is sound. The key refinements from discovery:

1. **Separate routing from consensus**: `RoutingStrategy` (backend selection) is distinct from `ConsensusStrategy` (response combination). The `RoleResolver` returns a `Resolution` struct that consumers interpret.

2. **Extend, don't replace**: `Delegator` becomes the fallback when no `[roles]` config exists. `RoleResolver` wraps it, not replaces it. This guarantees backward compatibility.

3. **Use existing patterns**: `HashMap<String, RoleConfig>` for roles config mirrors `HashMap<String, BackendConfig>` for backends. Enum dispatch for strategies mirrors `ConsensusStrategy`. `FuturesUnordered` for parallel mirrors `min_deps_success`.

4. **Define team activation via CLI**: `--team <name>` flag + `[defaults.team]` config. Team overrides are project-level (in `./lok.toml`), not user-level.

This approach is recommended because it:
- Extends the existing config merge system without changes
- Uses established Rust patterns (enum dispatch, HashMap config, serde defaults)
- Preserves backward compatibility by wrapping, not replacing
- Delivers incremental value: config-driven routing first, execution strategy integration later

---

## Research Reading List

- TensorZero TOML config: https://github.com/tensorzero/tensorzero (closest Rust+TOML+fallback analog)
- API7 multi-LLM routing: https://docs.api7.ai/api7-gateway/ai-gateway/use-cases/multi-llm-routing-and-fallback (priority groups pattern)
- AWS avoiding fallback: https://aws.amazon.com/builders-library/avoiding-fallback-in-distributed-systems/ (validates parallel/hedging strategy)
- K8s node affinity: https://kubernetes.io/docs/concepts/scheduling-eviction/assign-pod-node/ (required vs preferred pattern)