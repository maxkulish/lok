# Spec Review: clo-591

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-07-31
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is exceptionally clear, complete, and accurate. It defines exact source locations, stream outputs (stdout/stderr), and details the dependency tree impacts of binary dependencies on the library target. It goes beyond the initial Linear ticket description by identifying and incorporating other silent CLI/console print statements (such as stdout-contaminating sandbox warnings and verbose Claude CLI diagnostics) to ensure a fully clean library boundary.

## 2. Acceptance Criteria Review
**Strong**: 
- Highly specific and measurable criteria for compile-time dependency exclusion (**AC1**, **AC3**) and binary-target build guarantees (**AC8**).
- Concrete verification of complete output silence (**AC2**) which provides an excellent testability gate.
- Systematic alignment and resolution of all ADR divergences (**AC5**, **AC6**) and architectural cleanup of `StepContext` (**AC7**).

**Gaps**:
- **AC1** only explicitly checks for the absence of `indicatif`, `colored`, and `clap` in the dependency tree. It should expand this list to verify that other binary-only dependencies feature-gated in sub-task 1 (`chrono`, `dirs`, `futures`, `minijinja`, `sha2`) are also cleanly excluded when compiled with `--no-default-features`.
- No acceptance criterion governs CLI output parity. Replacing explicit console printing with logging runs a risk of visual regression on the CLI side (e.g., verbose default logger prefixes/timestamps). An additional criterion is needed: *"The CLI terminal output retains exact layout and visual presentation parity (concise warning prefixes, yellow colors, and custom retry glyphs) without verbose logger prefixes or timestamps."*

## 3. Constraints Check
**Aligned**:
- Excellent choice of standard, lightweight `log` facade instead of heavy `tracing` machinery.
- Retaining `default = ["cli"]` and using `required-features = ["cli"]` on `[[bin]]` targets perfectly aligns with standard Rust practices to feature-gate binary dependencies.
- Restricting `create_backend` signatures and preserving `BACKEND_CACHE` location rules are accurate and prevent unsafe refactoring sprawl.

**Concerns**:
- **Missing CLI Logger Formatter Constraint**: To prevent default log output verbosity (timestamps, modules, levels) from cluttering standard `lok` CLI runs, a new **Must** constraint is required: *"Must implement a custom formatter for the binary logger in `main.rs` to replicate original colors/style and suppress default logger headers (timestamp, target name, level prefixes) for standard CLI execution."*

## 4. Decomposition Quality
**Well-scoped**:
- Sub-tasks are highly logical, independent where possible, and well-scoped to <2 hours of effort.
- Dependency orderings are correctly mapped (routing library warnings through `log` must precede making `colored` optional).

**Issues**:
- Sub-task 5 ("Classify public API") should explicitly include demoting internal-use-only `pub` items (e.g., `is_cache_entry_fresh`, `parse_health_cache_ttl`, `resolve_health_cache_ttl`, `HEALTH_TTL_LOGGED`, and cache structures) to `pub(crate)` visibility to ensure optimal encapsulation.
- Sub-task 3 needs to explicitly require configuring the custom formatter for the binary-side `env_logger`.

## 5. Evaluation Coverage
**Covered**:
- Extensive and highly realistic evaluation table testing across compilation flags, features, and streams.
- The inclusion of an explicit edge case asserting that custom consumer loggers still receive library warnings is highly polished.

**Gaps**:
- **Thread-safe Output Capturing**: Standard Rust tests run in parallel. Capturing stdout/stderr in `tests/backend_public_api.rs` (Tests 5/6/7) can be flaky due to test runner parallel output. The evaluation approach should specify serializing these tests (e.g., using a serial lock) or invoking them in a separate subprocess to ensure clean, isolated stream capturing.
- Lacking verification that the build is free of Clippy warnings across both feature sets.

## 6. Codebase Alignment
**Violations**:
- None. The specification elegantly aligns with the codebase layout and Trait/Error patterns.

**Alignment**:
- Follows the established `Backend` trait interface.
- Respects the existing single-package architecture choice.
- Resolves the exact six ADR divergence rows to synchronize the code with the design contract.
- Replaces Tokio leaky lock types with an opaque `TestLockGuard` to maintain dependency isolation.

## 7. Blind Spots
1. **Public API Leakage (Visibility)**: Multiple variables and helpers in `src/backend/mod.rs` (such as `is_cache_entry_fresh`, `parse_health_cache_ttl`, `HEALTH_TTL_LOGGED`) are marked `pub`. Since these are only used internally by the binary side of the crate (`engine.rs`), they should be demoted to `pub(crate)` visibility so they don't leak into the public library target surface.
2. **CLI Color/Style Regression**: Standard `env_logger::init()` produces verbose, timestamped lines. A custom logger formatter must be explicitly mandated to preserve the polished CLI UX.
3. **Flakiness in Parallel Test Capturing**: Highlighting the risk of standard output capturing in parallel tests and providing a robust strategy (e.g., serial execution or subprocess invocation).

## 8. Verdict
`APPROVE_WITH_SUGGESTIONS`

## 9. Actionable Feedback
1. **CLI Logger Custom Formatter (High Priority)**: Mandate implementing a custom formatter for `env_logger` in `main.rs` (Sub-task 3). This formatter must intercept logs and output them cleanly to preserve existing CLI presentation (suppressing default logger metadata and formatting warning levels with yellow/colored prefixes).
2. **Encapsulate Public API to `pub(crate)` (Medium Priority)**: Update Sub-task 5 to demote all internal-use-only public items in `src/backend/` (such as `is_cache_entry_fresh`, `parse_health_cache_ttl`, `resolve_health_cache_ttl`, `HEALTH_TTL_LOGGED`, and raw caching structs) to `pub(crate)` visibility to keep the public library target clean and encapsulated.
3. **Expand AC1 Dependency Check (Medium Priority)**: Update **AC1** and **Test 1** to verify the clean exclusion of all other binary-only dependencies (`chrono`, `dirs`, `futures`, `minijinja`, `sha2`) from `cargo tree --no-default-features`.
4. **Robust Test Output Capturing (Medium Priority)**: Specify a robust approach for the stdout/stderr capturing tests (Tests 5, 6, 7) in the evaluation suite (e.g., serial execution or subprocess invocation) to prevent flakiness under parallel test runs.
5. **Add Clippy Verifications (Low Priority)**: Under Section 5, add checks to verify both `--no-default-features` and default builds are completely free of Clippy warnings (`cargo clippy --all-targets -- -D warnings` and `cargo clippy --all-targets --no-default-features -- -D warnings`), ensuring that making dependencies optional doesn't introduce unused import warnings.
