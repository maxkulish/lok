# Plan: CLO-638 — MSRV CI Gate

## Context
- **Design:** docs/designs/clo-638-msrv-ci-gate.md
- **Discovery:** docs/discovery/clo-638.md
- **Linear:** https://linear.app/cloud-ai/issue/CLO-638/verify-the-declared-rust-version-180-in-ci-or-raise-it-to-what-the
- **Branch:** `feat/clo-638-rust-version`

## Sub-tasks

### ST1 Update `rust-version` in Cargo.toml
**Files:** `Cargo.toml`
**Acceptance:** `grep 'rust-version' Cargo.toml` shows `"1.83"`
**Estimate:** S

Change line 4 from `rust-version = "1.80"` to `rust-version = "1.83"`.

### ST2 Add `msrv-check` job to CI workflow
**Files:** `.github/workflows/ci.yml`
**Acceptance:** `cargo +1.83.0 check --locked --all-targets` passes locally
**Estimate:** S

Add a new `msrv-check` job that:
- Uses `dtolnay/rust-toolchain@1.83` to install the MSRV toolchain
- Shares the cargo cache key with `build-and-test` (`${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}`)
- Runs `cargo check --locked --all-targets`
- Runs on `ubuntu-latest` only

### ST3 Wire `msrv-check` into CI Gate
**Files:** `.github/workflows/ci.yml`
**Acceptance:** CI Gate's `needs` list includes `msrv-check` and the assertion step checks it
**Estimate:** S

Add `msrv-check` to:
- `ci-gate` job's `needs: [build-and-test, library-boundary, msrv-check]`
- The assertion step's `for pair in ...` loop

### ST4 Prove the gate catches violations (then revert)
**Files:** `Cargo.toml` (temporary)
**Acceptance:** CI run shows `msrv-check` job failing with a clear error
**Estimate:** S

1. Temporarily set `rust-version = "1.84"` in `Cargo.toml`
2. Push to a test branch
3. Verify the `msrv-check` job fails
4. Revert the temporary change

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## Risks
- **Cache key collision:** The shared cache key (`${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}`) means the MSRV job and `build-and-test` share the same cache. This is fine — both use the same `Cargo.lock`, and the toolchain is installed separately by `dtolnay/rust-toolchain`. The compiled output is toolchain-specific and cached per-key.
- **Lesson clo-625-l2:** No `paths:` filter on the new job — it must run on every PR to satisfy the required check.
- **Lesson clo-625-l6:** Do not pipe the gate assertion command into `tail` — that replaces the exit status.
- **Lesson clo-625-l7:** The PR file that records its own PR number needs two commits (one to create, one to record the number). Not applicable here — no self-referential file.
