# Design Review: clo-182

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-03-31
**Pipeline**: lok design-review

---

The review is **VALID**. It contains:
- A clear verdict (`APPROVE_WITH_SUGGESTIONS`)
- 4 numbered sections with assessment language
- Actionable feedback with specific options

Here is the clean review content:

---

### Verdict: APPROVE_WITH_SUGGESTIONS

The design is highly detailed, well-researched, and effectively bridges the gap between the `QueryOutput` introduced in CLO-180 and the validation pipeline coming in CLO-183. The explicit mapping of how different execution paths (single-backend, shell, consensus, for-each) thread (or don't thread) the new metadata is excellent. 

However, there is a **critical UX regression blind spot** regarding how shell execution outputs are handled during this intermediate phase.

### 1. Critical UX/Visibility Regression (Operational Readiness)
**Finding:** The design splits `stderr` from `stdout` in `run_shell()` (which is correct architecturally), but the "Follow-Up Notes" explicitly defer updating `print_results()` and `format_results()` to show `stderr` to a future task. 
**Impact:** If this is merged as-is, shell scripts that fail and print to `stderr` will suddenly look like they have "empty" outputs. `stderr` will be swallowed by the engine and invisible to the user until the follow-up task is implemented.
**Action:** You must tie the visibility of `stderr` to the separation of `stderr`. Either:
*   **Option A (Recommended):** Bring the `print_results()` and `format_results()` updates into the scope of CLO-182 so `stderr` is rendered immediately upon separation.
*   **Option B:** Defer the `run_shell()` behavior change to the same follow-up task that handles the display logic.

### 2. ValidationResult Cardinality (Architecture/Blind Spot)
**Finding:** `ValidationResult` has a single `validator: String` field. If a step supports multiple chained validators (e.g., `not_empty` followed by an `llm` evaluation), it's ambiguous what `validator` holds.
**Action:** Clarify the semantics in the document. 
*   If validation short-circuits on the first failure, does `validator` only hold the name of the *failed* validator? 
*   If all validators pass, what goes in `validator`? (e.g., `"all"`, or a comma-separated list?) 
*   *Suggestion:* Rename to `failed_validator: Option<String>` to make it clear that it represents the overall validation phase's failure point, or change it to `validators_run: Vec<String>`.

### 3. Ambiguity of `exit_code: None` (Edge Cases)
**Finding:** The doc correctly notes that Unix processes killed by a signal will return `exit_code: None`. However, API backends (like Claude/Bedrock) also naturally have `exit_code: None`. This conflates "Not applicable to this backend" with "Applicable, but violently terminated by the OS".
**Action:** In `run_shell()`, if `output.status.code()` is `None` but `!output.status.success()`, leverage `std::os::unix::process::ExitStatusExt` to check for a terminating signal. Inject a synthetic message into `stderr` (e.g., `"Process terminated by signal: {signal}"`) to ensure signal kills don't become silent, indistinguishable failures.

### 4. Construction Site Boilerplate (Code Quality)
**Finding:** The document states: *"Prefer: Use explicit `None` at construction sites over a Default impl"*. While explicit intent is generally good, updating 33 construction sites manually to add 4 `None` fields is brittle. Every future extension of `StepResult` will require a similarly massive blast radius.
**Action:** I agree with creating the `StepResult::error()` constructor for the 16 error paths. Consider also implementing `Default` for the extension fields and using struct update syntax (`..Default::default()`), or introducing a `StepResult::success()` constructor. This will drastically reduce churn for future metadata fields.
