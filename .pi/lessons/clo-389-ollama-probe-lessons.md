# Lessons: CLO-389 Ollama health probe + ModelInfo + workflow model validation

Durable rules from implementing Ollama API health checks, strict workflow model-matching fallback, and thread-safe concurrent test cache synchronization.

---

## L1 - Centralize static test locks for concurrent global state access

**Source incident**: CLO-389 pre-PR checks. Adding `test_ollama_model_validation` to `src/workflow.rs` which cleared and mocked the global static `BACKEND_CACHE` caused parallel backend tests (like `test_retry_wrapper_delegates_health_check` and `test_warmup_batch_writes_all_results`) to fail sporadically due to state leakage.

**Rule**: Any asynchronous/parallel tests that mutate or assert against shared global static state (like a standard `OnceLock` health cache) MUST acquire a shared test mutex to prevent concurrency collisions.

**How to apply**: Centralize a static mutex and an `acquire_test_lock()` guard exposed publicly (under `#[cfg(test)]`) on the state container module. Ensure every test in the workspace that modifies or depends on an empty/known cache state calls `let _guard = acquire_test_lock().await;` and clears/sets the cache at the very start of the test.

---

## L2 - Enforce strict tag mapping for untagged model validations

**Source incident**: CLO-389 pre-PR validation. The initial workflow validator accepted prefix matching for pulled tagged models (e.g. validating untagged `llama3` when only `llama3:8b` was pulled). Codex flagged this as an architectural weakness because Ollama's runtime resolves untagged requests exclusively to `<name>:latest`, which would lead to download or run failures at runtime if the latest tag is missing.

**Rule**: Model validation for local backends (like Ollama) must strictly replicate the backend's tag resolution logic. Untagged model requests must ONLY validate against exact matching tags or the canonical `:latest` variation.

**How to apply**: Restrict the model-matching comparison inside `Workflow::validate` to `m.name == requested_name` or, if the request has no tag, `format!("{requested_name}:latest") == m.name`. Drop broad prefix matching, and back this with clear negative test cases.
