# Lessons: CLO-589

## L1 - Validate API boundary ownership at symbol granularity before pre-PR gates

### Source incident

`CLO-589` initial pre-PR synthesis passed high-level bucket coverage but Codex flagged missing symbol-level allocation for backend boundary ownership (e.g., cache/global-state symbols and per-type owner decisions).

### Rule

When the task defines a library boundary, include an explicit per-item owner decision table for every public symbol that is intended for review and handoff.

### How to apply

In design/plan artifacts and validation checks, enumerate each public symbol with: current source path, owner (`library`/`binary`/`deferred`/`test-only`), and rationale. Do not rely only on module-level buckets.

## L2 - Keep untested extraction feasibility as a formal handoff assumption for the follow-up task

### Source incident

`CLO-589` deferred actual `[lib]` extraction feasibility to `CLO-590`; this task only froze the ADR shape and did not validate crate/API build/test behavior.

### Rule

If feasibility is deferred to a follow-up task, record it as an explicit assumption and hand it to the blocker chain rather than letting it be implicitly inferred as already validated.

### How to apply

Keep `CLO-590` scoped to executable validation (`cargo build --bins --lib`, Bedrock feature paths, and CLI/API usage) and treat the ADR as the source-of-truth ownership contract, not a completion proof.
