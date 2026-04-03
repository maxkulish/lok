# Design Review: clo-184

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-04-03
**Pipeline**: lok design-review

---

**VERDICT: NEEDS_REVISION**

### Key Findings
The design document is comprehensive, well-architected, and aligns smoothly with Lok's existing layered validation approach. However, there are a few critical flaws related to LLM output behavior, prompt interpolation, and parsing fallback logic that must be addressed to prevent dangerous false positives and injection bugs.

### Prioritized Actionable Items

#### 1. CRITICAL: Dangerous Fallback Logic in Parsing
**Issue:** In `parse_validation_response`, if the LLM response is neither valid JSON nor prefixed with `REVIEW_FAILED:`, the system defaults to treating the entire response as a "pass" and uses it as the `cleaned_output`. If the LLM generates a refusal (e.g., "I cannot fulfill this request") or goes off the rails, this will incorrectly **pass** validation and silently replace the actual step output with the refusal string.
**Recommendation:** Validation must be fail-closed. If the model fails to return a recognized valid response format, it should result in a `ValidatorError` indicating a malformed validation response, rather than defaulting to a pass.

#### 2. CRITICAL: JSON Parsing Resilience (Markdown Fences)
**Issue:** LLMs frequently wrap JSON responses in Markdown code blocks (e.g., ````json\n{"status": "pass"}\n````). `serde_json::from_str` will fail to parse this. Combined with the dangerous fallback logic above, a valid JSON response wrapped in markdown will fail to parse and fall through to the plain-text branch, which then sets the step's cleaned output to include the markdown fences.
**Recommendation:** Add logic to strip Markdown code fences (e.g., ````json` and ````) from the response before attempting to parse it with `serde_json`.

#### 3. CRITICAL: Prompt Interpolation Vulnerability
**Issue:** `interpolate_validation_prompt` uses sequential string `.replace()` calls:
```rust
let result = prompt.replace("{{ output }}", &truncated_output);
match stderr {
    Some(s) => result.replace("{{ stderr }}", s),
    // ...
}
```
If the untrusted `output` contains the literal string `"{{ stderr }}"`, the second `.replace()` will evaluate that placeholder and overwrite that part of the output with the actual stderr. This alters the text the LLM is supposed to validate.
**Recommendation:** Perform a single-pass string replacement (e.g., using a regex or manual string builder) to ensure placeholders injected from earlier evaluation are not recursively expanded.

#### 4. SUGGESTION: Truncation vs. Output Replacement Data Loss
**Issue:** When `max_input_length` is hit, the prompt appends `[TRUNCATED ...]`. If the LLM is acting as a semantic judge and `replace_output = true` is set, the LLM's returned "cleaned" output will likely only contain the truncated portion. This means the system will silently discard the latter half of the step's raw output.
**Recommendation:** Explicitly document this risk, or add a safety constraint: if `replace_output` is true, either bypass truncation, log a loud warning, or fail the validation step if truncation occurs to prevent silent data loss.

#### 5. SUGGESTION: Validation-Specific Timeouts
**Issue:** The design inherits the validation timeout from the backend's configuration. If the backend defaults to a long timeout (e.g., 5 minutes for generation tasks), the validation step will hang for 5 minutes if the API stalls. Validation steps using cheap/fast models should fail fast.
**Recommendation:** Consider adding an optional `timeout_ms` field to `ValidateConfig` to allow the validation check to fail fast independently of the backend's global default.
