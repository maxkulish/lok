# Lessons: CLO-395 opencode health probe

Durable rules from implementing the opencode health probe and Google auth detection for the gemini backend.

## L1 — Health probe tests that mutate env vars must use the shared TEST_MUTEX from backend/mod.rs

**Source incident**: CLO-395 PR pre-flight. `test_get_backends_with_filter` failed because gemini health probe tests set `HOME` to a temp directory, and the backend test's `health_check()` → `detect_auth_from_file()` read the wrong auth.json from the temp `HOME`. The gemini test module initially used its own local `ENV_MUTEX`, which did not serialize with the backend module's `TEST_MUTEX` (tokio async mutex).

**Rule**: Any test under `src/backend/` that sets `HOME`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, or any other env var read by backend-level tests MUST acquire `super::super::acquire_test_lock().await` (the shared `TEST_MUTEX` from `src/backend/mod.rs`). Do not create a local per-module mutex for env var serialization — it won't protect against cross-module races.

**How to apply**: Convert the test to `#[tokio::test]`, call `let _lock = super::super::acquire_test_lock().await;` at the top, and restore env vars before the function returns (the guard is held until drop). For `detect_auth_from_file`, prefer the `_at(path)` testability hook instead of mutating `HOME` to avoid the lock entirely.

## L2 — `detect_auth_from_file()` should expose a path-accepting variant for testability

**Source incident**: CLO-395 test design. `detect_auth_from_file()` reads `$HOME/.local/share/opencode/auth.json`. To test it without env var races, a `detect_auth_from_file_at(path)` variant was added that accepts an explicit path. This allows test files to be created in `tempfile::TempDir` and tested without mutating the global `HOME` env var.

**Rule**: When writing functions that read environment-dependent file paths, provide an internal `_at(path)` variant that accepts an explicit path. The public function delegates to it after resolving from env. This avoids cross-test env var pollution entirely.

**How to apply**: Match the pattern: public function resolves path from env/state → delegates to `_at(path)` private function. Tests call `_at` with a `TempDir`-backed path.

## L3 — Worktree deletion in complete phase must handle stale cwd

**Source incident**: CLO-395 complete phase. `git worktree remove --force` deleted the feature branch worktree (`/Users/mk/Code/orchestrator/lok--feat-clo-395-opencode`) which was also the Bash tool's cwd. Subsequent `bash` calls failed with "Working directory does not exist" until `ctx_execute` was used instead. The workflow YAML update path also shifted from the deleted worktree to the main worktree.

**Rule**: After `git worktree remove` in the complete phase, all subsequent file operations must use absolute paths rooted at the main worktree. The Bash tool's cwd is lost. Use `ctx_execute` with an explicit `cd` prefix as a fallback.

**How to apply**: Before deleting a worktree, `cd` the shell to `/` or the main worktree. If already deleted, use `ctx_execute` with `cd /Users/mk/Code/orchestrator/lok && ...` as the first command.
