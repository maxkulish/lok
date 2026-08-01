# Spec Review: clo-600

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-07-31
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is **clear, complete, and accurate**. It is highly self-contained, citing specific telemetry from real workflow runs (`30222596040` to `30223254125`) and pinpointing the exact line and assertion failure inside `gemini_health_check_version_timeout`. It perfectly aligns with the Linear context while correctly narrowing down the scope to the remaining gaps now that workflow dispatch is operational.

## 2. Acceptance Criteria Review
*   **Strong**: The criteria are exceptionally specific, measurable, and cover multiple dimensions including OS coverage, reproducible dependency builds, release-target selection, packaging constraints, and branch protection APIs. 
*   **Gaps**: 
    *   **Crates.io Dry-Run Verification**: AC-8 lists `cargo publish --dry-run --locked` as the verification step. However, a `--dry-run` does not validate token authenticity with crates.io (it compiles and packages locally). This should be noted so that a dummy/dry run does not give false confidence in the credentials themselves.
    *   **Cargo Workspace Locking**: While §3 mentions `--locked` on every dependency resolution, AC-5, AC-6, and AC-7 should explicitly state that the release builds themselves must use `cargo build --release --locked` to prevent dependency drift on compilation.

## 3. Constraints Check
*   **Aligned**: The Must/Must-Not constraints are highly aligned with the codebase. The decision to retain `reqwest`'s default TLS stack and build native glibc binaries on both Linux architectures (rather than cross-compiling static musl with a custom OpenSSL toolchain) is a massive time-saver and avoids runtime TLS issues.
*   **Concerns**: 
    *   **Invalid GitHub Runner Name**: In §3, the native runners list specifies `macos-15-intel`. **GitHub does not provide a `macos-15-intel` runner.** Public Intel macOS runners are hosted exclusively on `macos-13`. Using `macos-15-intel` will cause the Actions runner setup to fail immediately. The constraint must be updated to use `macos-13` for the Intel Mac target.

## 4. Decomposition Quality
*   **Well-scoped**: The 7 sub-tasks are highly independent, modular, and comfortably sized under the 2-hour implementation window. The dependency chaining (1 -> 2; 3 -> 4; (2, 3) -> 7) is logically sound.
*   **Issues**: None. This is an exemplary model of codebase task decomposition.

## 5. Evaluation Coverage
*   **Covered**: The evaluation table maps 1-to-1 with the acceptance criteria and is highly realistic. The inclusion of a manual `Cargo.lock` drift test (Test 6) and pre-release tag shape validations is outstanding.
*   **Gaps**: 
    *   **Multi-Binary Archive Structuring**: In Test 8, clarify whether both identical binaries (`lok` and `lokomotiv`) are shipped inside a single `.tar.gz` archive per target or separate archives. Shipping both identical binaries in a single archive simplifies release management and saves storage.

## 6. Codebase Alignment
*   **Violations**: None.
*   **Alignment**: The specification perfectly aligns with the `Backend` trait interface, `HealthStatus` struct definitions, and the `BackendError` maps to `BackendErrorKind` in `src/utils.rs`.
*   **Concurrency Fix Synthesis**: The `ETXTBSY` hypothesis is correct. While `into_temp_path()` closes the write descriptor, any concurrent thread calling `fork` during the brief script creation window will inherit the open file descriptor, leading to `ETXTBSY` on a concurrent spawn. The codebase already contains `acquire_test_lock()` using a static `TEST_MUTEX` in `src/backend/mod.rs` specifically for this, but it is currently only used in `gemini_health_check_bad_exit` and `gemini_health_check_no_auth`. Rather than altering `write_exec_script` itself, the established pattern dictates that we **explicitly extend `acquire_test_lock()` to all tests that write and execute temporary scripts** (including `gemini_health_check_version_timeout` and the four health-check tests in `src/backend/codex.rs`).

## 7. Blind Spots
*   **Runner Availability**: The use of `ubuntu-24.04-arm` is correct for native Linux Arm64 compilation, but it requires that the repository is on a GitHub plan that has access to public beta ARM runners. If not, the job will fail. (Since this is a public repo, it is fully supported by GitHub's public runner pool).
*   **Workflow Token Permissions**: `release.yml` needs `permissions: contents: write` to publish releases, but `publish.yml` also needs to be examined to ensure it runs with minimal token privileges since it relies on external crates.io secrets.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback

1.  **Correct the macOS Intel Runner Name**: Update §3 and §4 (Sub-task 5) to use `macos-13` instead of `macos-15-intel` for building the native `x86_64-apple-darwin` target.
2.  **Apply static `TEST_MUTEX` to all Probe Tests**: Explicitly specify in Sub-task 2 that `acquire_test_lock()` must be acquired at the start of:
    *   `gemini_health_check_version_timeout`
    *   `codex_health_check_success`
    *   `codex_health_check_unparseable`
    *   `codex_health_check_bad_exit`
    *   `codex_health_check_timeout`
    This completely eliminates parallel `fork`/`exec` races and guarantees that no descriptor is inherited by concurrent tests.
3.  **Define Binary Archiving Strategy**: In Sub-task 5, specify that both compiled binaries (`lok` and `lokomotiv`) are packaged together inside a single target-specific `.tar.gz` (e.g., `lok-<version>-<target>.tar.gz`) to prevent asset bloat.
4.  **Dry-Run Token Caveat**: Add a note to AC-8 that `cargo publish --dry-run` does not hit crates.io's authentication endpoints, so token presence check is a static environment assertion rather than an active validation.
