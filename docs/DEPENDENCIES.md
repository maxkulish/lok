# Dependencies - Lok
**Last Updated**: 2026-09-15 (CLO-660 in progress and placed in ROADMAP Phase 13. The crates.io publishing entry under Standing constraints replaces the note on "once the crate is published")

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| [CLO-650](https://linear.app/cloud-ai/issue/CLO-650) | CLO-623 | Not started | Bot-identity hardening belongs in the script CLO-623 extracts, not in duplicated markdown |

One task waits by design (above). The Phase 12 chain (CLO-589 -> CLO-593 -> CLO-591 -> CLO-592, plus CLO-600) closed on 2026-08-02; every other open task is independent.

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
| [CLO-639](https://linear.app/cloud-ai/issue/CLO-639) | None. CLO-633 follow-up | 2026-08-03 |
| [CLO-640](https://linear.app/cloud-ai/issue/CLO-640) | None. CLO-633 follow-up | 2026-08-03 |
| [CLO-649](https://linear.app/cloud-ai/issue/CLO-649) | None. Independent spec-review harness fix | 2026-08-06 |
| [CLO-651](https://linear.app/cloud-ai/issue/CLO-651) | None. Found during the 2026-08-06 Actions outage | 2026-08-07 |
| [CLO-652](https://linear.app/cloud-ai/issue/CLO-652) | None. Independent agent-template fix | 2026-08-07 |

CLO-609 landed on 2026-08-03 (PR #78, `8b96821`), so the crate metadata is ready for the first crates.io publish under a name this project controls.
CLO-633 landed the same day (PR #80, `a8f84d8`). It blocked nothing — the five Phase 15 findings
share an origin, not a mechanism — so no task became ready as a result. It did file four follow-ups,
listed below.

## Follow-ups filed by CLO-633

Recorded here because three of the four are latent defects rather than features, and the fourth
breaks a command this project runs on every task. All four were written up as prose on 2026-08-03
without their issue IDs, which kept them out of every prioritised list until the 2026-08-06 sync:

- **[CLO-639](https://linear.app/cloud-ai/issue/CLO-639) - `commit_file` ignores `git rev-parse HEAD`'s exit status**
  (`src/tasks/implement.rs:761-767`), returning `""` as a successful SHA on an unborn HEAD.
  CLO-633 made the *slice* safe; the error handling is still wrong. Phase 16.
- **[CLO-640](https://linear.app/cloud-ai/issue/CLO-640) - `extract_file_references` and `FILE_REF_RE` are duplicated verbatim**
  across `src/tasks/context.rs` and `src/tasks/fix.rs` — the same drift hazard that turned CLO-633's
  Defect 2 into two sites. Phase 16.
- **[CLO-638](https://linear.app/cloud-ai/issue/CLO-638) - No CI job builds against the declared `rust-version = "1.80"`.**
  The oldest toolchain installed locally is 1.94 and `ci.yml` uses runner-stable, so the MSRV is an
  unverified claim. It already costs anyone who builds from source with an older toolchain, which is
  why it now sits in Phase 13.
- **[CLO-637](https://linear.app/cloud-ai/issue/CLO-637) - `/pr:review` Step 9.5's re-review poll timed out on every clean pass**
  (landed 2026-08-06, PR #84 squashed as `4263a1c`). It waited for a *review* object on the new head SHA, but Qodo submits
  one only when a pass carries new inline findings; a clean re-review updates its review comment in
  place and announces completion with a new comment naming the covered commit, so the gate failed
  precisely on the success case. Observed on PR #80. Same untested-shell-in-markdown family as
  CLO-623 and CLO-624, so it joins them in Phase 14.

## Standing constraints

One item CLO-591 left open deliberately. It blocks no task, and its cost depends on the crates.io naming decision described below, not on a date:

- **The lib/bin boundary is convention, not compiler-enforced.** The
  `library-boundary` CI job is the compensating control. While no library
  version is on crates.io, a workspace split is a refactor with no yank,
  though it could change what git consumers write. Once a library version is
  published, changing the layout also means a new crate and a registry
  migration. `docs/decisions/clo-592-workspace-split.md` re-affirmed not
  splitting on 2026-09-15 and ties the question to the naming decision.

The second, **`BACKEND_CACHE` keyed by backend name alone**, was resolved by
[CLO-653](https://linear.app/cloud-ai/issue/CLO-653) on 2026-08-07. The cache now keys on
`BackendKey { name, config, retry }`. Tracking it here as a constraint rather than as an
issue is why it stayed invisible to every backlog view for as long as it did — the same
failure mode is worth watching for in the remaining entry.

Two bounds replaced it, both documented in `src/lib.rs` rather than here because a consumer
needs them at the API: the key cannot capture ambient construction inputs (the resolved
value behind `api_key_env`, Bedrock's AWS environment), and the cache is still
process-global rather than owned by the embedding host.

**crates.io publishing** (last verified 2026-09-15): the crates.io name `lokomotiv` is owned
by `ducks`, the upstream author, who published all 28 versions. The last one is `20260208.0.2`
(2026-02-08). Every version is binary-only and none is yanked. This project has never published,
and its `[lib]` target reached main later, in `ee28f3c` (2026-07-27). A library publish needs
either an owner grant from `ducks` or a new crate name, and that decision is open (see
`docs/PROJECT.md` Up Next). Until it is made, the library is consumed as a git dependency on
`github.com/maxkulish/lok`, and a downstream crate that depends on it that way cannot itself be
published to crates.io. Re-verify the owner and the latest version before relying on them:

```bash
curl -s -H 'User-Agent: lok (github.com/maxkulish/lok)' https://crates.io/api/v1/crates/lokomotiv/owners | jq -r '.users[].login'
curl -s -H 'User-Agent: lok (github.com/maxkulish/lok)' https://crates.io/api/v1/crates/lokomotiv/versions | jq -r '.versions | sort_by(.created_at) | [length, (map(select(.yanked)) | length), last.num, last.created_at[:10]] | @tsv'
```

The first prints the owners (`ducks`). The second prints the version count, the yanked count, and
the newest version with its date (`28  0  20260208.0.2  2026-02-08`). Recorded by
[CLO-660](https://linear.app/cloud-ai/issue/CLO-660).

A third, from CLO-600 and CLO-625: **nobody can push to `main`**, including the repository owner. Ruleset 20153405 requires the `CI Gate` check with no bypass actors, so every change, docs included, arrives through a pull request.

CLO-374 is Done (see ROADMAP Phase 10).
