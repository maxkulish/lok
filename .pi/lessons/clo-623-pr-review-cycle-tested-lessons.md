# Lessons: CLO-623 — pr-review-cycle shell snippets executable and tested

## L1 - Gate logic that lives in prose is untested code

**Source incident:** PR #71 (the incident CLO-623 exists to fix). Seven
review findings formed a chain, each link introduced by the fix for the
previous one, because the "code" was bash snippets embedded in
`.pi/skills/pr-review-cycle.md` and `.claude/commands/pr/review.md` that
nothing compiled, executed, or tested. Two of the seven - an invalid
`gh api --jq --arg` invocation and a `jq ... | max` over an empty array
that yields the literal string `null` - would each have been caught by
running the snippet once. Several gates failed **open**: absent or stale
evidence was reported as success.

**Rule:** A merge gate must be executable and tested before the PR
exists, and its tests must assert the failure *direction* (a missing,
empty, malformed or stale input exits non-zero and never reports
success). A gate written only as prose gets no syntax check and no test,
and the markdown is also the medium in which the fix for a failed gate
is written - so the same defect recurs. Extract the gate to a script with
documented exit codes, then keep the markdown as prose plus calls.

**How to apply:** `.pi/scripts/pr-review-cycle.sh` owns the gates behind
six subcommands with exit codes 0-4; `.pi/skills/pr-review-cycle.md` and
`/pr:review` Step 9.5 call it. `.pi/scripts/tests/check-inline-gates.sh`
scans the two markdown files for the banned inline-gate shapes and exits
non-zero if one reappears; the `shell-gates` CI job (Ubuntu + macOS) runs
the linter, the suite and the guard, and is wired into both `CI Gate`'s
`needs` and its assertion loop. When a gate cannot be expressed as a
call, that is the signal it has been inlined rather than extracted.

## L2 - A fixture trimmed to the prose tests the prose, not the API

**Source incident:** CLO-623 implement/ST10. Every login value in the
fake-`gh` fixtures was spelled `qodo-code-review` /
`copilot-pull-request-reviewer`, matching the spelling in the prose,
while the real API returns `qodo-code-review[bot]`. The suite was green
and structurally blind for its whole life; a live probe then found
`request-rereview` rejecting `probe-bots`' own stdout with exit 2,
because the exact identity comparison did not strip the `[bot]` suffix.
Fixed in `a376a57` (suffix stripped; all 26 fixtures moved to the API's
spelling; 2 round-trip tests added).

**Rule:** A recorded fixture is only as trustworthy as its source. If
the fixture is hand-trimmed to match the documentation rather than
captured from the real API, the test validates the documentation against
itself and cannot fail for the reason it exists. That is worse than no
test, because it reads as coverage.

**How to apply:** Take fixture values from the live or serialized API
response, including the parts the prose omits - `[bot]` suffixes, `null`
users, truncated bodies, whole-second timestamps. Add a round-trip test
that feeds one command's real stdout into the next command; the pattern
here is `test_request_rereview_accepts_probe_bots_output_verbatim`, which
runs `probe-bots` and passes its stdout straight to `request-rereview`.
When a fixture must be trimmed, say what was trimmed and why.

## L3 - A skip that exits 0 silently drops coverage

**Source incident:** CLO-623's design assumption was that the zsh leg
"skips gracefully if unavailable". During implementation that was
deliberately reversed: `skip_test` in `.pi/scripts/tests/pr-review-cycle.test.sh`
exits 0, so an absent `zsh` would silently drop
`test_call_site_status_capture_under_zsh` - one of only two call-site
tests, and the one that pins the agent shell that does not word-split.
The CI job now installs `zsh` on Linux and fails loudly if it is missing
rather than letting the skip mask a broken install.

**Rule:** A conditional test that reports success when its precondition
is absent is a gate that can pass without running. In CI, a skip must be
loud - non-zero, or an explicit and counted skip - and the precondition
should be installed rather than assumed. "Skips gracefully" is the
failure mode this rule names, not a mitigation.

**How to apply:** In `.github/workflows/ci.yml`'s `shell-gates` job the
Ubuntu leg runs `apt-get install -y zsh`; do not reintroduce a
success-exit skip for a tool the gate depends on. Same shape applies to
the linter: the macOS leg's `Ensure shellcheck (macOS)` step installs it
when absent, so a missing linter cannot turn the lint step into a no-op.
