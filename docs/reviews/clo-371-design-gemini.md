# Design Review: clo-371-migrate-backendquery-to-stepcontext

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-05-18
**Pipeline**: manual gemini CLI invocation (lok design-review.toml had template bug)

---

**VERDICT: NEEDS_REVISION**

The design document is well-structured and thoroughly tracks the migration of a core trait. It correctly identifies the scope, the caller sites, and the fallback behaviors. However, there is a critical Rust compilation error in the proposed implementation and an architectural contradiction regarding the batching of breaking trait changes.

Here are the key findings and prioritized actionable items:

### 1. [CRITICAL] Compilation Failure with `&Default::default()`
**Finding:** In Section 3.5, the `step_context` helper constructs the context using `options: &Default::default()`. Because `StepOptions` is a type alias for a `HashMap`, this creates a temporary `HashMap`, borrows it, and then drops the temporary at the end of the `step_context` function. This will immediately fail to compile with **E0515 (cannot return reference to temporary value)**.
**Action:**
- Change the `options` field in `StepContext` to be an `Option`: `pub options: Option<&'a StepOptions>`.
- In the `step_context` helper, initialize it safely with `options: None`.
- *(Alternative)*: If you must require a reference, you will need to define a `static EMPTY_OPTIONS: std::sync::LazyLock<StepOptions> = ...` and return a reference to that, but making it an `Option` matches your `schema` and `sandbox` fields and is much more idiomatic.

### 2. [ARCHITECTURE] Contradiction in Breaking Trait Changes (Q2)
**Finding:** Section 1 explicitly states that the primary motivation for grouping these changes is that "doing it once is cheaper than three sequential breakages." However, the resolution to Q2 states: *"Changing it to HealthStatus later would be a second trait break."* Accepting a second trait break defeats the purpose of batching the trait changes now.
**Action:**
- Introduce a placeholder `HealthStatus` struct in this PR (e.g., an empty struct `pub struct HealthStatus;` in `src/backend/context.rs`).
- Have `async fn health_check(&self) -> Result<HealthStatus, BackendError>` return this struct now. When FR-9/9a lands, you can simply add fields to `HealthStatus` without breaking the `Backend` trait signature again.

### 3. [CODE QUALITY] Over-Constrained Lifetimes in Helper
**Finding:** The `step_context` helper signature ties all input references to the exact same lifetime `'a`:
```rust
fn step_context<'a>(step: &'a Step, workflow: &'a Workflow, prompt: &'a str, cwd: &'a Path)
```
**Action:** While Rust's borrow checker will usually shrink these lifetimes to the shortest common lifetime automatically (which works because `StepContext` just borrows them), it can sometimes cause inference friction if callers pass temporaries (like a formatted `prompt` `String`) alongside long-lived `Workflow` references. If you encounter compiler friction here, consider letting the parameters have anonymous lifetimes (`&Step`, `&Workflow`) and explicitly linking only the ones `StepContext` actually retains (`prompt` and `cwd`) to `'a`.

### 4. [OPERATIONAL READINESS] Mock Implementations
**Finding:** The migration plan correctly targets the 5 concrete backends and `RetryExecutor`, but the test plan misses the update to mock implementations.
**Action:** Ensure that any `MockBackend` or dummy backends used in `tests/integration.rs` or `tests/workflows/` are explicitly added to the migration sweep in Section 7.1, as they will also fail to compile when the trait changes.
