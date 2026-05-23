# Implementation Plan - CLO-389 (Ollama health probe + ModelInfo + workflow model validation)

Replace Ollama's unconditional `is_available() -> true` with a real probe that pings `/api/version` and `/api/tags`. Populate `HealthStatus.models` so workflow validation can reject steps referencing a model Ollama doesn't have locally, providing a remediation hint.

## User-Facing / Structural Changes

### 1. `src/backend/ollama.rs`
- Define helper structs for deserializing Ollama API responses:
  - `VersionResponse { version: String }`
  - `TagsResponse { models: Vec<super::ModelInfo> }`
- Update `OllamaBackend::health_check` to:
  - Ping `GET /api/version` with a 2-second timeout. If it fails, return `Ok(HealthStatus::new_unavailable())`.
  - Ping `GET /api/tags` with a 2-second timeout. If it fails, return `Ok(HealthStatus::new_unavailable())`.
  - Populate and return `HealthStatus` with the parsed version and models list.

### 2. `src/workflow.rs`
- Add a new error variant `UnknownModel` to `WorkflowError`:
  ```rust
  #[error("Workflow '{workflow}': step '{step}' requests {backend} model '{model}' which is not present in HealthStatus.models\n  remediation hint: ollama pull {model}")]
  UnknownModel {
      workflow: String,
      step: String,
      backend: String,
      model: String,
  },
  ```
- Implement step model validation inside `Workflow::validate`:
  - For each step, get the list of backends via `step.get_backends()`.
  - If a backend is `"ollama"` and `step.model` is `Some(ref model_name)`, check if it exists in the cached `HealthStatus.models`.
  - Retrieve the cached health from `crate::backend::BACKEND_CACHE`. If `health` is present and `available` is true, check if the requested model matches any model in `status.models` (supporting flexible matches like exact name or `:latest` suffix fallback). If not present, return `WorkflowError::UnknownModel`.

### 3. `src/main.rs`
- In `validate_workflow`, ensure backends are warmed up first so that the health cache is populated when `load_workflow` (and thus `Workflow::validate`) runs:
  ```rust
  let _ = backend::Engine::warmup_backends(&config).await;
  ```

## Testing Plan

### 1. Unit Tests in `src/backend/ollama.rs`
- Test deserialization of `/api/tags` response into `Vec<ModelInfo>` correctly.
- Test connection-refused/timeout behavior in `health_check` pings, ensuring `available: false` is set with no panics.

### 2. Unit Tests in `src/workflow.rs`
- Test that workflow validation successfully rejects unknown Ollama models and produces the correct remediation message.
