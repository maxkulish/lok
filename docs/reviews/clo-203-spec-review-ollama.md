# Spec Review: clo-203

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-04
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is **clear, accurate, and self-contained**. I validated:

- ✓ `src/config.rs:227` confirms the first-match-wins strategy
- ✓ The three paths are correctly identified: `./lok.toml` → `~/.config/lok/lok.toml` → `Config::default()`
- ✓ The pain points (no global preferences, duplication, hardcoded defaults) are real limitations
- ✓ Matches Linear task description precisely
- ✓ Correctly identifies CLO-212 as downstream dependent

**Minor gap**: Doesn't mention that `init_config()` writes `Config::default()` - relevant because the spec notes it "must not break" this function, but the problem statement could explicitly state that `lok init` produces a complete config (no partial configs in the wild).

## 2. Acceptance Criteria Review

**Strong**:
- Scalar/Struct/HashMap merge behaviors are precisely defined
- `deny_unknown_fields` placement is specific (lists all structs)
- Backward compatibility for `--config path` is explicit
- Verification method (`cargo test && cargo clippy`) is concrete

**Gaps**:
1. **Missing CacheConfig**: The spec lists structs for `deny_unknown_fields` but omits `CacheConfig` (defined at `src/cache.rs:58`). This struct is a field of `Config` and will fail to parse if it has unknown fields.
2. **Merge conflict resolution unspecified**: What happens when user config has `[backends.codex]` with `timeout = 60` and project config has `[backends.codex]` with `timeout = 120`? The spec says "HashMap merge at key level" but doesn't clarify that the entire `BackendConfig` struct is replaced, not sub-field merged. This is a critical semantic distinction.
3. **Error message format not specified**: "Clear error messages mentioning the unknown field name" is vague. The actual serde error for `deny_unknown_fields` looks like `unknown field 'timout', did you mean 'timeout'?` - the criterion should specify whether this is sufficient or needs custom wrapping.
4. **Missing criterion for user config directory absence**: What if `~/.config/lok/` doesn't exist? Should it silently skip (current behavior would error on `fs::read_to_string`)?

## 3. Constraints Check

**Aligned**:
- Using `dirs::home_dir()` matches existing pattern at `src/config.rs:241` and `src/workflow.rs:2951`
- Merge order (defaults → user → project) matches stated problem
- `--config` bypass is correctly identified as backward-compat requirement

**Concerns**:
1. **`Vec<String>` merge semantics deferred to Escalate**: The spec says "escalate when merge semantics for Vec fields are ambiguous." This is a **Must-decide constraint**, not an escalation. The `BackendConfig.args` field is `Vec<String>` and will be replaced entirely on HashMap value replacement. This should be documented as explicit behavior, not escalated.

2. **Missing constraint**: What about `Option<String>` fields like `BackendConfig.command` or `api_key_env`? If user config sets `api_key_env = "MY_KEY"` and project config has no `api_key_env`, the spec says "Option fields: `if other.field.is_some() { self.field = other.field }`" - but this means project's `None` would NOT override user's `Some("MY_KEY")`. This may be surprising behavior.

3. **PartialConfig complexity acknowledged but underspecified**: The Prefer section suggests `PartialConfig` with all `Option` fields, but the decomposition doesn't include creating this type. The simpler approach (deserialize as Config with serde defaults) has a subtle flaw: can't distinguish "field not present" from "field set to default value" (e.g., `parallel = true` in user config vs. not specified).

## 4. Decomposition Quality

**Well-scoped**:
- Sub-task 1 (deny_unknown_fields) is ~1 hour, clear deliverable
- Sub-task 2 (merge method) is ~2 hours, appropriate scope
- Dependency ordering is correct (1 → 2 → 3 → parallel tests)

**Issues**:
1. **Sub-task 3 missing error handling**: "Clear error messages for parse failures at each layer" is mentioned but not scoped. Need to handle:
   - User config exists but is invalid TOML
   - User config directory is unreadable (permissions)
   - Should invalid user config be a hard error or fall through to defaults?

2. **Missing sub-task**: Update `init_config()` to document that it writes a complete config (no merge implications). The spec says "Do not break `init_config()`" but doesn't include verification steps.

3. **Integration test scope unclear**: Sub-task 5 says "tempdir containing user + project configs" but doesn't specify how to mock `dirs::home_dir()` which is called internally. This requires either:
   - Refactoring `load_config()` to accept a user config path parameter
   - Using a feature flag or conditional compilation for testing
   - Accepting that integration tests can't fully isolate this

4. **Missing CacheConfig addition**: Sub-task 1 should include adding `deny_unknown_fields` to `CacheConfig` in `src/cache.rs`.

