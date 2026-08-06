# Pre-PR validation: clo-638

**Reviewer**: Codex (gpt-5.6-sol)
**Validated**: 2026-08-06
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- **HIGH — The declared MSRV is false for the supported `bedrock` feature.** The new job runs only default features ([ci.yml](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/.github/workflows/ci.yml:84)), while `bedrock` enables AWS dependencies ([Cargo.toml](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/Cargo.toml:68)). Locked `aws-config` and `aws-sdk-bedrockruntime` require Rust 1.88 ([aws-config manifest](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/aws-config-1.8.12/Cargo.toml:14), [Bedrock manifest](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/aws-sdk-bedrockruntime-1.122.0/Cargo.toml:14)). Existing CI explicitly treats `bedrock` as a supported configuration that must be tested ([ci.yml](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/.github/workflows/ci.yml:51)). Consequently, `CI Gate` can pass even though `cargo check --features bedrock` rejects Rust 1.83.

- **HIGH — The required failure proof is absent and internally inconsistent.** ST4 requires a failing CI run ([plan](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/docs/plans/clo-638-msrv-ci-gate.md:38)), but branch history contains only the permanent manifest and workflow commits. No run URL or failure evidence is recorded. The design also alternates between proving a post-MSRV API violation and bumping the root manifest ([design](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/docs/designs/clo-638-msrv-ci-gate.md:86)); the latter would fail because the root package declares 1.84, not because a dependency "requires 1.83."

- **MEDIUM — A third-party action is referenced by a mutable tag.** The workflow states that third-party actions must use full commit SHAs ([ci.yml](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/.github/workflows/ci.yml:16)), but adds `dtolnay/rust-toolchain@1.83` ([ci.yml](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/.github/workflows/ci.yml:72)). This leaves the CI runner exposed to action-ref replacement.

- **LOW — Workflow status is unreliable.** The working copy claims "All sub-tasks landed" ([status YAML](/Users/mk/Code/orchestrator/lok--feat-clo-638-rust-version/docs/status/clo-638-workflow.yaml:184)) despite no ST4 record, and these status updates are uncommitted. The version at `HEAD` still marks implementation pending and records only commit `84dff5e`.

`actionlint`, YAML parsing, `cargo fmt --check`, and `git diff --check` passed. No Rust source, secret handling, or process-spawning code changed.

## Missing Items

- An MSRV decision and gate covering the supported `bedrock` feature.
- ST4's negative CI proof, followed by a clean post-revert run.
- Evidence that `msrv-check` runs successfully and causes `CI Gate` to fail when it fails.
- Committed, accurate workflow status and independently verified clippy/test results.

## Recommendations

1. Raise the package-wide MSRV to at least 1.88 and run `cargo check --locked --all-targets --all-features`, or pin AWS dependencies to versions compatible with 1.83. The former is cleaner because `bedrock` is already treated as supported.
2. Pin `dtolnay/rust-toolchain` to a reviewed commit SHA and pass the exact toolchain through its input.
3. Temporarily introduce an API stabilized after the chosen MSRV in a target compiled by the job. Record the failing run, revert it, then record a green `msrv-check` and `CI Gate`.
4. Key cached `target` artifacts by toolchain; only registry/git downloads should be shared across stable and MSRV jobs.
5. Correct and commit the workflow status only after all four subtasks are evidenced.
