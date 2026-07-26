# Plan: CLO-589 Record the crate-shape ADR for extracting the backend abstraction as a library

## Context

- Design: `docs/designs/clo-589-backend-library-shape.md`
- Discovery: `docs/discovery/clo-589.md`
- PRD: `docs/prds/clo-589-backend-library-shape.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-589/record-the-crate-shape-adr-for-extracting-the-backend-abstraction-as-a
- Chosen approach: decision-only ADR boundary freeze. This ticket changes documentation only; the library extraction belongs to CLO-590.

## Sub-tasks

### ST1 Finalize the backend library-shape ADR

**Files:** `docs/adrs/clo-589-backend-library-shape.md`

Confirm the ADR is marked Accepted and records all decisions from the finalized design: a `[lib]` target in the existing `lokomotiv` package, the library/binary boundary, binary ownership of `Config`, shared package versioning, optional Bedrock exposure, and rejection of a separate `lok-backend` crate. Do not add `src/lib.rs`, modify `Cargo.toml`, or move Rust modules in this ticket.

**Acceptance:** the following command exits successfully:

```bash
python3 - <<'PY'
from pathlib import Path
text = Path("docs/adrs/clo-589-backend-library-shape.md").read_text()
required = [
    "**Status:** Accepted",
    "[lib] target inside the existing `lokomotiv` package",
    "## Boundary allocation",
    "## Config-cycle rule (non-negotiable)",
    "## Versioning and publication decision",
    "## Bedrock feature exposure",
    "## Rejected option: separate `lok-backend` workspace crate",
]
missing = [item for item in required if item not in text]
assert not missing, f"ADR is missing required decisions: {missing}"
PY
```

**Estimate:** S

### ST2 Register the accepted ADR in the repository index

**Files:** `docs/adrs/README.md`

Keep the ADR directory discoverable by listing CLO-589 once in the index table with its Accepted status and by linking to the ADR using a relative path.

**Acceptance:** the following command exits successfully:

```bash
python3 - <<'PY'
from pathlib import Path
text = Path("docs/adrs/README.md").read_text()
assert text.count("| CLO-589 |") == 1, "CLO-589 must appear exactly once in the ADR table"
assert "| Accepted |" in text, "ADR index must record Accepted status"
assert text.count("./clo-589-backend-library-shape.md") == 1, "ADR must have exactly one relative link"
PY
```

**Estimate:** S

### ST3 Verify the frozen boundary against current source and documentation-only scope

**Files:** `docs/adrs/clo-589-backend-library-shape.md`; read-only checks against `src/backend/mod.rs`, `src/backend/context.rs`, `src/config.rs`, and `Cargo.toml`

Check that the principal types and orchestration functions named by the ADR still exist in their documented source modules. Confirm the branch contains no non-documentation changes and no `[lib]` target, preserving the decision-only scope for CLO-589.

**Acceptance:** the following command exits successfully:

```bash
python3 - <<'PY'
from pathlib import Path
import subprocess
backend = Path("src/backend/mod.rs").read_text()
context = Path("src/backend/context.rs").read_text()
config = Path("src/config.rs").read_text()
cargo = Path("Cargo.toml").read_text()
for symbol in ["pub trait Backend", "pub enum BackendError", "pub struct TokenUsage", "pub struct QueryOutput", "pub struct QueryResult"]:
    assert symbol in backend, f"missing documented backend symbol: {symbol}"
for symbol in ["pub struct StepContext", "pub type StepOptions", "pub struct Message"]:
    assert symbol in context, f"missing documented context symbol: {symbol}"
for symbol in ["pub struct Config", "pub struct BackendConfig", "pub struct Defaults"]:
    assert symbol in config, f"missing documented config symbol: {symbol}"
assert "[lib]" not in cargo, "CLO-589 must not add a library target"
status = subprocess.run(["git", "status", "--porcelain"], check=True, capture_output=True, text=True).stdout
paths = [line[3:] for line in status.splitlines() if line]
non_docs = [path for path in paths if not path.startswith("docs/")]
assert not non_docs, f"non-documentation changes are out of scope: {non_docs}"
PY
```

**Estimate:** S

## Pre-merge gate

- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (fmt + clippy + test)

## Risks

- The documented symbol inventory can drift if backend source changes before CLO-590 begins; ST3 detects drift at implementation time.
- Physical placement of `BackendConfig`, `Defaults`, timeout helpers, factories, cache state, and test doubles remains an explicit CLO-590 design concern; this ticket must not silently resolve those implementation questions through code changes.
- A documentation-only change can still coincide with an unrelated repository regression, so the full Rust pre-merge gate remains mandatory.
- Accidental source or manifest edits would expand CLO-589 beyond the accepted decision-only approach; ST3 rejects any non-`docs/` working-tree changes.
