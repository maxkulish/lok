# Lessons: CLO-371 StepContext migration

Durable rules from the CLO-371 `Backend::query` -> `StepContext` migration.

---

## L1 - Carrying structs must populate planned fields at construction time

**Source incident:** CLO-371 pre-PR validation (`docs/reviews/clo-371-validation-synthesis.md`) found that Step-aware workflow calls used `StepContext::from_prompt(...)`, preserving the outer `tokio::time::timeout` behavior but leaving `StepContext.timeout` as `None` everywhere.

**Rule:** When introducing a carrying struct for future per-step behavior, migrating the signature is not enough. Every field promised by the design for the current slice must be populated at the call site, even if existing runtime behavior is still enforced elsewhere.

**How to apply:** Add a local helper near the owning config type (for CLO-371: `step_context(step, workflow, prompt, cwd)`) and have it map existing config/defaults into the carrying struct. Review grep gates should check not only that legacy positional calls are gone, but also that important fields are not dead plumbing.

---

## L2 - Public carrying-struct field types need explicit re-exports

**Source incident:** CLO-371 pre-PR validation found `StepContext` was re-exported, but supporting public field types (`Message`, `Role`, `SandboxMode`, `StepOptions`) were not. Downstream callers could see the struct but could not cleanly construct non-default values.

**Rule:** If a public struct exposes public fields whose types live in a private module, re-export those field types alongside the struct in the public API surface.

**How to apply:** For backend context additions, keep `src/backend/mod.rs` exports aligned with the design's public API section. If clippy flags re-exports as unused inside the binary crate, prefer a narrow `#[allow(unused_imports)]` on the public re-export over hiding the types from downstream users.

---

## L3 - Avoid duplicate source-of-truth calculations in context helpers

**Source incident:** CLO-372 PR review on `src/backend/mod.rs` found the non-Step `StepContext` helper calculated timeout from config while `run_query_with_config` recalculated the same effective timeout separately. The same review also flagged the one-year "no timeout" sentinel as an unnamed magic number.

**Rule:** When a context helper populates a field that callers also need for control flow, use the populated context field as the single source of truth. Name sentinel values such as "effectively no timeout" constants instead of repeating arithmetic literals.

**How to apply:** Build the context once, read `ctx.timeout` (or the equivalent carrying-field value) for downstream wrappers, and add a focused test that proves the helper-provided value reaches the caller. Use named constants like `NO_TIMEOUT_SECS` for sentinel policy values.
