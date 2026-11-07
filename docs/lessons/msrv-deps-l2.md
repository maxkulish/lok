---
id: msrv-deps-l2
kind: constraint
scope: ["Cargo.lock minijinja", "src/template/", "dependency and security upgrades"]
evidence: ["Cargo.toml:103", "src/template/context.rs:244", "src/template/context.rs:252", "PR #137"]
learned_at_commit: df48a77
last_verified_commit: null
status: active
supersedes: null
---

# Keep minijinja at 2.19.0 until boolean rendering is handled

`Cargo.toml` accepts any minijinja 2.x, and `Cargo.lock` holds 2.19.0. minijinja 2.24
renders booleans as `True` and `False`. The tests `test_step_success_true` and
`test_step_success_false` expect `true` and `false`, so they fail after a plain
`cargo update`.

Rule: exclude minijinja from bulk lockfile updates. Upgrade it only in a change that
also decides how lok renders booleans.

How to apply: after `cargo update`, check that `Cargo.lock` still has minijinja 2.19.0.
If it does not, run `cargo update -p minijinja --precise 2.19.0`.
