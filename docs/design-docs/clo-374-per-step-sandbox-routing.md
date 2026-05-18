# CLO-374 Design: Per-step sandbox routing for Codex and Gemini backends (FR-21)

**Author:** CLO-374 orchestrator (pi)
**Date:** 2026-05-18
**Source PRD:** `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §FR-21, §9 release plan step 8
**Discovery report:** `docs/discovery/clo-374.md`

---

## 1. Problem

Codex sandbox mode (`-s read-only | workspace-write | danger-full-access`) and Gemini's `--approval-mode` are currently hardcoded into each backend's argv builder. The `StepContext.sandbox` field exists (from CLO-371) but is never populated. Without per-step sandbox:

- `apply_edits` steps cannot use Codex because `-s read-only` blocks file writes
- Gemini has no `--approval-mode` support at all
- Users cannot control sandbox granularity per step

**Out of scope:**
- FR-22 (default `workspace-write` for `apply_edits` steps with no explicit sandbox) — separate CLO
- FR-23 (per-step timeout) — already handled by `step.timeout` field
- FR-24/24a (per-step options, `include_directories`) — separate CLOs
- Claude/Ollama/Bedrock sandbox support — no-op, sandbox is meaningful only for CLI subprocess backends

---

## 2. Design

### 2.1 `Step` struct — add `sandbox` field

**File:** `src/workflow.rs`

Add to the `Step` struct:

```rust
/// Sandbox mode for subprocess backends (FR-21).
/// Controls Codex `-s` and Gemini `--approval-mode`.
/// None = backend default (read-only).
#[serde(default)]
pub sandbox: Option<SandboxMode>,
```

**Import:** Add `use crate::backend::SandboxMode;` to the imports in `src/workflow.rs`.

**Deserialization:** `SandboxMode` is defined in `src/backend/context.rs`. It needs `serde::Deserialize` and `serde::Serialize` derives with `#[serde(rename_all = "kebab-case")]` to parse `read-only`, `workspace-write`, `danger-full-access` from YAML. The file already imports serde types via `use serde_json::Value` but does not have `use serde::Deserialize;` — add `use serde::{Deserialize, Serialize};`.

**Current `SandboxMode` has no serde derives.** We need to add them:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}
```

### 2.2 `step_context()` — thread sandbox

**File:** `src/workflow.rs` (line ~167)

```rust
fn step_context<'a>(
    step: &'a Step,
    workflow: &Workflow,
    prompt: &'a str,
    cwd: &'a Path,
) -> backend::StepContext<'a> {
    backend::StepContext {
        sandbox: step.sandbox,
        timeout: workflow
            .step_timeout(step)
            .map(std::time::Duration::from_millis),
        ..backend::StepContext::from_prompt(prompt, cwd, step.model.as_deref())
    }
}
```

Update the doc comment to remove "sandbox remains empty" note.

### 2.3 `CodexBackend` — dynamic sandbox arg injection

**File:** `src/backend/codex.rs`

**Change the `query` method** to build args dynamically based on `ctx.sandbox` instead of using `self.args` verbatim.

Key logic:

```rust
async fn query(&self, ctx: StepContext<'_>) -> Result<QueryOutput, BackendError> {
    // ... existing setup ...

    let mut cmd = Command::new(&self.command);

    // Base args from config (may be empty -> use recommended defaults)
    if self.args.is_empty() {
        cmd.args(["exec", "--json", "--ephemeral"]);
    } else {
        cmd.args(&self.args);
    }

    // Sandbox flag: ctx.sandbox overrides, fallback to read-only
    let sandbox = ctx.sandbox.unwrap_or(SandboxMode::ReadOnly);
    let sandbox_str = match sandbox {
        SandboxMode::ReadOnly => "read-only",
        SandboxMode::WorkspaceWrite => "workspace-write",
        SandboxMode::DangerFullAccess => "danger-full-access",
    };
    cmd.arg("-s").arg(sandbox_str);

    // Model override
    if let Some(m) = effective_model.as_ref() {
        cmd.arg("--model").arg(m);
    }

    // Prompt as positional arg
    cmd.arg("--").arg(prompt)
        .current_dir(cwd)
        // ... rest unchanged ...
}
```

**Design decisions:**
- Sandbox is injected AFTER base args so it overrides any `-s` in custom config args. This is intentional — per-step sandbox should take priority over backend config.
- `--ephemeral` is included ONLY in the no-custom-args default set. If the user configured custom args, we do NOT inject `--ephemeral` (preserving their explicit config).
- When custom `config.args` are provided, we still append sandbox at the end, preserving user-configured flags like `--ignore-user-config`.

### 2.4 `GeminiBackend` — dynamic approval-mode injection

**File:** `src/backend/gemini.rs`

In the shell command string construction, append `--approval-mode <mode>` based on `ctx.sandbox`:

```rust
let approval_flag = match ctx.sandbox {
    Some(SandboxMode::ReadOnly) => " --approval-mode plan",
    Some(SandboxMode::WorkspaceWrite) => " --approval-mode auto_edit",
    Some(SandboxMode::DangerFullAccess) => " --approval-mode yolo",
    None => "",  // backend default (no restriction)
};
```

**Gemini approval mode mapping** (verified against `gemini-cli-gap-analysis.md` §5.8; final verification against `gemini --help` required at implementation time):

| SandboxMode | `--approval-mode` | Effect |
|---|---|---|
| `ReadOnly` | `plan` | Read-only, no mutations |
| `WorkspaceWrite` | `auto_edit` | Auto-approve edit tools |
| `DangerFullAccess` | `yolo` | Auto-approve all (CLI only) |
| `None` | (not added) | Gemini default behaviour |

**Design decision:** When `ctx.sandbox` is `None` (no per-step override), we add no `--approval-mode` flag. This differs from Codex where `None` falls back to `read-only`. Rationale: Gemini backends may be used for read-only tasks today without any flag; adding `--approval-mode plan` would be a behavioural change for all existing Gemini configurations. The v5 PRD treats `None` as "backend default" which for Gemini means the installed default (typically interactive).

### 2.5 `SandboxMode` — add serde derives

**File:** `src/backend/context.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}
```

Also update the `#[allow(dead_code)]` annotation — remove it once `Step.sandbox` starts consuming the enum.

