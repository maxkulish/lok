# Plan: CLO-393 — lok doctor HealthStatus renderer

**Task**: CLO-393
**Planned**: 2026-05-25

---

## Sub-tasks

### ST1: Modify Doctor command variant
**File**: `src/main.rs`
**Change**: Convert `Commands::Doctor` from unit variant to struct variant with `--output` flag.
**Acceptance**: `cargo check` + `cargo run --bin lok -- doctor --help` shows `--output <FORMAT>`

### ST2: Write print_doctor_table() helper
**File**: `src/main.rs`
**Change**: New function that renders entries as a human-readable table with columns BACKEND, MODE, VERSION, AVAILABLE, NOTES. Uses `colored::Colorize` for green/red availability markers. Truncates NOTES to fit terminal width.
**Acceptance**: `cargo check` compiles

### ST3: Write print_doctor_json() helper
**File**: `src/main.rs`
**Change**: New function that serializes entries as a stable JSON array of objects. Each object has `backend` key from config name + all `HealthStatus` fields. Uses `serde_json::to_string_pretty`.
**Acceptance**: `cargo check` compiles

### ST4: Rewrite Commands::Doctor handler
**File**: `src/main.rs`
**Change**: Replace hardcoded checks with:
- Call `warmup_backends(&config)` (already wired)
- Read `BACKEND_CACHE` for all enabled backends
- Collect into `Vec<(String, HealthStatus)>`
- Dispatch to `print_doctor_table` or `print_doctor_json` based on `--output`
- Exit code 0 if all available, 1 otherwise
**Acceptance**: `cargo run --bin lok -- doctor` shows backend table

### ST5: Update guide
**File**: `docs/guides/lok-setup-guide.md`
**Change**: Add `## lok doctor` section documenting table columns, JSON format, exit codes, and edge cases.
**Acceptance**: Section is present with correct column descriptions

### ST6: Add integration test
**File**: `tests/integration.rs`
**Change**: Add `test_doctor_json_output` test that runs `lok doctor --output json` and verifies valid JSON array with `backend` and `available` fields.
**Acceptance**: `cargo test --test integration test_doctor_json_output` passes

### ST7: Verify pre-merge gate
**Command**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
**Acceptance**: All green
