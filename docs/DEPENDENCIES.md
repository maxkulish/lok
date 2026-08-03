# Dependencies - Lok
**Last Updated**: 2026-08-03 (CLO-633 completed; nine open tasks, none blocked)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| - | - | - | - |

Nothing is blocked. The Phase 12 chain (CLO-589 -> CLO-593 -> CLO-591 -> CLO-592, plus CLO-600) closed on 2026-08-02, and every task filed since is independent of the others.

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| [CLO-610](https://linear.app/cloud-ai/issue/CLO-610) | None. Standalone `release.yml` change | 2026-08-01 |
| [CLO-623](https://linear.app/cloud-ai/issue/CLO-623) | PR #71 merged | 2026-08-02 |
| [CLO-624](https://linear.app/cloud-ai/issue/CLO-624) | PR #71 merged | 2026-08-02 |
| [CLO-627](https://linear.app/cloud-ai/issue/CLO-627) | CLO-625 merged | 2026-08-02 |
| [CLO-628](https://linear.app/cloud-ai/issue/CLO-628) | CLO-625 merged | 2026-08-02 |
| [CLO-631](https://linear.app/cloud-ai/issue/CLO-631) | None. Independent codex-security scan finding | 2026-08-03 |
| [CLO-632](https://linear.app/cloud-ai/issue/CLO-632) | None. Independent codex-security scan finding | 2026-08-03 |
| [CLO-634](https://linear.app/cloud-ai/issue/CLO-634) | None. Independent codex-security scan finding | 2026-08-03 |
| [CLO-635](https://linear.app/cloud-ai/issue/CLO-635) | None. Independent codex-security scan finding | 2026-08-03 |

CLO-609 landed on 2026-08-03 (PR #78, `8b96821`), so the pre-publish metadata deadline is cleared.
CLO-633 landed the same day (PR #80, `a8f84d8`). It blocked nothing — the five Phase 15 findings
share an origin, not a mechanism — so no task became ready as a result. It did file four follow-ups,
listed below.

## Follow-ups filed by CLO-633

Recorded here because three of the four are latent defects rather than features, and the fourth
breaks a command this project runs on every task:

- **`commit_file` ignores `git rev-parse HEAD`'s exit status** (`src/tasks/implement.rs:761-767`),
  returning `""` as a successful SHA on an unborn HEAD. CLO-633 made the *slice* safe; the error
  handling is still wrong.
- **`extract_file_references` and `FILE_REF_RE` are duplicated verbatim** across
  `src/tasks/context.rs` and `src/tasks/fix.rs` — the same drift hazard that turned CLO-633's
  Defect 2 into two sites.
- **No CI job builds against the declared `rust-version = "1.80"`.** The oldest toolchain installed
  locally is 1.94 and `ci.yml` uses runner-stable, so the MSRV is an unverified claim. It starts
  costing something once Phase 13 publishes to crates.io.
- **`/pr:review` Step 9.5's re-review poll can never fire against Qodo.** It waits for a *review*
  object on the new head SHA, but Qodo edits its existing review comment in place and submits no
  new review — which the same document states two sections earlier. Observed on PR #80: the poll
  ran to its full timeout while the re-review it was waiting for had already landed as a comment
  update. Same untested-shell-in-markdown family as CLO-623 and CLO-624.

## Standing constraints

Two items CLO-591 left open deliberately. Neither blocks a task, and both get more expensive once the crate is published:

- **`BACKEND_CACHE` is keyed by backend name alone**, so two consumers in one
  process with different configs share an instance. Neither fix the ticket
  proposed works as written; both need `is_backend_available` reworked. PR #66
  shows it already bites in tests.
- **The lib/bin boundary is convention, not compiler-enforced.** The
  `library-boundary` CI job is the compensating control. A workspace split
  before publish is a refactor; after publish it is a rename and a yank.

A third, from CLO-600 and CLO-625: **nobody can push to `main`**, including the repository owner. Ruleset 20153405 requires the `CI Gate` check with no bypass actors, so every change, docs included, arrives through a pull request.

CLO-374 is Done (see ROADMAP Phase 10).