### 2.6 Other backends — no-op

Claude, Ollama, and Bedrock backends do not use sandbox/approval-mode CLIs. No changes needed. The `ctx.sandbox` field is silently ignored.

---

## 3. Implementation plan

### 3.1 Files to change

| File | Change | Lines |
|---|---|---|
| `src/backend/context.rs` | Add serde derives to `SandboxMode` | ~3 lines |
| `src/workflow.rs` | Add `sandbox` field to `Step` struct | ~5 lines |
| `src/workflow.rs` | Thread `sandbox` in `step_context()` | ~2 lines + comment update |
| `src/workflow.rs` | Remove "sandbox remains empty" doc comment | ~1 line |
| `src/backend/codex.rs` | Dynamic sandbox arg injection in `query()` | ~15 lines |
| `src/backend/gemini.rs` | Dynamic approval-mode in shell command | ~10 lines |
| `src/backend/mod.rs` | Re-export `SandboxMode` (already done) | 0 lines |

### 3.2 Test plan

**Type: Unit tests for arg building**

Since we cannot easily capture process argv from a real `Command`, use a test helper `MockBackend` (or a test-only struct with the same arg-building logic) that captures the intended argv as a `Vec<String>`.

| Test | Assertion |
|---|---|
| `codex_sandbox_default` | `None` → Codex argv contains `-s read-only` |
| `codex_sandbox_workspace_write` | `Some(WorkspaceWrite)` → Codex argv contains `-s workspace-write` |
| `codex_sandbox_danger_full_access` | `Some(DangerFullAccess)` → Codex argv contains `-s danger-full-access` |
| `codex_defaults_include_ephemeral` | Default args include `--ephemeral` |
| `gemini_sandbox_default` | `None` → Gemini argv does NOT contain `--approval-mode` |
| `gemini_sandbox_read_only` | `Some(ReadOnly)` → Gemini argv contains `--approval-mode plan` |
| `gemini_sandbox_workspace_write` | `Some(WorkspaceWrite)` → Gemini argv contains `--approval-mode auto_edit` |
| `gemini_sandbox_danger` | `Some(DangerFullAccess)` → Gemini argv contains `--approval-mode yolo` |
| `step_yaml_parsing` | `sandbox: workspace-write` → `Step.sandbox == Some(WorkspaceWrite)` |
| `step_yaml_default` | No `sandbox` field → `Step.sandbox == None` |

### 3.3 Risk assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Gemini approval-mode names differ from gap analysis | Medium | Low | Verify with `gemini --help` at implementation time |
| Codex custom config args contain `-s` already | Low | Medium | Sandbox is appended after base args; per-step wins over config |
| Shell command string escaping breaks for Gemini | Low | Medium | The existing Gemini shell-cmd pattern already handles single-quote escaping; approval-mode is a simple append |
| `--ephemeral` breaks existing Codex behaviour | Low | Low | Per codex-quick-ref.md, this is a recommended addition; it only suppresses session artifact persistence |

---

## 4. Acceptance criteria

- [ ] `SandboxMode` has serde derives with `rename_all = "kebab-case"` parsing `read-only`, `workspace-write`, `danger-full-access`
- [ ] `Step` struct has `sandbox: Option<SandboxMode>` field with `#[serde(rename = "sandbox")]`
- [ ] `step_context()` in `src/workflow.rs` threads `step.sandbox` into `StepContext.sandbox`
- [ ] `CodexBackend::query` uses `ctx.sandbox` to inject `-s <mode>`; `None` → `read-only`
- [ ] `GeminiBackend::query` uses `ctx.sandbox` to inject `--approval-mode <mode>`; `None` → no flag
- [ ] Default Codex args include `--ephemeral` (per codex-quick-ref.md recommended defaults)
- [ ] `cargo test` passes with argv-capture tests for all sandbox levels
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Gemini approval-mode names verified against `gemini --help` and documented in PR description

---

## 5. Prior decisions consulted

- **ADR: Backend trait interface** — not modified, interface already supports `StepContext`
- **CLO-371 design** (`docs/designs/clo-371-migrate-backendquery-to-stepcontext.md`) — established `StepContext.sandbox` field; this CLO wires it end-to-end
- **System patterns** (`docs/context/system-patterns.md`) — per-step config threads through `step_context()` bridge function; this CLO follows the same pattern as `model` and `timeout`
- **Codex quick-ref** (`docs/investigations/codex-quick-ref.md`) — recommended default args include `--ephemeral`, sandbox flags documented
- **Gemini gap analysis** (`docs/investigations/gemini-cli-gap-analysis.md`) — approval-mode names: `plan`, `auto_edit`, `yolo`

---

## 6. Open questions

1. **Gemini `--approval-mode` exact flag names** — must be verified with `gemini --help` at implementation time. The gap analysis dates from an earlier gemini-cli version; names may have changed.
2. **Config args containing `-s`** — if a user's `BackendConfig.args` already includes `-s`, the per-step sandbox will double-specify. Codex picks the last `-s` value in argv, so per-step wins. This is intentional but undocumented; consider adding a warning log if both are present.
