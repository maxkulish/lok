# Design: CLO-383 - FR-22: apply_edits=true defaults Codex sandbox to workspace-write

## Problem

Workflow authors who set `apply_edits = true` on a Codex or Gemini step without an explicit `sandbox` value currently produce silent failures: the workflow engine parses JSON file-edits from the LLM response and writes them to disk, but the backend itself runs under FR-21's per-step default (`-s read-only` for Codex; no `--approval-mode` flag at all for Gemini). The LLM may emit edits, yet the sandbox forbids the very writes the workflow was configured to apply. FR-21 (CLO-374) already threads `StepContext.sandbox` through both subprocess backends; FR-22 is the last gap before per-step sandbox semantics match operator intent. Until this gap closes, `apply_edits` workflows that omit `sandbox` look authored-correct but cannot mutate the workspace - a class of bug that escapes review because the TOML reads as if it should work.

## Goals / Non-goals

**Goals**

- Add `apply_edits: bool` to `backend::StepContext` so backends see per-step intent.
- Resolve an effective sandbox in `CodexBackend::build_argv_prefix` and `GeminiBackend::build_shell_cmd`: when `apply_edits == true && sandbox.is_none()`, treat as `Some(SandboxMode::WorkspaceWrite)`.
- Emit a warning via `println!` when `apply_edits == true && sandbox == Some(SandboxMode::ReadOnly)`; honour the explicit value (do not silently upgrade).
- Honour an explicit `sandbox` setting in every other case (including `DangerFullAccess` and `WorkspaceWrite`) unchanged.
- Wire `apply_edits` through `workflow::step_context()` from `Step.apply_edits` without changing the TOML surface.
- Cover the defaulting matrix with unit tests in both `codex.rs` and `gemini.rs`, plus an integration test that drives a workflow with `apply_edits = true` and observes the argv / shell command shape.

**Non-goals**

- Changing the `Backend` trait shape. No new trait method (Approach C was rejected in discovery).
- Resolving the default in `workflow.rs::step_context()` (Approach B was rejected: it spreads backend-specific semantics into the builder and would set `sandbox` for backends that ignore it).
- Adding `apply_edits` support to Claude, Ollama, or Bedrock backends - those backends do not consume `sandbox` today and are out of scope.
- Surfacing the `apply_edits + read-only` warning anywhere other than backend logs.
- Touching the existing FR-21 sandbox-mapping tests or the kebab-case `SandboxMode` serde contract.
- Persisting the effective sandbox back into the workflow record / run summary (current logs are sufficient).

## Architecture

The change is additive and confined to four files. `StepContext` gains one `Copy`-compatible `bool`. `workflow::step_context()` sources it from `Step.apply_edits`. Each subprocess backend resolves an effective `SandboxMode` at the same point it already consumes `ctx.sandbox`. No new modules, no new types.

```
src/workflow.rs                src/backend/context.rs              src/backend/codex.rs
+-----------------+            +---------------------+             +-----------------------+
| Step            |            | StepContext         |             | build_argv_prefix(    |
|   apply_edits   |---------+  |   sandbox: Option   |  ctx --->   |   base_args,          |
|   sandbox       |---------|->|   apply_edits: bool |             |   sandbox,            |
+-----------------+            +---------------------+             |   apply_edits,        |
                                         |                         |   model)              |
                                         |                         |   -> resolves         |
                                         v                         |      effective mode   |
                               +---------------------+             +-----------------------+
                               | step_context()      |
                               | builds ctx from     |             src/backend/gemini.rs
                               | Step + Workflow     |             +-----------------------+
                               +---------------------+   ctx --->  | build_shell_cmd(...,  |
                                                                   |   sandbox,            |
                                                                   |   apply_edits, ...)   |
                                                                   |   -> resolves         |
                                                                   |      approval flag    |
                                                                   +-----------------------+
```

**Resolution rule (identical in both backends):**

```text
effective = match (apply_edits, sandbox) {
    (true,  None)              => Some(WorkspaceWrite),  // FR-22 default
    (true,  Some(ReadOnly))    => { warn!(); Some(ReadOnly) },
    (_,     other)             => other,                 // explicit wins; apply_edits=false untouched
}
```

The warn-and-pass-through branch lives in backend code because only backend code knows that `ReadOnly` semantically means "cannot write files for this CLI". The shared shape between Codex and Gemini is small enough (a single `match`) that we do not introduce a helper today; if a third subprocess backend ever needs it, the rule can be promoted to `src/backend/context.rs` as `StepContext::effective_sandbox()`.

**Data flow:**

