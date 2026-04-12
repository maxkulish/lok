# Review Synthesis: clo-212

**Synthesized**: 2026-04-12
**Pipeline**: lok design-review
**Reviewers**: Gemini 3.1 Pro, Codex/Ollama (glm-5:cloud)

---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | OK | Produced full review in 65s |
| Ollama (Codex) | OK | Produced full review in 37s |
| Claude (fallback) | SKIPPED | Not needed - both external models succeeded |

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Missing timeout handling in Parallel strategy** - Both reviewers flagged that `RoutingStrategy::Parallel { min_success }` has no timeout semantics. If some backends hang, the strategy could block indefinitely. | MEDIUM |
| 2 | **Missing error type specification** - Neither review defines what error types `RoleResolver::resolve()` returns. Callers cannot distinguish "role not found" vs "all backends disabled" vs "validation failed". | MEDIUM |
| 3 | **Open questions unresolved** - The design leaves critical questions open, including `--team` flag semantics (additive vs exclusive with config) and who handles empty backend lists. | MEDIUM |

---

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | **Separation of routing vs execution** | RoutingStrategy should be a pure function returning a Resolution plan; Conductor/Orchestrator should execute futures, not the strategy itself | Did not flag this concern; focused on missing error types and timeouts |
| 2 | **Team override requiring global role first** | Teams should be able to define custom roles without polluting global namespace | Did not flag this; focused on cancellation and phase numbering |

---

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Fallback error semantics** - Gemini: `Fallback` strategy should only trigger on transient errors (429, 5xx, timeout) and short-circuit on terminal errors (401, 400) | Gemini | MEDIUM |
| 2 | **Cancellation semantics** - Ollama: Pending futures after `min_success` reached should be cancelled; `cancel_remaining_on_success: bool` parameter needed | Ollama | MEDIUM |
| 3 | **CLI override parameter** - Gemini: `RoleResolver::resolve` needs explicit `team_override: Option<&str>` parameter to wire `--team` flag through | Gemini | MEDIUM |
| 4 | **Phase plan disconnected** - Ollama: Architecture section and implementation phases don't reference each other clearly | Ollama | LOW |

---

## Consolidated Verdict

**Overall: APPROVE_WITH_SUGGESTIONS**

Both reviewers independently reached the same verdict. The design is fundamentally sound but has gaps around error handling, timeout/cancellation semantics, and a few unresolved design questions.

---

## Priority Actions

1. **[MEDIUM] Define `RoleResolutionError` enum** - Spec out error variants: `RoleNotFound`, `AllBackendsDisabled`, `ValidationError(String)`. Both reviewers flagged this gap.

2. **[MEDIUM] Add timeout semantics for Parallel strategy** - Either add `timeout: Option<Duration>` to `RoutingStrategy::Parallel` or document that callers must wrap with tokio timeouts.

3. **[MEDIUM] Resolve `--team` flag semantics** - Decide: does `--team` CLI flag override `[defaults.team]` config exclusively, or is flag additive? Core CLI contract question.

4. **[MEDIUM] Define Fallback error semantics** - Clarify which errors trigger fallback (transient: 429, 5xx, timeout) vs short-circuit (terminal: 401, 400).

5. **[MEDIUM] Document cancellation behavior** - When `min_success` is reached in Parallel, are remaining futures cancelled? Add `cancel_remaining_on_success: bool` parameter if yes.

6. **[MEDIUM] Update `RoleResolver::resolve` API signature** - Add `team_override: Option<&str>` parameter so CLI `--team` flag wires through cleanly.

7. **[LOW] Allow team-only roles** - Teams should define custom roles (e.g., `frontend_lint`) without requiring a global `[roles.<custom>]` entry first.

8. **[LOW] Align phase references** - Add phase labels to architecture diagram or remove phase numbering from implementation plan for consistency.

---

*This synthesis was automatically generated from multi-model AI review. Human judgment should be applied when interpreting these suggestions.*
