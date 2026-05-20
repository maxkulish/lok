# Lessons: CLO-383 FR-22 apply_edits sandbox defaulting

Durable rules from implementing FR-22: steps with `apply_edits=true` default Codex/Gemini sandbox to workspace-write / auto-edit.

---

## L1 — Pre-capture Copy/Clone fields before `tokio::spawn(async move)`

**Source incident:** CLO-383 implementation. The initial ST2 wired `step.sandbox` and `step.apply_edits` into `StepContext` inside a `tokio::spawn(async move { ... })` closure. The `Step` struct is not `'static`, so borrowing `&Step` inside the closure produced a lifetime error when the compiled closure was passed to `tokio::spawn`, which requires `'static` captures.

**Rule:** Rust async closures passed to `tokio::spawn` (or any executor) must not contain references shorter than `'static`. Fields from referenced structs must be **pre-captured** as `Copy`/`Clone`/`String` values outside the `async move` block, then moved into the closure. Do not leave raw field accesses (e.g., `step.sandbox`) inside the block.

**How to apply:**
```rust
// CORRECT: pre-capture before async move
let step_sandbox = step.sandbox; // Option<SandboxMode> is Copy
let step_apply_edits = step.apply_edits; // bool is Copy
async move {
    let ctx = StepContext {
        sandbox: step_sandbox,      // Value, not borrow
        apply_edits: step_apply_edits,
        ...
    };
}
```

```rust
// WRONG: borrows &Step which is not 'static
async move {
    let ctx = StepContext {
        sandbox: step.sandbox,      // Compile error: '1 must outlive 'static
        apply_edits: step.apply_edits,
        ...
    };
}
```

This applies whenever any new field is threaded through an existing `tokio::spawn` path in `workflow.rs`. Any field added to `PreparedStep` or used directly in the `async move` block must be pre-captured alongside the existing `backend_name`, `step_name`, `timeout_duration`, etc.

---

## L2 — New function parameters landed on main create merge-conflict hotspots for test call sites

**Source incident:** CLO-383 branch diverged from main before CLO-380 (FR-3b: output-last-message) landed. CLO-380 added a 5th parameter `output_last_message_path: Option<&Path>` to `CodexBackend::build_argv_prefix`. When CLO-383 merged, every existing test call site of `build_argv_prefix` — and every new `apply_edits` test — had a three-way conflict: our branch called with 4 args (`base_args`, `sandbox`, `apply_edits`, `model`), common ancestor had 3 (`base_args`, `sandbox`, `model`), main had 4 (`base_args`, `sandbox`, `model`, `output_last_message_path`).

**Rule:** When a function signature in `src/backend/*.rs` gains a parameter on `main` while a feature branch also modifies that function, resolving the merge requires updating **all** call sites — existing tests and new tests alike. The resolution is not a simple binary pick of HEAD vs. current-branch version; it requires synthesizing a new 5-argument call pattern across every test.

**How to apply:**
1. Identify the final signature on main.
2. Scan the entire file for function-call occurrences (not just the one in `query()`).
3. Update **every** call site — existing backend tests, new tests from the branch, and the `query()` path itself — to match the new parameter count.
4. Prefer `cargo fmt` to catch multi-line call-site formatting issues, then `cargo test --no-run` to verify compilation.
5. Record the conflict resolution time in the merge commit so future tasks branching near the same code know to expect this.

---

## L3 — The pre-PR validation gate caught a silent failure path that cargo test alone missed

**Source incident:** The initial CLO-383 implementation passed `cargo test` (555 green), but the validation gate (Codex reviewer) found that `WorkflowEditRequester::request_retry` built a `StepContext` from scratch instead of inheriting the step's `sandbox` and `apply_edits`. On a verify/retry failure, the second-chance LLM call would silently revert to `read-only` or `None`, losing the `apply_edits=true` intent. This is exactly the silent-failure pattern FR-22 is designed to eliminate.

**Rule:** Passing unit tests are necessary but not sufficient for correctness. Backend unit tests test the *builder* in isolation; they do not observe that the *caller* passes the builder the right inputs. A reviewer that inspects the full call graph (design doc → implementation plan → diff) can catch regressions in data flow that unit tests miss.

**How to apply:**
- After the initial pre-merge gate passes (fmt + clippy + test), run the `pre-pr-validation` workflow before opening the PR.
- Treat reviewer findings in the MEDIUM/HIGH categories as pre-PR blockers, not PR-review-cycle suggestions.
- When validation surfaces a gap, add a regression test that exercises the exact data-flow path (not just the isolated builder).
