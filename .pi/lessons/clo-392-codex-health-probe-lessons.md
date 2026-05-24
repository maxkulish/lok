# Lessons: CLO-392 — Codex health probe + version-aware unusable-flag matrix

Durable rules extracted from the CLO-392 implementation and review cycle.

---

## L1 — Validation-gate bounded fix iteration prevents unbounded loops

**Source incident:** CLO-392 pre-PR validation (`docs/reviews/clo-392-validation-synthesis.md`) returned `PROCEED_WITH_FIXES` with 3 MUST FIX items (HIGH: warning placement wrong, MEDIUM: child-process leak, MEDIUM: missing integration tests). One bounded fix iteration resolved all three. Re-running the full gate confirmed green.

**Rule:** When the synthesis returns `PROCEED_WITH_FIXES`, enumerate every MUST FIX item, apply all in one commit, then re-run the full pre-merge gate (`fmt + clippy + test`). Do NOT cherry-pick fixes or loop back to the validation gate for a second synthesis pass; one iteration is the budget.

**How to apply:**
1. After reading the synthesis, copy every MUST FIX into a checklist in the commit message.
2. Apply all fixes, commit as `fix(CLO-XX): address pre-pr review feedback`.
3. Re-run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` locally.
4. Only if the gate is green, mark `validation_fix_iteration_count` and proceed to PR.

---

## L2 — Explicit return types prevent reviewer confusion under `anyhow::Result`

**Source incident:** PR #40 review by `gemini-code-assist` flagged `fn parse_version(...) -> Result<(u32, u32, u32), super::BackendError>` as a compilation error because `anyhow::Result<T, E = Error>` only takes one type parameter. The reviewer did not notice that `super::BackendError` was the explicit `E` type parameter and `anyhow::Result` is actually an alias for `core::result::Result<T, E = Error>`.

**Rule:** In modules that `use anyhow::Result;`, helper functions returning a *different* error type must spell out `std::result::Result<T, E>` explicitly. Using the bare `Result<T, E>` identifier invites reviewer misreading even when rustc resolves it correctly.

**How to apply:**
- After adding `use anyhow::Result;` to a module, run `grep -n "-> Result<" src/...` and change any non-anyhow returns to `-> std::result::Result<...>`.
- When reading PR feedback about "compilation errors" that you know compile, check whether the issue is type-identifier ambiguity before dismissing it.

---

## L3 — Hardcoded flag lists in workflow validation are a coupling hotspot

**Source incident:** `Workflow::validate()` contains a hardcoded `flags_used` array (`["--json", "--ephemeral", "-o", "-s"]`) that duplicates the default flags passed by `CodexBackend::query()`. Future Codex CLI releases may add new default flags, and `Workflow` will silently miss them.

**Rule:** When a workflow validator checks backend-specific flags, the authoritative list must live in the backend module and be exposed via a public const or trait method. Duplicating it in `workflow.rs` creates a desynchronization risk that no compiler will catch.

**How to apply:**
- Add `pub const DEFAULT_FLAGS: &[&str] = &["--json", ...]` to `CodexBackend`.
- Reference `crate::backend::codex::CodexBackend::DEFAULT_FLAGS` in `workflow.rs` instead of hardcoding.
- Include this in the release checklist alongside the flag matrix sync.
