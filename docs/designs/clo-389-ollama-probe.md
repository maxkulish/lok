# Design: CLO-389 - Ollama health probe + ModelInfo + workflow model validation

## Problem
Currently, the Ollama backend has an unconditional `is_available() -> true` stub with no active health probe. This prevents the system from verifying whether Ollama is actually running locally, and which models are pulled/available. We want to implement active probes pinging `/api/version` and `/api/tags` to populate `HealthStatus.models`, allowing workflow step model validation and helpful remediation hints.

## Goals / Non-goals
**Goals**
- Replace Ollama's `is_available() -> true` stub with an active probe pinging `/api/version` and `/api/tags`.
- Populate `HealthStatus.models` with models returned by `/api/tags`.
- Implement model validation inside `Workflow::validate` to reject steps requesting unavailable Ollama models with a clear remediation hint (e.g. `ollama pull <model>`).
- Warm up backends before workflow validation in `validate_workflow`.

**Non-goals**
- Touching other backends (Gemini, Codex, Claude).
- Background polling or daemonizing the health probe.

## Architecture
The health check inside `src/backend/ollama.rs` will perform two `GET` requests using the backend's HTTP client with a 2-second timeout:
1. `GET /api/version` to check version information.
2. `GET /api/tags` to fetch pulled models.

If both requests succeed, we parse the responses and populate the global `BACKEND_CACHE` with a `HealthStatus` containing the fetched models list.

In `src/workflow.rs`, `Workflow::validate` is updated to check if the step requests an `"ollama"` backend and a specific model. If so, it reads the cached health from `BACKEND_CACHE`. If the cached health indicates Ollama is available but the requested model is not found in the pulled models list, validation fails with `WorkflowError::UnknownModel`.
