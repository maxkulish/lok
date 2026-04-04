# Spec Review: clo-203

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-04-04
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and accurate. It perfectly diagnoses the limitations of the current "first-match-wins" strategy and establishes a strong rationale for the three-layer merge approach (system defaults -> user globals -> project locals).

## 2. Acceptance Criteria Review
**Strong**: The criteria are highly specific and cover the core functionality, including field-level overrides, sub-field overrides, HashMap merging by key, partial configs, and backward compatibility with the `--config` flag.
**Gaps**: The spec states `#[serde(deny_unknown_fields)]` should be applied to all config structs, but fails to mention `CacheConfig`, which is part of the `Config` struct but lives in a different file (`src/cache.rs`).

## 3. Constraints Check
**Aligned**: Constraints around merge order, backward compatibility, and relying on `dirs::home_dir()` align well with the current implementation.
**Concerns**: The "Prefer" constraint suggesting "deserialize each layer as `Config`... then merge non-default values" is fundamentally flawed. If the merge condition uses `if other.field != Default::default()`, it becomes impossible for a project config to override a user config's value *back* to the system default. The logic would interpret the explicitly provided default value as "omitted" and keep the user's overridden value instead. The same flawed logic breaks `Vec` replacements and boolean overrides.

## 4. Decomposition Quality
**Well-scoped**: The sub-tasks flow logically from struct updates to the merge logic, to `load_config` modifications, and finally tests.
**Issues**:
- **Sub-task 2** enforces the flawed `Default::default()` merge logic. This should be replaced with a robust merging strategy (e.g., merging parsed `toml::Value` trees prior to deserializing into `Config`).
- **Sub-task 5** suggests testing `load_config` with a tempdir. Because `dirs::home_dir()` and the current working directory rely on process-global state, doing this in parallel `cargo test` executions will cause race conditions and flaky tests. `load_config` must be refactored to take `cwd` and `home` paths as arguments to be purely testable.

## 5. Evaluation Coverage
**Covered**: Solid coverage of scalar overrides, HashMap additions, and handling of partial configs.
**Gaps**:
- Missing a test to verify the edge case where a project config reverts a setting back to the default value (overriding a user config's custom value).
- Missing a test to explicitly verify array replacements (e.g., verifying `BackendConfig.args` are overwritten, not appended).

## 6. Codebase Alignment
**Violations**: Testing by modifying the process environment (`HOME` or `set_current_dir`) is an anti-pattern in Rust since cargo tests run concurrently in a single process.
**Alignment**: The specification accurately references the locations and structures of `src/config.rs`. Leveraging `serde(deny_unknown_fields)` is perfectly aligned with idiomatic Rust deserialization.

## 7. Blind Spots
- **Reverting to Defaults**: The most significant blind spot is the inability of the proposed "simpler approach" to handle reverting values. A true `PartialConfig` or pre-deserialization `toml::Value` merge is absolutely necessary.
- **Testability**: `load_config`'s reliance on environmental APIs makes it untestable without a minor refactoring to accept injected path overrides.
- **File Boundaries**: Forgetting that `CacheConfig` exists in `src/cache.rs` rather than `src/config.rs`.

## 8. Verdict
NEEDS_REVISION

## 9. Actionable Feedback
1. **Fix the Merge Algorithm**: Abandon the `if other.field != Default::default()` strategy. Update the spec to dictate reading the TOML layers as `toml::Value`, running a recursive deep-merge on the TOML tables (where arrays are replaced, and tables are recursively traversed), and finally deserializing the merged `toml::Value` into `Config`. This elegantly handles all edge cases (omitted vs. default values, HashMaps, Arrays) without massive `PartialConfig` boilerplate.
2. **Refactor `load_config` for Testability**: Update Sub-task 3 to extract the core loading logic into a pure function (e.g., `fn load_config_internal(cwd: &Path, home_dir: Option<&Path>, explicit_path: Option<&Path>) -> Result<Config>`). Update Sub-task 5 to test this internal function with `tempdir` paths to avoid race conditions in Rust's parallel test runner.
3. **Include `CacheConfig`**: Add `CacheConfig` (in `src/cache.rs`) to the list of structs requiring `#[serde(deny_unknown_fields)]` in Sub-task 1.
4. **Expand Evaluation Table**: Add two tests to the Evaluation section:
   - "Project reverts user override to default" (verifies explicit default values aren't ignored).
   - "Array replacement" (verifies `BackendConfig.args` is replaced, not appended).