## 5. Evaluation Coverage

**Covered**:
- All 8 test cases map to acceptance criteria
- Edge cases table is thorough
- Happy path and error cases both addressed

**Gaps**:
1. **No test for user config parse failure**: The edge cases mention "User config parse failure - should error clearly" but there's no corresponding test in the evaluation table.

2. **No test for user config directory missing**: What if `~/.config/lok/` doesn't exist? Current code would silently skip (fs::read_to_string fails). This behavior should be tested.

3. **Vec replacement behavior not tested**: Spec says project args replace user args, but evaluation table has no test for this.

4. **HashMap key-level merge not tested**: Evaluation table tests "add backend" and "override backend" but not "project removes backend that user added". Actually, per spec, this is impossible (HashMap merge never drops keys) - but should be explicitly tested.

## 6. Codebase Alignment

**Violations**: None found. The spec follows established patterns:
- Uses `anyhow::Context` for error wrapping (matches `src/config.rs:230-231`)
- Uses `serde(default)` attribute pattern consistently
- Follows the `Config::default()` pattern for base config

**Alignment**:
- Error handling matches existing `with_context(|| format!(...))` pattern
- `dirs::home_dir()` usage is consistent with `workflow.rs:2951`
- Test structure follows existing `#[cfg(test)] mod tests` pattern

**Deviations to flag**:
- The spec's proposed `merge()` method signature (`&mut self, other: Config`) differs from typical Rust builder patterns which would use `impl Into<Config>` or consume self. Not wrong, but worth noting.

## 7. Blind Spots

1. **CacheConfig not mentioned**: `src/cache.rs:58` defines `CacheConfig` with `enabled` and `ttl_hours` fields. The spec's `deny_unknown_fields` list omits this. It will break if users have typos in `[cache]` section.

2. **User config directory creation**: The spec doesn't address whether `lok init` should also create `~/.config/lok/lok.toml` (global config). Currently only creates `./lok.toml`. This is probably out of scope but should be noted.

3. **No logging of config sources**: When merge happens, there's no indication which file contributed which setting. For debugging, users may want to know "why is timeout 600?" - traceability is missing.

4. **No validation of backend references**: If user config defines `[backends.custom]` and project config's task references `backends = ["custom2"]`, this is already broken behavior. But merged config could have confusing combinations. Not spec's problem, but worth noting.

5. **Concurrent config file modifications**: What if `lok.toml` is modified while `load_config()` is running? Not mentioned. Probably fine (single-threaded load), but edge case not covered.

6. **Serde flatten not considered**: If any struct uses `#[serde(flatten)]`, `deny_unknown_fields` would break. Checked the codebase - none use flatten, but spec should note this restriction.

7. **PartialConfig vs Config trade-off**: The spec suggests two approaches but doesn't commit. The simpler approach has a real flaw: can't override a field to `false` if user config has `parallel = true` (since `false == default`, merge would skip it). The spec needs to choose one.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec is well-structured, accurately describes the problem, and provides a clear implementation path. The decomposition is logical and testable. However, there are four issues that should be addressed before implementation:

1. **P0**: Missing `CacheConfig` from `deny_unknown_fields` list
2. **P0**: HashMap value replacement semantics need clarification (entire struct replaced, not sub-field merged)
3. **P1**: PartialConfig vs Config approach must be chosen, not left ambiguous
4. **P1**: Vec merge semantics should be documented as explicit behavior, not escalated

## 9. Actionable Feedback

**P0 (Must fix before implementation)**:
1. Add `CacheConfig` to the `deny_unknown_fields` acceptance criterion and decomposition sub-task 1.
2. Clarify HashMap merge semantics: when `backends["codex"]` exists in both layers, the **entire** `BackendConfig` from project replaces the one from user. Sub-fields are NOT merged. Add an example.
3. Document explicit Vec replacement behavior: project's `args` replace user's `args` (not append). Add this to Must constraints.

**P1 (Should fix)**:
4. Choose between PartialConfig and Config-with-defaults approach. Recommend: use `PartialConfig` pattern to correctly handle `parallel = false` override.
5. Add test case for user config parse failure to evaluation table.
6. Add test case for "project config sets field to false when user has true" to verify merge correctness.
7. Specify error message format: wrap serde's error with "Error parsing {path}: {serde_error}".

**P2 (Nice to have)**:
8. Add logging/traceability for which config file contributed each setting.
9. Note that `~/.config/lok/` directory creation is out of scope (or add a follow-up task).
10. Document that `#[serde(flatten)]` must not be added to config structs (would break deny_unknown_fields).
