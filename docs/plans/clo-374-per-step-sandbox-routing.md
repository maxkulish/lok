# Plan: CLO-374 — Per-step sandbox routing for Codex and Gemini backends (FR-21)

## Context

- Design: `docs/design-docs/clo-374-per-step-sandbox-routing.md`
- Discovery: `docs/discovery/clo-374.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-374/per-step-sandbox-routing-for-codex-and-gemini-backends-fr-21

---

## Sub-tasks

### ST1 Add serde derives to `SandboxMode` enum

**Files:** `src/backend/context.rs`

Add `serde::Deserialize` and `serde::Serialize` derives to `SandboxMode` with `#[serde(rename_all = "kebab-case")]`. Update the import to `use serde::{Deserialize, Serialize};` (currently only imports `serde_json::Value`). Remove the `#[allow(dead_code)]` on the enum definition.

**Acceptance:** `cargo test` passes; `cargo clippy -- -D warnings` clean

**Estimate:** S

### ST2 Add `sandbox` field to `Step` struct

**Files:** `src/workflow.rs`

Add `pub sandbox: Option<SandboxMode>` field to the `Step` struct with `#[serde(default)]`. Add `use crate::backend::SandboxMode;` import.

**Acceptance:** `cargo test` passes; workflow YAML parsing works with `sandbox: workspace-write` and without the field

**Estimate:** S

### ST3 Thread `sandbox` through `step_context()` bridge

**Files:** `src/workflow.rs`

Update the `step_context()` function (line ~167) to set `sandbox: step.sandbox` on the `StepContext`. Update the doc comment that says "History/sandbox/schema/options remain empty until their follow-on CLOs land."

**Acceptance:** `cargo test` passes; no compilation warnings

**Estimate:** S

### ST4 Dynamic sandbox arg injection in `CodexBackend::query`

**Files:** `src/backend/codex.rs`

Modify `CodexBackend::query` to:
- When `self.args.is_empty()` (no custom config args), use `["exec", "--json", "--ephemeral"]` as base args
- When `self.args` is non-empty, use them as-is
- Append `-s <mode>` based on `ctx.sandbox` (fallback `read-only` when `None`)
- Keep `--model` override logic and `--` separator unchanged

**Acceptance:** `cargo test` passes; Codex argv contains correct `-s` flag for each sandbox level

**Estimate:** S

### ST5 Dynamic approval-mode injection in `GeminiBackend::query`

**Files:** `src/backend/gemini.rs`

In the shell command string construction, append `--approval-mode <mode>` based on `ctx.sandbox`:
- `ReadOnly` → `--approval-mode plan`
- `WorkspaceWrite` → `--approval-mode auto_edit`
- `DangerFullAccess` → `--approval-mode yolo`
- `None` → no flag added

**Acceptance:** `cargo test` passes; Gemini shell command contains correct `--approval-mode` flag for each sandbox level

**Estimate:** S

### ST6 Add argv-capture unit tests

**Files:** `src/backend/codex.rs`, `src/backend/gemini.rs` (or new test module)

Add tests that capture the intended argv by constructing the arg list programmatically (without actually spawning a process). Tests:

| Test | Assertion |
|---|---|
| `codex_sandbox_default_none` | `None` → argv contains `-s read-only` |
| `codex_sandbox_workspace_write` | `Some(WorkspaceWrite)` → argv contains `-s workspace-write` |
| `codex_sandbox_danger` | `Some(DangerFullAccess)` → argv contains `-s danger-full-access` |
| `codex_defaults_include_ephemeral` | No custom args → argv contains `--ephemeral` |
| `codex_custom_args_preserved` | Custom `self.args` → they appear before `-s` in argv |
| `gemini_sandbox_none` | `None` → no `--approval-mode` in command string |
| `gemini_sandbox_read_only` | `Some(ReadOnly)` → contains `--approval-mode plan` |
| `gemini_sandbox_workspace_write` | `Some(WorkspaceWrite)` → contains `--approval-mode auto_edit` |
| `gemini_sandbox_danger` | `Some(DangerFullAccess)` → contains `--approval-mode yolo` |

**Acceptance:** All argv-capture tests pass

**Estimate:** M

---

## Pre-merge gate

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Risks

- **Gemini approval-mode names**: Must be verified against `gemini --help` at implementation time. The gap analysis docs may be stale. If names differ, update the design doc and this plan.
- **Codex custom config with `-s`**: If a user's `BackendConfig.args` already includes `-s`, the per-step sandbox will double-specify. Codex picks the last value, so per-step wins. This is intentional but worth a log warning.
- **`--ephemeral` in defaults**: May change Codex session behaviour for users who rely on persisted session artifacts. The flag only suppresses session file persistence; output is unaffected.
