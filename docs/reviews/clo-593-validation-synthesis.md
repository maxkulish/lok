# Pre-PR validation: clo-593

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-26
**Pipeline**: lok pre-pr-validation
---

Verification complete. I found the reviewers were reviewing two different repositories, and I found a blocker neither of them caught.

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK (wrong review target) | Reported FAIL. Analysis of the `gcm` checkout is factually correct (`main` == `HEAD` == `bf8e395`, empty diff), but the design scopes all source changes to `~/Code/orchestrator/lok`. Codex never inspected that repo. |
| Gemini | OK | Reported PASS. Reviewed the correct repo; every substantive claim I re-verified held up. Test-count characterization was imprecise. |
| Claude fallback | SKIPPED | Both external reviewers returned `success=true`. |

## Verdict

PASS_WITH_NOTES

## Grounding evidence

The design doc states at line 9: *"Code under change: the `lokomotiv` package at `~/Code/orchestrator/lok`, `main @ 796154d`... no gcm source changes are in scope here."* The implementation is present there as 5 commits on top of exactly that base:

```
e14957e feat(CLO-593): add ADR and finish pre-merge gate
8f9802f feat(CLO-593): add external-consumer integration test
88a2a5d feat(CLO-593): split orchestration into engine.rs
0ed5665 feat(CLO-593): extract BackendConfig and retry defaults
b0e8c5c feat(CLO-593): add [lib] target
```

Gates I ran myself: `cargo build --all-targets` clean; `cargo clippy --all-targets` clean (only the pre-existing dual-bin `main.rs` warning); full suite green. G1–G7 all present — `[lib]` in `Cargo.toml`, `src/lib.rs`, `src/engine.rs` (1,951 lines), `src/backend/config.rs` (189 lines), `tests/backend_public_api.rs`, `docs/adrs/001-lib-target-vs-workspace.md`. G3 verified by grep: no `crate::config`, `indicatif`, or `futures::` remain in `src/backend/` outside one doc comment.

**Test baseline resolved.** HEAD reports 1,232 passing vs the design's 1,356 baseline. This is not a regression. I ran the suite at `796154d` in a scratch worktree: the base ran 664 unit tests twice (two bin targets sharing `main.rs`), while HEAD splits them into a 149-test lib suite that runs *once* plus 525 bin tests that run twice. Unique tests went **692 → 708**. No tests were lost; 10 unit tests and 6 integration tests were added. I removed the worktree afterward.

## Must Fix Before PR

- **The lok implementation is committed onto `main`, not `feat/clo-593-extract`.** Neither reviewer caught this. `git branch --show-current` in the lok repo returns `main`, and all 5 CLO-593 commits are unpushed (`origin/main..HEAD`). The design doc names `feat/clo-593-extract` as the branch, and global git conventions require it. A PR cannot be opened `main` → `main`. Fix is mechanical and safe because nothing is pushed: create `feat/clo-593-extract` at `e14957e`, then `git reset --hard origin/main` on `main`, then push the branch.
- **The gcm-side CLO-593 docs are untracked.** Codex's MEDIUM is correct and is the real gcm-repo finding. All 7 files (design, plan, PRD, discovery, reviews, workflow YAML) are `??` in `git status`. The plan's gcm-only gate (line 123) is precisely this docs commit, so the gcm branch currently carries nothing.
- **Live Ollama acceptance test is unrun.** `ollama_query_round_trip` is correctly written and `#[ignore]`d, satisfying G6's structure, but plan ST9 requires actually running it against a local `ollama serve` and recording results in the PR description. Currently 1 ignored, unexecuted.

## Out of Scope / Deferred

- **`BACKEND_CACHE` keyed only by backend name** (Gemini rec #1). Real, but explicitly locked by design decision **D6** and recorded in Open questions Q2 as a known consequence. Redesigning a process-global cache inside a pure extraction ticket is exactly what the design forbids. Belongs to CLO-594.
- **`retry.rs` writing to stderr via `eprintln!`/`colored`** (Gemini rec #2). Legitimate library-hygiene concern, but the design's non-goals bar behaviour changes: *"no new... retry maths"*. Converting to a `log`/`tracing` facade is a follow-up.
- **`acquire_test_lock()` returns `tokio::sync::MutexGuard<'static, ()>`** (Gemini rec #3). Confirmed at `src/backend/mod.rs:507`. It leaks the Tokio version into the consumer contract, but sits behind the `test-support` feature, so it is not on the default public surface. Right call for CLO-594, which locks the boundary.
- **`ClaudeBackend` missing from the `lib.rs` root re-exports** while Codex/Gemini/Ollama/Bedrock are present. It stays reachable as `lokomotiv::backend::ClaudeBackend` (`mod.rs:18`), and plan ST7 line 73 only required the other three, so this matches the plan. A one-line consistency fix worth folding in opportunistically.

## False Positives / Tooling Artifacts

- **Codex CRITICAL — "no committed delta from `main`"**: factually accurate about the gcm checkout but reviews the wrong repository. The design's line 9 makes an empty gcm source diff the *expected* outcome. Not a defect.
- **Codex HIGH — "the checked-out repo is gcm, but the design says lok"**: this is the correct diagnosis of its own CRITICAL, misfiled as a blocker. The implementation does exist and is complete; Codex simply lacked the path. Its "Missing Items" list is entirely void — I verified every item present.
- **Gemini finding #3 — "All 525 unit tests remain green"**: understates the picture. 525 is only the binary unit suite; the real total is 1,232 across 8 suites including the new 149-test lib suite. The conclusion (tests green) holds.

## Re-validation after fix iteration 1

Fixes applied:

1. **Branch shape corrected.** In `~/Code/orchestrator/lok`, created `feat/clo-593-extract` from the 5 CLO-593 commits and reset `main` to `origin/main` (c12b3c5). The branch was then rebased onto `origin/main`; rebase completed cleanly and the full suite still passes.
2. **gcm docs committed.** All CLO-593 design, plan, discovery, PRD, review, validation, and workflow artifacts are now committed on the gcm `feat/clo-593-extract` branch.
3. **Live Ollama acceptance test executed.** Ran `cargo test --test backend_public_api -- --ignored ollama_query_round_trip` against a local `ollama serve` (model `gemma4:12b`). Result: **passed**.

Updated verdict after fixes: **PASS**.

Remaining notes are non-blocking follow-ups for CLO-594: process-global `BACKEND_CACHE`, `retry.rs` stderr output, and opaque `TestLockGuard` for `test-support`.

