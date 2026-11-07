---
id: msrv-deps-l1
kind: process
scope: ["Cargo.lock", "Cargo.toml rust-version", "bedrock feature", "dependency and security upgrades"]
evidence: ["Cargo.toml:5", ".github/workflows/ci.yml:66", ".github/workflows/ci.yml:92", "PR #138"]
learned_at_commit: df48a77
last_verified_commit: null
status: active
supersedes: null
---

# Resolve the bedrock tree and the default tree at their own MSRVs

The default build needs Rust 1.83, and the `bedrock` feature needs Rust 1.91.1. Both
trees share one `Cargo.lock`, and the CI job `MSRV check (1.83)` builds both. A plain
`cargo update` of the AWS crates raised shared crates such as tokio, icu and indexmap
above 1.83. A fallback resolve of the whole tree at 1.83 fixed that, but it also
downgraded `time` below a security fix.

Rule: resolve the AWS crates at 1.91.1 first. Then re-resolve only the default-tree
crates that now need more than 1.83, at 1.83.

How to apply:

1. Set `rust-version = "1.91.1"` in `Cargo.toml` for the moment.
2. Run `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update -p aws-config -p aws-sdk-bedrockruntime --recursive`.
3. Restore `rust-version = "1.83"`.
4. List the crates in the default tree whose `rust_version` in `cargo metadata` is above
   1.83. Run `cargo update -p <crate>` for each, with the same environment variable.
5. Compare the security-relevant crates with `main` before you commit.
