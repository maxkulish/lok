# Lessons: CLO-388 FR-9a + FR-10 + FR-15 Engine Warmup & Health Cache

Durable rules from implementing a parallel async warmup pipeline and a thread-safe health cache.

---

## L1 - Inlined backend creation prevents cascading warmup failures across healthy backends

**Source incident:** CLO-388 PR review cycle. The initial implementation of `warmup_backends` fetched all enabled backends via a helper `get_all_enabled_backends` which used the `?` operator. If even a single backend failed initialization (e.g., due to a missing command or configuration error), the entire warmup failed with `Err`, preventing other perfectly healthy and configured backends from warming up. 

**Rule:** A warmup pipeline should be resilient to individual backend construction and probe failures. Backend creation should be inlined inside the loop, and errors should be handled gracefully (e.g., via warnings/logs) rather than short-circuiting.

**How to apply:** In loops fanning out over parallel initializations or health checks, inline the initialization logic. Match on `create_backend` results and only spawn futures for successfully constructed backends, recording warnings for others, rather than bubbling up errors using `?`.

---

## L2 - Always handle RwLock write-lock poisoning explicitly instead of silently ignoring failures

**Source incident:** CLO-388 PR review cycle. The initial implementation of `warmup_backends` checked write lock access via `if let Ok(mut lock) = cache.write()`, silently ignoring the failure if the lock was poisoned. In a single-execution CLI tool, a poisoned lock means state has been corrupted and the program should fail fast rather than silently failing to update the health cache.

**Rule:** Shared cache accesses on critical startup paths should explicitly handle lock poisoning (e.g., via `.expect()` or `.unwrap()`) to fail fast and prevent silent state discrepancies.

**How to apply:** Replace silent `if let Ok` write checks with `cache.write().expect("lock poisoned")` to ensure any thread crash or state corruption is caught immediately.
