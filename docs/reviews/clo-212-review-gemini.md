# Design Review: CLO-212 - Role Routing Config

**Reviewed**: 2026-04-12
**Reviewer**: Gemini 3.1 Pro
**Design Document**: docs/design-docs/clo-212-role-routing-config.md
**Pipeline**: lok design-review

---

YOLO mode is enabled. All tool calls will be automatically approved.
YOLO mode is enabled. All tool calls will be automatically approved.
Here is my architectural review of the **CLO-212: Add Configurable Role Routing with [roles]/[teams] Config** design document.

### Verdict: APPROVE_WITH_SUGGESTIONS

The overall design is solid. Moving from hardcoded backend profiles to a configurable TOML-based two-tier lookup (global roles with team overrides) will significantly improve `lok`'s flexibility. The configuration schema is clean, idiomatic, and clearly addresses the findings from the discovery phase. 

However, there are a few architectural leaks around the separation of concerns (Routing vs. Execution) and blind spots regarding timeouts, error semantics, and API signatures that need to be addressed before implementation begins.

---

### Key Findings

#### 1. Architecture Quality: Separation of Concerns (Routing vs. Execution)
In the "Detailed Design" section, `RoutingStrategy` is defined as controlling *backend selection*. However, Step 7 and Phase 3 detail implementing `resolve_parallel` and executing `FuturesUnordered` *inside* the routing strategy. 
*   **Issue:** If `RoutingStrategy` executes the futures, it is acting as the Orchestrator/Conductor. Routing should ideally be a pure function that returns a `Resolution` (the *Plan*), and the `Conductor` or Orchestration layer should interpret this plan to execute the HTTP requests and manage the `FuturesUnordered` logic. This ensures routing remains easy to test without mocking async HTTP clients.

#### 2. Code Quality / Interfaces: Missing CLI Override Parameter
*   **Issue:** The API design in Step 4 defines `RoleResolver::resolve(&self, role: &str, available_backends: &[BackendId])`. However, Step 6 states that the CLI `--team <name>` flag is passed to the `RoleResolver`. 
*   **Fix:** The method signature must accept the CLI override: `pub fn resolve(&self, role: &str, team_override: Option<&str>, available_backends: &[BackendId]) -> Resolution`. The resolver must prefer `team_override` > `self.default_team` > `None`.

#### 3. Operational Readiness & Concurrency: Missing Timeouts
*   **Issue:** The execution strategies (`First`, `Parallel`, `Fallback`) do not specify timeout behavior. In a `Fallback` strategy, if the first backend hangs indefinitely, the fallback will never trigger.
*   **Fix:** Ensure the execution implementation wraps backend calls in a `tokio::time::timeout`. The timeout duration should either be a global default or configurable per-role.

#### 4. Blind Spots: Error Semantics in `Fallback`
*   **Issue:** When using `RoutingStrategy::Fallback`, stopping at the "first success" implies handling failures. If Backend A returns a `401 Unauthorized` or `400 Bad Request` (terminal errors), falling back to Backend B is likely a waste of time and tokens.
*   **Fix:** Clarify that `Fallback` should only trigger on *transient* errors (e.g., `429 Rate Limit`, `500 Server Error`, Timeouts). Terminal errors (auth failures, malformed requests) should short-circuit the fallback chain and bubble up immediately.

#### 5. Codebase Alignment: Unnecessary Constraint on Team Roles
*   **Issue:** Under edge cases, the design states: *"Team override references a role that doesn't exist in global config: team override is ignored..."* 
*   **Fix:** This is a poor developer experience. Teams should be able to define their own specific roles (e.g., `frontend_lint`) without having to pollute the global `[roles]` namespace with a dummy definition. A team override should be able to instantiate a completely new role.

---

### Prioritized Actionable Items

1. **Update API Signature:** Modify `RoleResolver::resolve` to accept `team_override: Option<&str>` so the `--team` CLI flag can be explicitly passed through the stack.
2. **Clarify Execution vs. Routing:** Update the design to clarify whether `RoleResolver` actually *executes* the futures, or if it simply returns the `RoutingStrategy` enum in the `Resolution` for the `Conductor` to execute. (Highly recommend the latter for a pure, testable routing layer).
3. **Define Fallback Error Semantics:** Explicitly state in the design that the `Fallback` execution strategy will only trigger on transient errors (timeouts, 5xx, 429) and will short-circuit on terminal errors (401, 400).
4. **Address Timeout Handling:** Add a note to the implementation plan ensuring async futures in `Parallel` and `Fallback` are wrapped in appropriate tokio timeouts to prevent blocking the orchestration flow.
5. **Remove Global Role Restriction:** Change the edge case behavior so that if a `[teams.<name>.roles.<custom>]` is defined, it is valid and resolvable even if `[roles.<custom>]` does not exist globally.
6. **Answer Open Questions:** 
    * *Team Flag vs Config:* `--team` CLI flag should take precedence over `[defaults.team]`.
    * *No Available Backends:* The Resolver should return the `Resolution` with an empty backend list or an explicit `NoBackendsAvailable` error, leaving the caller/Conductor to decide whether to fall back to the legacy `Delegator`.