1. TOML parse: `Step.apply_edits: bool` (already present) and `Step.sandbox: Option<SandboxMode>` (already present).
2. `workflow::step_context()` copies both into `StepContext`.
3. `CodexBackend::query` / `GeminiBackend::query` pass `ctx.sandbox` and `ctx.apply_edits` to their respective argv builders.
4. Argv builders compute the effective mode and emit the existing `-s <mode>` / `--approval-mode <mode>` flag.

No FR-21 code path changes shape; the new parameter is threaded alongside the existing one.

## Public API surface

All signatures are Rust as they will appear after this change. Comments preserve the existing doc-comment style.

**`src/backend/context.rs` (before)**

```rust
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct StepContext<'a> {
    pub prompt: &'a str,
    pub history: &'a [Message],
    pub model: Option<&'a str>,
    pub cwd: &'a Path,
    pub sandbox: Option<SandboxMode>,
    pub schema: Option<&'a Value>,
    pub options: Option<&'a StepOptions>,
    pub timeout: Option<Duration>,
}

impl<'a> StepContext<'a> {
    pub fn from_prompt(prompt: &'a str, cwd: &'a Path, model: Option<&'a str>) -> Self { ... }
}
```

**`src/backend/context.rs` (after)**

```rust
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct StepContext<'a> {
    pub prompt: &'a str,
    pub history: &'a [Message],
    pub model: Option<&'a str>,
    pub cwd: &'a Path,
    /// Sandbox level (FR-21). None = backend default.
    pub sandbox: Option<SandboxMode>,
    /// Per-step intent to parse and apply JSON file-edits from the response (FR-22).
    /// Backends that map to a sandbox flag use this to default to `WorkspaceWrite`
    /// when `sandbox` is `None`. Backends that ignore `sandbox` ignore this field too.
    pub apply_edits: bool,
    pub schema: Option<&'a Value>,
    pub options: Option<&'a StepOptions>,
    pub timeout: Option<Duration>,
}

impl<'a> StepContext<'a> {
    pub fn from_prompt(prompt: &'a str, cwd: &'a Path, model: Option<&'a str>) -> Self {
        Self {
            prompt,
            history: &[],
            model,
            cwd,
            sandbox: None,
            apply_edits: false,
            schema: None,
            options: None,
            timeout: None,
        }
    }
}
```

**`src/backend/codex.rs`**

```rust
// Before
fn build_argv_prefix(
    base_args: &[String],
    sandbox: Option<super::SandboxMode>,
    model: Option<&str>,
) -> Vec<String> { ... }

// After
fn build_argv_prefix(
    base_args: &[String],
    sandbox: Option<super::SandboxMode>,
    apply_edits: bool,
    model: Option<&str>,
) -> Vec<String> { ... }
```

Call site in `CodexBackend::query`:

```rust
let argv = Self::build_argv_prefix(
    &self.args,
    ctx.sandbox,
    ctx.apply_edits,
    effective_model.as_deref(),
);
```

**`src/backend/gemini.rs`**

```rust
// Before
fn build_shell_cmd(
    command: &str,
    args: &[String],
    model: Option<&str>,
    sandbox: Option<super::SandboxMode>,
    prompt: &str,
) -> String { ... }

// After
fn build_shell_cmd(
    command: &str,
    args: &[String],
    model: Option<&str>,
    sandbox: Option<super::SandboxMode>,
    apply_edits: bool,
    prompt: &str,
) -> String { ... }
```

Call site in `GeminiBackend::query`:

```rust
let shell_cmd = Self::build_shell_cmd(
    &self.command,
    &self.args,
    effective_model.as_deref(),
    ctx.sandbox,
    ctx.apply_edits,
    prompt,
);
```

**`src/workflow.rs`**

```rust
// Before
fn step_context<'a>(
    step: &'a Step,
    workflow: &Workflow,
    prompt: &'a str,
    cwd: &'a Path,
) -> backend::StepContext<'a> {
    backend::StepContext {
        sandbox: step.sandbox,
        timeout: workflow.step_timeout(step).map(std::time::Duration::from_millis),
        ..backend::StepContext::from_prompt(prompt, cwd, step.model.as_deref())
    }
}

// After
fn step_context<'a>(
    step: &'a Step,
    workflow: &Workflow,
    prompt: &'a str,
    cwd: &'a Path,
) -> backend::StepContext<'a> {
    backend::StepContext {
        sandbox: step.sandbox,
        apply_edits: step.apply_edits,
        timeout: workflow.step_timeout(step).map(std::time::Duration::from_millis),
        ..backend::StepContext::from_prompt(prompt, cwd, step.model.as_deref())
    }
}
```

The `Step` TOML schema is unchanged - `apply_edits` and `sandbox` already exist as documented in `src/workflow.rs:248` and `src/workflow.rs:299`. Public TOML stays additive.

