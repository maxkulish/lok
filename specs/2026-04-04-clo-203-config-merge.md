# Spec: Implement Three-Layer Config Merge with deny_unknown_fields

**Created**: 2026-04-04
**Linear**: CLO-203
**Estimated scope**: S (2 files, ~6 sub-tasks)

## 1. Problem Statement

lok's `load_config()` (`src/config.rs:227`) uses a first-match-wins strategy: it tries `./lok.toml`, then `~/.config/lok/lok.toml`, then `Config::default()`. If a project config exists, the user config is completely ignored. This means:

- Users can't set global preferences (e.g., preferred model, timeout) that apply across all projects
- Project configs must duplicate every setting from the user config, even if they only need to override one backend
- The `Config::default()` hardcodes 4 backends and 2 tasks - if a user wants to add a 5th backend globally, they must add it to every project's `lok.toml`

**What needs to change**: Implement three-layer merge: `Config::default()` -> `~/.config/lok/lok.toml` -> `./lok.toml`. Each layer overrides fields from the previous. HashMap fields (`backends`, `tasks`) merge at key granularity. Add `#[serde(deny_unknown_fields)]` to catch typos.

**Who's affected**: All `load_config()` callers - currently only `src/main.rs:383`. The `Config` struct is used everywhere but the loading logic is centralized.

**Why it matters**: Foundation for CLO-212 (configurable role routing) which adds `[roles]` and `[teams]` config sections. Also a quality-of-life improvement for all lok users.

## 2. Acceptance Criteria

- [ ] `load_config()` loads all three layers: defaults -> user config -> project config
- [ ] Scalar fields (parallel, timeout) override correctly, including reverting to default values (e.g., `parallel = false`)
- [ ] Struct fields (conductor, cache) override at sub-field level
- [ ] HashMap fields (backends, tasks) merge at key level - project adds/overrides specific keys, doesn't replace the entire map
- [ ] Within a HashMap entry, the entire value is replaced (project's `BackendConfig` for key "codex" replaces user's entire entry)
- [ ] `Vec<String>` fields (e.g., `args`) are replaced, not appended
- [ ] Partial configs work - user config with only `[defaults]` doesn't zero out backends
- [ ] `#[serde(deny_unknown_fields)]` on Config, Defaults, ConductorConfig, CacheConfig, BackendConfig, TaskConfig, TaskPrompt
- [ ] Unknown TOML fields produce clear error messages: "Error parsing {path}: unknown field `{name}`"
- [ ] Explicit path (`--config path`) skips merge and loads only that file (backward compatible)
- [ ] `load_config` is testable with injected paths (no global CWD/home dependency in core logic)
- [ ] Existing tests pass unchanged
- [ ] `cargo test` passes, `cargo clippy -- -D warnings` clean

**Verification method**: `cargo test && cargo clippy -- -D warnings`

## 3. Constraints

**Must**:
- Merge order: `Config::default()` -> user (`~/.config/lok/lok.toml`) -> project (`./lok.toml`)
- Use `toml::Value` tree merge approach: read each layer as raw `toml::Value`, deep-merge tables recursively (tables recurse, arrays/scalars replace), then deserialize the merged tree into `Config`. This correctly handles the "omitted vs set to default" problem without needing a `PartialConfig` struct.
- HashMap merge at key level: project key replaces entire value for that key, doesn't sub-merge BackendConfig fields
- `deny_unknown_fields` on all config structs including CacheConfig (`src/cache.rs`)
- Explicit `--config path` bypasses merge (loads only that file, current behavior)
- Use existing `dirs::home_dir()` for user config path (already a dependency)
- Parse failure at any layer produces clear error: `"Error parsing {path}: {serde_error}"`
- Never use `#[serde(flatten)]` on config structs (incompatible with `deny_unknown_fields`)

