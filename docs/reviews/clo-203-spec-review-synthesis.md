# Spec Review Synthesis: clo-203

**Synthesized**: 2026-04-04
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Missing `CacheConfig`** from `deny_unknown_fields` list - both reviewers independently identified `src/cache.rs` is omitted from Sub-task 1 | P0 |
| 2 | **Merge algorithm is flawed** - the "simpler approach" (`if other.field != Default::default()`) cannot distinguish "field omitted" from "field explicitly set to default value". Breaks boolean overrides (`parallel = false`), Vec replacements, and reverting-to-defaults | P0 |
| 3 | **`load_config` untestable without refactoring** - `dirs::home_dir()` and CWD are process-global state; parallel `cargo test` will race. Must inject paths as arguments | P1 |
| 4 | **Vec merge semantics must be decided, not escalated** - `BackendConfig.args` replacement vs. append is a concrete design choice that affects correctness | P1 |
| 5 | **Evaluation table missing key tests** - no test for array replacement behavior, no test for reverting a value back to its default | P1 |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | **Merge strategy** | Replace the entire merge approach: read TOML layers as `toml::Value`, deep-merge tables recursively, then deserialize the merged tree into `Config`. Avoids `PartialConfig` boilerplate entirely | Use `PartialConfig` pattern (all `Option` fields) to distinguish present-vs-absent. More idiomatic Rust, but more boilerplate |
| 2 | **HashMap value merge granularity** | Implicit - project replaces entire backend entry (follows from `toml::Value` tree merge where tables recurse but terminal values replace) | Wants explicit spec language: when `backends["codex"]` exists in both layers, the **entire** `BackendConfig` from project replaces user's. Sub-fields are NOT merged |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `Option<String>` fields (e.g. `api_key_env`) - project's `None` won't override user's `Some(...)`, which may surprise users | Ollama | P1 |
| 2 | No test for user config parse failure (invalid TOML in `~/.config/lok/lok.toml`) | Ollama | P1 |
| 3 | No test for missing user config directory (`~/.config/lok/` absent) | Ollama | P2 |
| 4 | Missing logging/traceability of which config file contributed each setting | Ollama | P2 |
| 5 | `#[serde(flatten)]` must never be added to config structs (would break `deny_unknown_fields`) - should be documented as a constraint | Ollama | P2 |
| 6 | `init_config()` writes a complete `Config::default()` - spec should verify this isn't broken, not just assert it | Ollama | P2 |
| 7 | Error message format for parse failures is vague - should specify wrapping pattern like `"Error parsing {path}: {serde_error}"` | Ollama | P2 |

## Consolidated Verdict

**NEEDS_REVISION**

Gemini issued NEEDS_REVISION. The core merge algorithm is unsound in its current form - both reviewers converge on this independently, differing only in the recommended fix.

## Priority Actions

**P0 - Must fix before implementation:**

1. **Fix the merge algorithm.** The `if other.field != Default::default()` strategy is broken. Two viable alternatives were proposed:
   - **(a) `toml::Value` tree merge** (Gemini): Read each layer as raw `toml::Value`, deep-merge tables recursively (arrays replace, tables recurse), then deserialize once into `Config`. Zero boilerplate, handles all edge cases naturally.
   - **(b) `PartialConfig` with `Option` fields** (Ollama): More idiomatic Rust, explicit about presence-vs-absence, but requires maintaining a parallel struct.
   - **Recommendation:** Option (a) is simpler and eliminates the entire class of "can't distinguish omitted from default" bugs. Choose one and update Sub-tasks 2 and 3 accordingly.

2. **Add `CacheConfig`** (`src/cache.rs`) to the `deny_unknown_fields` struct list in Sub-task 1.

3. **Document Vec and HashMap merge semantics as Must constraints**, not escalation items. Specify: project's `args` replaces user's `args`; project's `BackendConfig` for a given key replaces user's entirely (or sub-field merges if using tree approach - decide and document).

**P1 - Should fix:**

4. **Refactor `load_config` for testability.** Extract to `fn load_config_internal(cwd: &Path, home_dir: Option<&Path>, explicit_path: Option<&Path>) -> Result<Config>`. Update Sub-task 3 and Sub-task 5 accordingly.

5. **Expand evaluation table** with:
   - Project reverts user override back to default value
   - Array replacement (not append) for `BackendConfig.args`
   - Project sets `parallel = false` when user has `parallel = true`
   - User config is invalid TOML (error handling)

6. **Clarify `Option<String>` override behavior** - can a project config clear an `api_key_env` set by user config? If using `toml::Value` merge, absent keys are simply not present in the tree (correct). If using `PartialConfig`, `None` means "not specified" (also correct). Document the expected behavior either way.

**P2 - Nice to have:**

7. Add logging of config source provenance (which file set which value)
8. Document that `#[serde(flatten)]` is incompatible with `deny_unknown_fields`
9. Specify error message wrapping format for parse failures
10. Test for absent `~/.config/lok/` directory (silent skip)
