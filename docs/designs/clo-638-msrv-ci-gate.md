# Design: MSRV CI Gate — CLO-638

## Problem

`Cargo.toml` declares `rust-version = "1.80"` but no CI job builds against it. The local toolchain is 1.95, `ci.yml` uses runner-stable, and no job pins a specific version. The claim is unverified — any API stabilised after 1.80 compiles clean on every machine that touches this repo and breaks only for a consumer on an older toolchain.

Discovery (see `docs/discovery/clo-638.md`) confirmed that **1.80 does not build**. Transitive dependencies (ICU4X via `reqwest → url → idna → idna_adapter`) require Rust 1.83. The actual minimum is **1.83**, and no source code in `src/` uses any post-1.83 API.

## Goals / Non-goals

### Goals
- Update `rust-version` in `Cargo.toml` from `"1.80"` to `"1.83"` to match reality
- Add a CI job that builds the crate against the declared MSRV (1.83) and fails when code or dependencies need a newer toolchain
- Wire the new job into the `CI Gate` required-check aggregation so it cannot be ignored
- Prove the gate works by adding a deliberate post-MSRV API usage that fails the job (then revert)

### Non-goals
- Not changing the dependency tree to squeeze the MSRV lower (Approach B was considered and rejected in discovery)
- Not adding `cargo-msrv` or other tooling to CI
- Not changing how local development works (no `rust-toolchain.toml` file)
- Not changing the release or publish workflows (they use runner-stable, which is fine)

## Architecture

### Changes to `Cargo.toml`

One line change:
```toml
rust-version = "1.83"    # was "1.80"
```

### New CI job: `msrv-check`

A new job in `.github/workflows/ci.yml`:

```yaml
msrv-check:
  name: MSRV check (1.83)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4

    - name: Install MSRV toolchain
      uses: dtolnay/rust-toolchain@1.83

    - name: Cache cargo
      uses: actions/cache@v4
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

    - name: Check against MSRV
      run: cargo check --locked --all-targets
```

Key design decisions:
- Uses `dtolnay/rust-toolchain@1.83` (pinned to the exact MSRV version) rather than `rustup toolchain install` — cleaner, handles caching, and is the standard approach
- Runs `cargo check --locked --all-targets` — checks the library, all binaries, and all tests against the locked dependency versions
- Shares the cargo cache key with `build-and-test` — both jobs use the same `Cargo.lock`, so a shared key avoids redundant downloads without cross-contamination (the toolchain is separate)
- Runs on `ubuntu-latest` only — the MSRV is platform-independent, and macOS would add cost for no additional signal

### CI Gate update

The `CI Gate` job's `needs` list gains the new job:

```yaml
ci-gate:
  name: CI Gate
  needs: [build-and-test, library-boundary, msrv-check]
  if: always()
  ...
```

The assertion step in `ci-gate` also gains the new job:

```bash
for pair in \
  "build-and-test:${{ needs.build-and-test.result }}" \
  "library-boundary:${{ needs.library-boundary.result }}" \
  "msrv-check:${{ needs.msrv-check.result }}"; do
```

### Proof: deliberate post-MSRV API

To prove the gate catches violations, a temporary commit will bump `rust-version` to `"1.84"` (a version higher than the actual MSRV). This is simpler than adding a post-1.83 API usage — no code change needed, and the failure is deterministic: `cargo check` will fail because the locked dependency tree requires 1.83.

The proof will be:
1. Temporarily set `rust-version = "1.84"` in `Cargo.toml`
2. Push to a test branch
3. Verify the MSRV job fails with a clear error about the dependency requiring 1.83
4. Revert the temporary change

## Assumptions

| Text | Confidence | Verification |
|------|-----------|-------------|
| `dtolnay/rust-toolchain@1.83` resolves to a toolchain that can compile the crate's dependency tree | High | Verified locally: `cargo +1.83.0 check --locked --all-targets` passes |
| The MSRV is platform-independent (no platform-specific dependencies require a newer Rust on macOS vs Linux) | High | The ICU4X dependency chain is the same on all platforms; `cargo check` on aarch64-darwin with 1.83 passes |
| Adding a new job to `CI Gate`'s `needs` list does not break the `if: always()` semantics | High | The existing pattern with `build-and-test` and `library-boundary` already works this way |
| No `paths:` filter is needed on the new job (per lesson clo-625-l2) | High | The job must run on every PR to satisfy the required check |
| The shared cache key does not cause cross-contamination between the MSRV job and build-and-test | High | Both jobs use the same Cargo.lock; the toolchain is installed separately by dtolnay/rust-toolchain, so the compiled output is toolchain-specific and cached per-key |

## Test plan

### Unit / integration
- No Rust code changes, so no new unit tests needed
- Existing tests continue to pass (verified with `cargo +1.83.0 test --locked`)

### CI verification
1. Push the branch with the CI changes
2. Verify the `msrv-check` job appears in the CI run and passes
3. Verify the `CI Gate` job shows all three required jobs as green
4. Create a temporary commit that sets `rust-version = "1.84"` (or uses a post-1.83 API) and push to a test branch
5. Verify the `msrv-check` job fails
6. Revert the temporary commit

### Manual verification
- `cargo +1.83.0 check --locked --all-targets` — already verified during discovery
- `cargo +1.83.0 test --locked` — verify tests pass against the MSRV

## Migration / rollout

This is a single PR. No staged rollout needed. The change is:
1. Update `Cargo.toml` (`rust-version`)
2. Update `.github/workflows/ci.yml` (new job + CI Gate update)
3. Push, verify CI passes
4. Merge

After merge, the `CI Gate` required check on `main` will automatically include the new job (the ruleset checks the job name, and the job name is `CI Gate` — the internal `needs` list is part of the workflow definition, not the ruleset).

## Open questions

None. The approach is straightforward and the verification is complete.
