# Pre-PR validation: clo-638

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-08-06
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Full review completed; findings independently spot-checked against repo state (see below) |
| Claude fallback | SKIPPED | Codex review succeeded, fallback not invoked |

I independently verified Codex's four findings against the working tree, `Cargo.lock`, and the registry-cached crate manifests — all four hold up:

- **bedrock MSRV gap confirmed**: `.github/workflows/ci.yml:53-57` shows `build-and-test` lints and tests the `bedrock` feature, but `msrv-check` (lines 65-84) only runs `cargo check --locked --all-targets` with default features — no `--features bedrock`. `aws-config-1.8.12` and `aws-sdk-bedrockruntime-1.122.0` (the exact locked versions per `Cargo.lock`) both declare `rust-version = "1.88"` in their registry manifests. So `msrv-check` can pass while the crate's own CI-tested `bedrock` configuration would reject the declared 1.83 toolchain.
- **ST4 proof missing confirmed**: `git log main..HEAD` shows only two commits (`84dff5e`, `e8ba32e`); the plan's ST4 (deliberate failure + revert, `docs/plans/clo-638-msrv-ci-gate.md:38-46`) has no corresponding commit or recorded CI run anywhere.
- **Mutable action ref confirmed**: `ci.yml:16-17` states third-party actions must pin a full commit SHA, and `release.yml:192` follows that (`softprops/action-gh-release@3d0d98...`), but the new `dtolnay/rust-toolchain@1.83` (`ci.yml:72`) uses a floating version tag, breaking the file's own stated convention.
- **Status file inconsistency confirmed**: `git diff -- docs/status/clo-638-workflow.yaml` shows the "All sub-tasks landed" / `assumptions_revalidated: true` state is uncommitted; `HEAD`'s committed version still shows `implement: pending` with only `84dff5e` recorded.

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- Close the bedrock MSRV gap: either (a) add a bedrock-scoped MSRV check (e.g. a second job or step gating `cargo check --features bedrock` against the actual MSRV that feature needs, likely 1.88), or (b) explicitly document that `rust-version = "1.83"` covers default-feature builds only and note bedrock's real floor separately — whichever is chosen, the declared MSRV must stop being falsifiable by a CI-tested feature.
- Complete ST4: push a deliberate failure (temporary `rust-version = "1.84"` bump or equivalent), confirm `msrv-check` goes red, revert, confirm green — and record the evidence (run URL or log) rather than leaving it asserted-but-unproven.
- Pin `dtolnay/rust-toolchain@1.83` to a full commit SHA, consistent with the header comment's stated policy and `release.yml`'s existing practice.
- Commit an accurate `docs/status/clo-638-workflow.yaml` only after the above land — don't leave "All sub-tasks landed" claimed while ST4 is outstanding and the file itself uncommitted.

## Out of Scope / Deferred
- Keying cached `target` artifacts by toolchain to avoid stable/MSRV cache cross-use (Codex recommendation #4) — a reasonable hygiene improvement, but not required for correctness of the gate; the toolchain is installed separately per job regardless.

## False Positives / Tooling Artifacts
- None. All four Codex findings were independently confirmed against the actual repo state.

## Recommendation
PROCEED_WITH_FIXES: (1) resolve the bedrock/MSRV mismatch by adding a bedrock-scoped MSRV gate or explicitly scoping the 1.83 claim to default features, (2) execute and record the ST4 negative-proof CI run, (3) pin `dtolnay/rust-toolchain` to a commit SHA, (4) commit a truthful workflow status once the above are evidenced. All four are mechanical and fit within one implementation pass by the task's existing executor — no user-level scope decision is required, though whichever bedrock-MSRV option is chosen should be a deliberate one-line call, not silently punted again.
