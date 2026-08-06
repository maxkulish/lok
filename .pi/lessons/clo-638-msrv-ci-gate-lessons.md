# Lessons: CLO-638 — MSRV CI Gate

## L1 - `dtolnay/rust-toolchain` does not accept a `toolchain` input

**Source incident:** CLO-638 implement phase. The workflow initially used
`dtolnay/rust-toolchain@c18ebc0` with `with: { toolchain: 1.83 }`, but the
action at that commit SHA does not accept a `toolchain` input (valid inputs
are `targets`, `target`, `components`). The input was silently ignored and
the action installed its default toolchain. Discovered when a proof commit
changed the input to `1.82` and the job still passed.

**Rule:** The `dtolnay/rust-toolchain` action uses the `@` ref to specify
the toolchain version. `@1.83` installs Rust 1.83. Do not pass a
`toolchain` input — it is not a valid parameter. To pin to a commit SHA,
find the SHA of the version branch (e.g. `1.83`) and use
`dtolnay/rust-toolchain@<sha>`.

**How to apply:** When adding a `dtolnay/rust-toolchain` step, verify the
action's README for valid inputs. The `@` ref IS the version selector;
`with: { toolchain: ... }` is not supported.

## L2 - Validation gate catches bedrock MSRV gap

**Source incident:** CLO-638 Codex review. The initial `msrv-check` job
only ran `cargo check --locked --all-targets` (default features), but the
`bedrock` feature pulls in AWS SDK dependencies requiring Rust 1.88. The
Codex review caught this as a HIGH finding. The fix added a second step
that checks `--features bedrock` against 1.88.

**Rule:** When adding an MSRV CI job, check ALL CI-tested feature
configurations, not just default features. An optional feature with a
higher MSRV makes the declared `rust-version` falsifiable if only default
features are tested.

**How to apply:** For each feature that `build-and-test` tests, add a
corresponding MSRV check. If the feature's dependencies require a newer
toolchain, document the per-feature MSRV separately.

## L3 - Negative proof requires a CI failure run

**Source incident:** CLO-638 Qodo review. The plan's ST4 (prove the gate
catches violations) was initially verified only locally (`cargo +1.82.0
check` fails). Qodo correctly flagged that local verification is not
sufficient — the CI workflow must be shown to fail. A temporary commit
changing the toolchain to 1.82, pushed to the branch, produced a failing
CI run (31129038763) with the expected error.

**Rule:** "Prove the gate catches violations" means a CI run that fails,
not a local command that would fail if run in CI. The proof must include
the failing run URL and the error message.

**How to apply:** After adding a CI gate, create a temporary commit that
triggers the failure condition, push to the branch, wait for CI to fail,
capture the run URL and error, then revert. Record the evidence in the PR
body or a review reply.
