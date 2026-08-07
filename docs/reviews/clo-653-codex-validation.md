# Pre-PR validation: clo-653

**Reviewer**: Codex (gpt-5.6-sol)
**Validated**: 2026-08-07
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

The production implementation appears correct, but the branch does not satisfy the design's mandatory proof obligations. Reviewed `a98b4b1` against `main`; the uncommitted workflow-status edit was excluded.

## Findings

- **MEDIUM — Required evaluation tests are marked complete but absent.** The plan marks Tests 3–10 and edge cases complete ([plan](/Users/mk/Code/orchestrator/lok--fix-clo-653-backend-cache/docs/plans/clo-653-backend-cache-key.md:128)), but there is no observable retry-attempt test, no end-to-end ambiguous Ollama `Workflow::validate` test, and no ambient-credential sharing test. The branch explicitly declines the latter ([engine.rs](/Users/mk/Code/orchestrator/lok--fix-clo-653-backend-cache/src/engine.rs:2182)). Key inequality and helper tests do not prove the specified runtime behavior.

- **MEDIUM — The cache-clear warmup test is vacuous.** `warmup_writeback_survives_a_cache_clear` constructs and clears the cache, then calls `warmup_backends`, which performs a fresh construction after the clear ([engine.rs](/Users/mk/Code/orchestrator/lok--fix-clo-653-backend-cache/src/engine.rs:2220)). It never clears between warmup's own `create_backend` call and write-back, so it does not test the edge case required by the design.

- **MEDIUM — Full validation is not proven for current HEAD.** The committed validation record covers `0a4e083` and `f17cb8a`, while `84d3096` adds tests and `a98b4b1` changes production code afterward. The worktree's uncommitted status update says validation remains in progress ([workflow status](/Users/mk/Code/orchestrator/lok--fix-clo-653-backend-cache/docs/status/clo-653-workflow.yaml:237)). Therefore the acceptance criterion requiring every Phase 6 command to pass on the final revision remains unverified.

- **LOW — The race test deviates from the specified public-path proof.** `losing_racer_does_not_erase_a_probe` deterministically exercises the extracted insertion helper instead of racing two `create_backend` calls ([mod.rs](/Users/mk/Code/orchestrator/lok--fix-clo-653-backend-cache/src/backend/mod.rs:1266)). It is discriminative and the production caller uses that helper, but the plan should record this intentional substitution rather than claim the two-thread test was implemented.

No correctness defect was found in the main runtime wiring, and no new hardcoded secrets or unsafe process-spawning behavior was introduced.

## Missing Items

- Observable retry behavior for identical `BackendConfig` values with different retry policies.
- Exactly one `health.is_some()` entry per configured backend after warmup.
- End-to-end ambiguous Ollama model validation using two healthy inventories.
- A non-vacuous cache-clear-between-construction-and-write-back test.
- The documented same-key/different-credential behavior test.
- Full Phase 6 validation on exact current HEAD.

## Recommendations

- Add a 500-returning recording-server test asserting different request counts for different retry policies.
- Seed two Ollama cache identities with conflicting inventories and call `Workflow::validate` in both insertion orders.
- Extract the warmup write-back into a small helper or add a test barrier so the cache can be cleared after construction but before insertion.
- Use a subprocess for the ambient-environment test to avoid racing process-global environment mutation.
- Run the complete Phase 6 matrix on `a98b4b1`, then commit the updated evidence and correct the completed checkboxes.

`git diff --check` and `cargo fmt --all -- --check` passed. Cargo-based tests could not run normally because the review sandbox forbids creating `target/debug/.cargo-lock`; read-only-safe prebuilt tests for ambiguous health and the public cache-key API passed.
