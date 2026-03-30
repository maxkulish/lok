# Design Review: clo-181

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-03-30
**Pipeline**: lok design-review

---

The review is valid. Here is the clean review content:

---

## 1. Completeness Check

**Sections Present**:
- Problem Statement: Clear, well-researched, references actual code line numbers
- Acceptance Criteria: Detailed, testable success metrics
- Constraints: Good separation of must/must-not/prefer
- Decomposition: Logical task breakdown with dependency order
- Evaluation: Comprehensive test table with edge cases

**Sections Missing**:
- No Architecture/Background section (not strictly needed for a small change)
- No Implementation Plan (Decomposition serves this purpose)

Overall: **Well-structured for the scope**. This is a focused change, so detailed sections like architecture aren't necessary.

---

## 2. Architecture Assessment

**Strengths**:
- Clean interface change: `Option<&str>` is the right choice (borrowed, zero-cost)
- Backward-compatible: all existing call sites pass `None`, preserving current behavior
- Proper constraint: no changes to `BackendConfig` or `QueryOutput` - keeps blast radius contained
- Good decomposition order: trait change first, then backends in parallel, then call sites

**Concerns**:
1. **Gemini/Codex silent ignore**: The design says these backends should "accept the parameter but use their defaults (no CLI flag for model)". This is fine but should be documented as intentional limitation in user-facing docs.
2. **No validation of model string**: If `model = "invalid-model-name"` is passed, the error will come from the backend API/CLI, not from Lok. This is acceptable but worth noting.

---

## 3. Codebase Alignment

**Verified Against Source**:

| Claim in Design | Codebase Verification | Status |
|----------------|----------------------|--------|
| `Step` has no `model` field | `src/workflow.rs:249-289` - confirmed no `model` field | Verified |
| `Backend::query()` takes `(prompt, cwd)` | `src/backend/mod.rs:53` - signature matches | Verified |
| Claude API model at init | `src/backend/claude.rs:66-102` - confirmed | Verified |
| Claude CLI model from config | `src/backend/claude.rs:156-158` - confirmed | Verified |
| Ollama model from `self.model` | `src/backend/ollama.rs:67` - confirmed in `ChatRequest` | Verified |
| Bedrock model from `self.model_id` | `src/backend/bedrock.rs:107` - confirmed | Verified |
| 13 call sites across 7 files | Verified via grep: workflow (6), conductor (1), spawn (2), debate (1), team (2), backend/mod.rs (1) | Verified |

**Pattern Compliance**:
- Uses `Option<&str>` for borrowed optional parameter (matches codebase style)
- Uses `#[serde(default)]` for new Step field (matches existing fields)
- Model stored at construction for API backends (existing pattern)

**Violations**: **None found**

---

## 4. Security Review

| Concern | Assessment |
|---------|------------|
| API keys use `SecretString` | Verified in `claude.rs:66` - `api_key: SecretString` |
| No hardcoded secrets | All keys from env vars or config |
| Input validation for subprocess | Model string passed to CLI as `--model {m}` - safe since `Command::arg()` handles escaping |
| Path traversal | N/A - no file writes |
| Model string validation | No validation, but backend will reject invalid models |

**Security Verdict**: **Acceptable**. The model string is passed through `Command::arg()` which handles escaping properly.

---

## 5. Implementation Concerns

1. **Decomposition is correct**: Task ordering properly captures dependencies.
2. **Call site counts match**: Design lists 13 sites, code verification confirms 13.
3. **Constraint alignment**:
   - `run_query_with_config` signature unchanged (design explicitly forbids this change)
   - `QueryOutput` unchanged (already has `stdout`, `stderr`, `exit_code`)
   - `BackendConfig` unchanged (model override is step-level, not config-level)
4. **Testing strategy**: Verification method is sound (`cargo test && cargo clippy -- -D warnings && cargo build --features bedrock`).

---

## 6. Blind Spots

### What's Missing:

1. **Empty string handling**: Edge case `model = ""` in TOML. Design mentions it should behave like `None`, but no implementation guidance. Recommend: backends should treat `Some("")` same as `None`.
2. **Thread safety**: The `model` parameter is `&str` (borrowed), which is `Send`. No issues across `tokio::spawn` boundaries.
3. **Logging/observability**: No mention of logging when model override is applied. Should log `Using model override: {model}` for debugging.
4. **`query_with_system` not mentioned**: `claude.rs:181` has `query_with_system()` used by conductor. This method is NOT part of the `Backend` trait, so it's out of scope, but conductor's direct backend usage (`backend.query(prompt, cwd).await?` at line 185) IS in scope.
5. **Consensus step behavior**: Design says "model override applies to all backends in the fan-out" - correct, but should note that consensus steps with multiple backends will use the SAME model for all. This is intentional but worth documenting.
6. **No mention of caching**: `cache.rs` is not mentioned, but since the model override is at query time and doesn't affect `QueryResult`, caching is unaffected.
7. **Bedrock model format**: Bedrock model IDs are different from Claude API model strings (e.g., `us.anthropic.claude-sonnet-4-20250514-v1:0` vs `claude-sonnet-4-20250514`). Users might pass wrong format. Consider documenting this.

---

## 7. Verdict

**APPROVE**

The design is well-researched, correctly scoped, and aligns with existing codebase patterns. The trait change is minimal, backward-compatible, and the decomposition is properly sequenced. Call site counts are accurate. All critical constraints preserve blast radius containment.

---

## 8. Actionable Feedback

**P0 (Should add before implementation)**:
1. Add guidance for empty string handling: `model = ""` should be treated as `None` by backends. Add a helper function `fn normalize_model(model: Option<&str>) -> Option<&str>` that filters empty strings.
2. Add logging recommendation: Backends should log when model override is applied:
   ```rust
   if let Some(m) = model { tracing::debug!("Using model override: {}", m); }
   ```

**P1 (Nice to have)**:
3. Add note about Bedrock model ID format difference from Claude API.
4. Add note that `cache.rs` is unaffected (for reviewer confidence).
5. Add test case for consensus step with model override (verify all backends receive same model).

**P2 (Documentation)**:
6. Consider user-facing documentation: "Gemini CLI and Codex CLI do not support model override and will use their configured defaults."