**Must-not**:
- Do not change `Config::default()` built-in backends/tasks (they remain the base layer)
- Do not add new config sections (that's CLO-212)
- Do not change the `Config` struct fields
- Do not break `init_config()` which writes `Config::default()` to `lok.toml`

**Prefer**:
- Extract core loading logic into `fn load_config_from_paths(cwd: &Path, home_dir: Option<&Path>, explicit_path: Option<&Path>) -> Result<Config>` for testability
- Keep `load_config()` as a thin wrapper that resolves CWD and home_dir

**Escalate when**:
- `deny_unknown_fields` breaks existing lok.toml files (check the project's own lok.toml first)

### toml::Value Deep Merge Algorithm

```rust
fn deep_merge(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                match base_table.entry(key) {
                    Entry::Occupied(mut entry) => deep_merge(entry.get_mut(), value),
                    Entry::Vacant(entry) => { entry.insert(value); }
                }
            }
        }
        (base, overlay) => *base = overlay, // scalars, arrays: replace
    }
}
```

## 4. Decomposition

1. **Add `deny_unknown_fields` to all config structs** - files: `src/config.rs`, `src/cache.rs`
   - Add `#[serde(deny_unknown_fields)]` to: Config, Defaults, ConductorConfig, BackendConfig, TaskConfig, TaskPrompt, CacheConfig
   - Verify the project's own `lok.toml` still parses
   - Run existing tests to catch any breakage

2. **Implement `deep_merge` for toml::Value** - files: `src/config.rs`
   - Recursive table merge: tables recurse, scalars/arrays replace
   - Serialize `Config::default()` to `toml::Value` as the base layer

3. **Refactor `load_config()` with testable core** - files: `src/config.rs`
   - Extract `fn load_config_from_paths(cwd: &Path, home_dir: Option<&Path>, explicit_path: Option<&Path>) -> Result<Config>`
   - Three-layer logic: serialize default -> merge user TOML -> merge project TOML -> deserialize
   - Explicit path bypasses: parse only that file
   - Clear error messages with file path context
   - Keep `load_config()` as thin wrapper resolving CWD and home_dir

4. **Add unit tests for merge and deny_unknown_fields** - files: `src/config.rs`
   - Test: deny_unknown_fields rejects typo
   - Test: scalar override (timeout, parallel = false)
   - Test: HashMap merge (project adds backend, existing preserved)
   - Test: HashMap entry replacement (project overrides entire BackendConfig for key)
   - Test: partial config (only `[defaults]` section)
   - Test: Vec replacement (args replaced, not appended)
   - Test: empty config file is valid
   - Test: user config parse failure produces clear error
   - Test: project sets `parallel = false` when default is `true`

5. **Add integration test for three-layer loading** - files: `src/config.rs`
   - Test `load_config_from_paths` with tempdir containing user + project configs
   - Verify merge precedence: default < user < project

6. **Verify backward compatibility** - files: `src/config.rs`
   - All existing tests pass
   - `init_config()` still works
   - Serialization roundtrip still works

**Dependency order**: 1 -> 2 -> 3 -> (4, 5, 6 in parallel)

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | deny_unknown_fields rejects typo | Error with unknown field name | `cargo test test_deny_unknown_fields` |
| 2 | Scalar merge: timeout override | Later layer wins | `cargo test test_merge_scalar_override` |
| 3 | Boolean merge: `parallel = false` | false overrides true | `cargo test test_merge_boolean_override` |
| 4 | HashMap merge: add backend | New backend added, existing preserved | `cargo test test_merge_hashmap_add` |
| 5 | HashMap merge: override backend | Entire BackendConfig replaced for that key | `cargo test test_merge_hashmap_override` |
| 6 | Partial config merge | Only specified fields change | `cargo test test_merge_partial_config` |
| 7 | Vec replacement | Project args replace, not append | `cargo test test_merge_vec_replace` |
| 8 | Three-layer precedence | default < user < project | `cargo test test_three_layer_precedence` |
| 9 | User config parse failure | Error with file path | `cargo test test_user_config_parse_error` |
| 10 | Existing tests pass | No regressions | `cargo test` |
| 11 | Clippy clean | No warnings | `cargo clippy -- -D warnings` |

**Edge cases to verify**:
- Empty config file (`lok.toml` with no content) - valid, no overrides
- Config with only `[backends.custom]` - adds to defaults, not replace all
- User config directory absent (`~/.config/lok/` missing) - silent skip, no error
- Explicit `--config path` loads only that file, no merge with defaults or user config
- `init_config()` serializes Config::default() correctly with deny_unknown_fields