## Assumptions

- **(high)** `apply_edits` is a per-step boolean rather than a per-backend one; consensus steps that fan a single prompt to multiple backends therefore want the same defaulting rule on every subprocess backend. Verified by `src/workflow.rs:248` (single `apply_edits` field on `Step`) and the discovery report's review of `step_context()`.
- **(high)** Only Codex and Gemini consume `ctx.sandbox` today; Claude / Ollama / Bedrock backends will continue to ignore the new `apply_edits` field. Verified by greping `ctx.sandbox` usage during discovery; no other backend touches it.
- **(high)** `StepContext` is `Copy` and all its fields are POD, so adding a `bool` is zero-cost and does not break existing call sites that destructure or spread it. Verified at `src/backend/context.rs:11` and the existing `test_step_context_is_copy` test.
- **(medium)** Emitting a warning log line inside argv-builder functions is acceptable; `println!` / `eprintln!` already serves diagnostics in the project (see `src/conductor.rs`, `src/debate.rs`). Verification path: grep `println!` / `eprintln!` under `src/backend/`; if absent, surface the warning from `query()` and pass an `effective_sandbox` value out of the argv builder.
- **(medium)** The integration test can drive the resolution rule by exercising `CodexBackend::build_argv_prefix` and `GeminiBackend::build_shell_cmd` directly, without spawning real Codex / Gemini binaries. Verification path: existing FR-21 tests already follow this pattern (`codex_sandbox_workspace_write` at `src/backend/codex.rs:184`, `gemini_sandbox_workspace_write_adds_auto_edit` at `src/backend/gemini.rs:322`).
- **(low)** No external caller constructs `StepContext` with positional / struct-literal syntax outside the workspace. If a downstream crate ever did, adding a field would be a breaking change; today the struct is crate-internal. Verification path: `cargo check` across the workspace plus a grep for `StepContext {` outside `src/`.

## Test plan

**Unit tests in `src/backend/codex.rs` (`tests` module):**

- `codex_apply_edits_true_no_sandbox_defaults_workspace_write` - `build_argv_prefix(&[], None, true, None)` emits `-s workspace-write`.
- `codex_apply_edits_true_explicit_workspace_write_preserved` - explicit `WorkspaceWrite` stays `workspace-write` (regression guard for double-resolution).
- `codex_apply_edits_true_explicit_danger_preserved` - explicit `DangerFullAccess` stays `danger-full-access`.
- `codex_apply_edits_true_explicit_read_only_preserved_and_warns` - explicit `ReadOnly` stays `read-only`; assert that the resolved mode is returned from the builder (prefer the return-value approach for testability). If the builder cannot return it, capture stderr / `println!` output instead.
- `codex_apply_edits_false_no_sandbox_keeps_read_only_default` - FR-21 baseline must still produce `-s read-only`.
- `codex_apply_edits_false_explicit_workspace_write_preserved` - confirms `apply_edits = false` does not regress an explicit sandbox.
- `codex_exactly_one_sandbox_flag_with_apply_edits_default` - argv contains exactly one `-s` flag (parallel to existing `codex_exactly_one_sandbox_flag_with_defaults`).

**Unit tests in `src/backend/gemini.rs` (`tests` module):** mirror the Codex matrix against `build_shell_cmd`.

- `gemini_apply_edits_true_no_sandbox_emits_auto_edit` - command contains `--approval-mode auto_edit`.
- `gemini_apply_edits_true_explicit_plan_preserved_and_warns` - command contains `--approval-mode plan`; warn assertion as above.
- `gemini_apply_edits_true_explicit_auto_edit_preserved`.
- `gemini_apply_edits_true_explicit_yolo_preserved`.
- `gemini_apply_edits_false_no_sandbox_omits_approval_flag` - regression guard for FR-21 default (no `--approval-mode` substring).
- `gemini_apply_edits_false_explicit_auto_edit_preserved`.

**Per-backend test matrix:**

| `apply_edits` | `sandbox`             | Codex argv contains      | Gemini cmd contains             |
| ------------- | --------------------- | ------------------------ | ------------------------------- |
| false         | None                  | `-s read-only`           | (no `--approval-mode`)          |
| false         | `ReadOnly`            | `-s read-only`           | `--approval-mode plan`          |
| false         | `WorkspaceWrite`      | `-s workspace-write`     | `--approval-mode auto_edit`     |
| false         | `DangerFullAccess`    | `-s danger-full-access`  | `--approval-mode yolo`          |
| true          | None                  | `-s workspace-write`     | `--approval-mode auto_edit`     |
| true          | `ReadOnly` (+ warn)   | `-s read-only`           | `--approval-mode plan`          |
| true          | `WorkspaceWrite`      | `-s workspace-write`     | `--approval-mode auto_edit`     |
| true          | `DangerFullAccess`    | `-s danger-full-access`  | `--approval-mode yolo`          |

