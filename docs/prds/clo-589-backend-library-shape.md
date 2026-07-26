# PRD: Backend module exposure shape for lok (CLO-589)

## Problem

`lok` currently ships `src/backend` only as part of binary-only implementation inside the root crate. Multiple downstream consumers now need to share this abstraction:

- this repository's internal roadmap includes a dedicated extraction ticket for ADR clarity,
- and an external `remem-ai` fork depends on upstreaming a compatible LLM backend interface for execution and token accounting.

### Impact

Without a recorded shape decision, teams cannot safely start extraction work, and they risk making incompatible changes (e.g., moving too much of the binary config surface into a reusable library or splitting it into a second crate too early).

### Desired outcome

Decide and document, before any code moves, the package shape for exposing backend abstraction:

- whether it is a `[lib]` target in `lokomotiv`, or
- whether `lok-backend` becomes a separate workspace crate.

The outcome must clearly state:

- the exact boundary of library vs binary ownership,
- why `Config` stays in the binary,
- what this means for versioning and release flow,
- and how `bedrock` is exposed to consumers.

### Scope

- One-time ADR record in `docs/adrs/` and index update.
- No code extraction in this ticket; this task records the decision.
- No consumer-facing API changes in this ticket.

### Acceptance criteria

- [ ] ADR exists in `docs/adrs/` and index file includes it.
- [ ] Chosen shape and rejected alternative are explicitly documented with rationale.
- [ ] Boundaries are recorded item-by-item (what moves to library vs remains binary-owned).
- [ ] `Config`/`backend`-shaped orchestration coupling is explained.
- [ ] Versioning and `crates.io` publishing decision is explicit.
- [ ] `bedrock` feature exposure to consumers is explicit.

### References

See Linear issue body for internal coupling analysis and external context, including:
- `CLO-589` and linked external handoff/design/proposal notes,
- issue `CLO-590` dependencies.
