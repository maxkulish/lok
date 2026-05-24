The design document for **CLO-391: Claude dual-mode health probe** has been reviewed against the specified criteria.

### Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is thorough, well-researched, and technically sound. It aligns perfectly with the existing architecture and demonstrates a strong understanding of both the problem domain and the codebase. The proposed implementation is robust, safe, and operationally ready. The suggestions below are minor and intended to further refine the implementation.

---

### Key Findings

*   **Completeness & Quality:** The document is exemplary, covering all necessary sections from background and architecture to a detailed, actionable implementation plan. The design choices are well-justified, particularly the decision to implement inline probes and leverage the `HealthStatus.mode` field for disambiguation.
*   **Codebase Alignment:** The proposal integrates seamlessly with existing components like `HealthCache` and the `Backend` trait, correctly identifying and avoiding breaking changes. The analysis of the cache keying strategy was insightful and led to the right, minimally invasive solution.
*   **Concurrency & Safety:** The design correctly specifies the use of `tokio::process::Command` for non-blocking I/O, includes necessary timeouts for external processes, and employs a thread-safe pattern (`OnceLock<Mutex<...>>`) for memoization.
*   **Operational Readiness:** The separation of the fast, offline API check from the slower CLI check is a key operational win. The explicit 2-second timeout on CLI commands prevents the health check system from stalling.

---

### Actionable Items (Suggestions)

1.  **[Low Priority] Refine `probe_api` Logic:**
    *   **Suggestion:** In the provided code snippet for `probe_api`, the checks for the API key and model are in separate `if` blocks that return the same structure. Combine them for conciseness.
    *   **Example:**
        ```rust
        if api_key.expose_secret().is_empty() || model.is_empty() {
            return Ok(HealthStatus { available: false, mode: Some("api".into()), ..Default::default() });
        }
        Ok(HealthStatus { available: true, mode: Some("api".into()), ..Default::default() })
        ```

2.  **[Low Priority] Add a Code Comment Regarding Mutex in Async:**
    *   **Suggestion:** The use of `std::sync::Mutex` within an `async` function (`get_help_output`) is acceptable here as the critical section is very short and non-blocking. However, this can be a footgun. Add a comment to clarify why it's safe here and to warn future maintainers against holding this lock across an `.await` point.
    *   **Example Comment:**
        ```rust
        // NOTE: Using a std::sync::Mutex here is safe because the lock is held
        // for a very short, non-async operation (a HashMap lookup/insert).
        // Do not hold this lock across an `.await` boundary.
        if let Some(cached) = cache.lock().unwrap().get(v) { ... }
        ```

3.  **[Low Priority] Ensure Robust Version Parsing:**
    *   **Suggestion:** The `claude --version` command output format may not be stable. The implementation for `parse_semver_line` should be flexible, for example, by using a regular expression to find a semantic version string (`X.Y.Z`) anywhere in the output, rather than relying on a fixed position on the first line.