**`workflow.rs` unit test:**

- `step_context_threads_apply_edits` - construct a `Step` with `apply_edits = true`, `sandbox = None`, build a `StepContext` via `step_context()`, assert `ctx.apply_edits == true` and `ctx.sandbox.is_none()` (defaulting happens in the backend, not in the builder).
- **Compilation gate**: run `rg 'StepContext \{' src/ tests/` before committing; every struct-literal construction must include the new `apply_edits` field or be migrated to `StepContext::from_prompt(...)`.

**Integration test (`tests/apply_edits_sandbox.rs`, new file):**

- `workflow_apply_edits_step_defaults_codex_sandbox_to_workspace_write` - parse a minimal TOML workflow with one Codex step (`apply_edits = true`, no `sandbox`); reach into the same argv-builder used by `query()` (or assert via the workflow's pre-flight argv snapshot if exposed) and confirm `workspace-write` appears.
- `workflow_apply_edits_explicit_read_only_preserved` - same workflow but with `sandbox = "read-only"`; assert `-s read-only` is preserved.

If the project's existing tests folder layout suggests these belong as backend unit tests instead (no live binary needed), the integration test can be downgraded to a `#[test]` in `codex.rs` / `gemini.rs` that constructs `StepContext` directly. Final placement chosen during implementation; FR-22 coverage matrix above is binding.

**Manual verification:**

1. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` from the worktree root (the pre-merge gate).
2. Run a workflow with `apply_edits = true`, `backend = "codex"`, no `sandbox`; confirm via `RUST_LOG=lokomotiv=debug` that the Codex argv contains `-s workspace-write`.
3. Run the same workflow with `sandbox = "read-only"`; confirm warn line fires and argv still contains `-s read-only`.

## Migration / rollout

The change is purely additive:

- `StepContext` gains a `bool` field with default `false` in `from_prompt`. No serialization of `StepContext` exists (it is a runtime carrier struct, not a config type), so there is no schema migration.
- Compilation is the regression guard: `cargo check` will fail if any `StepContext { ... }` struct literal across `src/` or `tests/` omits the new field. A pre-commit grep (`rg 'StepContext \{' src/ tests/`) catches any missed literal.
- `Step` TOML surface is unchanged. Workflows that omit `apply_edits` (the common case) still get FR-21's `read-only` default. Workflows with `apply_edits = true` and no `sandbox` will now succeed where they previously silently failed - the observable change is "edits actually land", which is the intended fix and matches PRD FR-22.
- No feature flag is required. The behaviour change is bounded to the `(apply_edits=true, sandbox=None)` cell of the matrix, which was a silent-failure cell before this change; no working configuration shifts behaviour.
- Rollout order: land in a single PR; no staged deploy needed. The pre-merge gate covers the matrix.
- Backward compatibility: any caller that constructs `StepContext` literally (e.g. tests) must add the new field. All such call sites live in this crate; `cargo check` will surface them. Prefer the spread pattern (`..StepContext::from_prompt(...)`) where it already exists.

## Open questions

- **Warning emission point.** Should the `apply_edits + read-only` warn fire inside `build_argv_prefix` / `build_shell_cmd` (close to the resolution but inside a pure-function-ish helper) or in `query()` (cleaner separation, but the helper would have to return an effective-sandbox tuple)? Discovery did not pin this down. Tradeoff: in-helper keeps the rule in one place but couples a pure builder to stderr output; in-`query` keeps builders deterministic but spreads the warning logic across two call sites. The test plan above assumes in-helper for unit-test simplicity; revisit if implementation finds the coupling awkward.
- **Helper extraction.** The resolution `match` is identical between Codex and Gemini. Extract it as `StepContext::effective_sandbox(&self) -> Option<SandboxMode>` now, or wait until a third subprocess backend appears? Discovery's Approach A cons list flagged this as deferrable. Inlining today avoids speculative abstraction; extracting today centralises the rule. No decision required to ship FR-22 either way.
- **Consensus + mixed-backend steps.** A step can declare `backends = ["codex", "gemini", "claude"]` with `apply_edits = true`. Codex and Gemini will both default to write-capable sandboxes; Claude will ignore `apply_edits` (no sandbox concept). Is this the right behaviour, or should `apply_edits` be rejected at parse time for steps that include non-edit-capable backends? PRD FR-22 reads as "default the sandbox", not "validate backend compatibility", so the current scope leaves this open for a follow-on issue if operators report confusion.
