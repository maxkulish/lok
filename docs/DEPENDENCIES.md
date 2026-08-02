# Dependencies - Lok
**Last Updated**: 2026-08-02 (CLO-592 completed)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| - | - | - | - |

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| - | - | - |

CLO-592 is done. The two items CLO-591 left open remain relevant:

- **`BACKEND_CACHE` is keyed by backend name alone**, so two consumers in one
  process with different configs share an instance. Neither fix the ticket
  proposed works as written; both need `is_backend_available` reworked. PR #66
  shows it already bites in tests.
- **The lib/bin boundary is convention, not compiler-enforced.** The
  `library-boundary` CI job is the compensating control. A workspace split
  before publish is a refactor; after publish it is a rename and a yank.

CLO-374 is Done (see ROADMAP Phase 10).
