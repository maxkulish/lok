# Dependencies - Lok
**Last Updated**: 2026-08-03 (CLO-633 started; the five codex-security scan findings added, eleven open tasks, none blocked)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| - | - | - | - |

Nothing is blocked. The Phase 12 chain (CLO-589 -> CLO-593 -> CLO-591 -> CLO-592, plus CLO-600) closed on 2026-08-02, and every task filed since is independent of the others.

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| [CLO-609](https://linear.app/cloud-ai/issue/CLO-609) | None. Standalone manifest fix | 2026-08-01 |
| [CLO-610](https://linear.app/cloud-ai/issue/CLO-610) | None. Standalone `release.yml` change | 2026-08-01 |
| [CLO-623](https://linear.app/cloud-ai/issue/CLO-623) | PR #71 merged | 2026-08-02 |
| [CLO-624](https://linear.app/cloud-ai/issue/CLO-624) | PR #71 merged | 2026-08-02 |
| [CLO-627](https://linear.app/cloud-ai/issue/CLO-627) | CLO-625 merged | 2026-08-02 |
| [CLO-628](https://linear.app/cloud-ai/issue/CLO-628) | CLO-625 merged | 2026-08-02 |
| [CLO-631](https://linear.app/cloud-ai/issue/CLO-631) | None. Independent codex-security scan finding | 2026-08-03 |
| [CLO-632](https://linear.app/cloud-ai/issue/CLO-632) | None. Independent codex-security scan finding | 2026-08-03 |
| [CLO-634](https://linear.app/cloud-ai/issue/CLO-634) | None. Independent codex-security scan finding | 2026-08-03 |
| [CLO-635](https://linear.app/cloud-ai/issue/CLO-635) | None. Independent codex-security scan finding | 2026-08-03 |

CLO-609 is the one with a deadline attached to it rather than a blocker: per-version crate metadata freezes at publish, so it has to land before the next release or the wrong repository link is baked into another version.

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
