# CLO-592 Lessons: Library docs and publish dry-run

## L1 - CARGO_REGISTRY_TOKEN is the variable cargo reads, not CARGO_TOKEN

**Source incident**: CLO-592 publish workflow. The workflow asserted `CARGO_TOKEN` was set, but cargo reads `CARGO_REGISTRY_TOKEN` from the environment. The assertion was checking the wrong variable, so even if a secret existed, a real publish step would authenticate as nobody.

**Rule**: When asserting a token for `cargo publish`, always use `CARGO_REGISTRY_TOKEN` (the variable cargo reads). `CARGO_TOKEN` is not read by cargo and is a common source of confusion.

**How to apply**: In any `publish.yml` or CI step that passes a token to cargo, export it as `CARGO_REGISTRY_TOKEN` or pass it via `--token`. The workflow header comment should document this distinction.

## L2 - Cargo does not support `publish = false` at the `[[bin]]` level

**Source incident**: CLO-592 Phase 4. The design proposed adding `publish = false` to the `silence_probe` `[[bin]]` section to exclude it from the published artifact. Cargo only supports `publish` at the `[package]` level, not per binary target.

**Rule**: To exclude a binary from the published package, either:
- Gate it behind a `required-features` that end users won't enable (e.g., `test-support`)
- Move it to a workspace member crate
- Accept that it will be packaged in the `.crate` archive (Cargo includes all source files)

**How to apply**: Before adding `publish = false` to a `[[bin]]` section, verify it's a valid key in the Cargo manifest reference. If not, document the decision with a rationale comment instead.
