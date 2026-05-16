# Lok ↔ Ollama API Gap Analysis

**Date:** 2026-05-16  
**Ollama version analyzed:** v0.24.0 (latest, released May 14, 2026)  
**Lok backend:** `src/backend/ollama.rs`  
**Author:** AI-assisted analysis

---

## Executive Summary

The `lok` Ollama backend (`src/backend/ollama.rs`) is a thin wrapper around a single API endpoint (`POST /api/chat` with `stream: false`). It sends three fields: `model`, `messages` (one user message only), and `stream`. Meanwhile, Ollama has evolved rapidly through 25+ releases in 2026, adding tool calling, structured outputs, thinking model support, embeddings, multimodality, image generation, and much more.

**Verdict:** The backend is functional for basic text-only chat but has critical gaps in stability, capability awareness, and API surface coverage. Three must-fix stability issues exist. Ten-plus API features are completely unsupported.

---

## 1. Critical Gaps — Unsupported Ollama Features

| Feature | Ollama API | Lok Status | Impact |
|---------|-----------|------------|--------|
| **Tool calling** | `tools` param in `/api/chat` | ❌ Not supported | Can't use function-calling models through Ollama |
| **Structured outputs** | `format` param (JSON schema) | ❌ Not supported | Can't request structured JSON responses |
| **Thinking / reasoning models** | `think` param (bool, "high"/"medium"/"low"/"max") | ❌ Not supported | Can't control reasoning in thinking models (DeepSeek-R1, etc.) |
| **Embeddings** | `POST /api/embed` | ❌ Not supported | Can't generate embeddings at all via Ollama |
| **Image / multimodal** | `images` field in messages | ❌ Not supported | Can't use vision models (llava, gemma3-vision) |
| **Conversation history** | Full messages array | ❌ Single-turn only | Can't maintain multi-turn context |
| **Completion endpoint** | `POST /api/generate` | ❌ Not supported | No completion-mode queries |
| **Image generation** | Experimental image gen models | ❌ Not supported | Can't generate images |
| **Streaming** | `stream: true` | ❌ Not supported | Large responses delayed until complete |

## 2. Functional Gaps — Missing API Controls

| Feature | Ollama API | Lok Status |
|---------|-----------|------------|
| `keep_alive` control | Controls model load duration (default 5m) | ❌ Not sent — uses server default |
| `options` (temperature, top_p, etc.) | Per-request model parameters | ❌ Not supported |
| `format: "json"` | Simple JSON mode | ❌ Not supported |
| `raw` mode | Bypass prompt templating | ❌ Not supported |
| `suffix` | Text after model response | ❌ Not supported |
| Model listing | `GET /api/tags` | ❌ No model discovery |
| Running models | `GET /api/ps` | ❌ No running model awareness |
| Version check | `GET /api/version` | ❌ No version compatibility check |
| Model management | Create / Copy / Delete / Pull / Push | ❌ No model management |

## 3. Stability Issues — Lok → Ollama Interaction

### 3.1 `is_available()` is a no-op (🔴 Critical)

```rust
fn is_available(&self) -> bool {
    // Ollama is a server, not a CLI. Can't easily check synchronously.
    // Return true and let runtime connection fail if not running.
    true
}
```

This always returns `true`. Lok will attempt queries against a non-running server and fail at runtime with a generic connection error instead of surfacing a clear "Ollama is not running" message. Since `Backend` is synchronous, a real check would need to be async — but the `is_available()` contract is sync. This is a trait design issue as much as an implementation gap.

**Fix:** Either make `is_available()` async in the trait, or cache an async health-check result (e.g., a `tokio::sync::OnceCell<bool>` updated on first query attempt).

### 3.2 No server error recovery (🟡 Medium)

- 5xx server errors are treated as fatal `BackendError::ExecutionFailed` (non-retryable)
- The retry executor (`RetryExecutor`) only retries `Timeout`, `RateLimit`, and `Network` errors
- A transient Ollama server overload gets no retry
- No handling of `done_reason: "length"` (truncated output that should be retried with higher `num_predict`)

### 3.3 No keep_alive / timeout coordination (🟡 Medium)

- Lok's default timeout is 300s
- Ollama's default `keep_alive` is 5min
- If lok times out, the model stays loaded in memory for 5 more minutes — wasting VRAM
- No mechanism to cancel the model unload when lok retries

### 3.4 Default model is `llama3.2` (🟡 Medium)

```rust
let model = config.model.clone().unwrap_or_else(|| "llama3.2".to_string());
```

`llama3.2` (3B params) may not be pulled in newer Ollama installations. A better approach: detect available models from `/api/tags` or let the config be explicit without a questionable fallback.

### 3.5 No `done_reason` handling (🟢 Low)

The response includes `done_reason` ("stop", "length", "load", "unload") but lok ignores it entirely. A "length" completion indicates the response was truncated by `num_predict` and the query should be retried or the context window increased.

## 4. Backward Compatibility Assessment

**Current is safe but fragile.**

**Safe:**
- Minimal request shape (`model`, `messages`, `stream: false`) will continue to work
- `#[serde(default)]` on optional response fields handles new fields gracefully
- Serde ignores unknown fields in responses

**Fragile:**
- If `model` is not found or not loaded, the error text is passed through but not classified by `BackendError::from()` — it lands as a generic `ExecutionFailed` instead of something actionable
- Future Ollama may deprecate implicit `keep_alive` defaults
- A model rename or deprecation (e.g., `llama3.2` → `llama3.2-vision`) silently breaks queries

## 5. Recommended Improvements

### 5.1 Immediate — Low Effort, High Impact

1. **Real availability check** — Cache an async `/api/version` ping result that `is_available()` reads
2. **Add `keep_alive` to requests** — Match lok's per-backend timeout so models unload promptly
3. **Classify common Ollama error responses** — Map "model not found", "no model loaded", "connection refused" to typed `BackendError` variants
4. **Add model existence check** — Query `/api/tags` and warn if the configured model isn't pulled

### 5.2 Medium-Term — Moderate Effort

1. **Support conversation history** — Pass the full message array from lok's conductor to enable coherent multi-step agent conversations
2. **Support `options` passthrough** — Expose `temperature`, `top_p`, `num_ctx`, etc. via `lok.toml` backend config and pass them as the `options` dict
3. **Handle `done_reason: "length"`** — Automatically retry with higher `num_predict` when response is truncated

### 5.3 Long-Term — Strategic

1. **Streaming support** — Implement a streaming variant that yields tokens as they arrive, preventing timeouts on long responses
2. **Tool calling** — If lok ever wants Ollama-hosted models to participate in its agent framework with tool capabilities, this is necessary
3. **Structured outputs** — The `format` parameter with JSON schema would allow lok to request typed responses directly, reducing parsing errors
4. **Backend capability advertising** — Let each backend declare: supports_tools, supports_thinking, supports_structured_outputs, supports_vision, etc., so the conductor can route appropriately

## 6. Summary Matrix

| Area | Verdict |
|------|---------|
| **Does lok need updates?** | **Yes** — The Ollama backend is overly minimal and misses 10+ API features |
| **Must-fix for stability?** | **Yes** — `is_available()` lies, no health check, no keep_alive coordination |
| **Backward compat risk?** | **Low** but real — Minimal request format is safe; error classification is fragile |
| **Biggest blind spot** | No awareness of model capabilities (thinking, tools, vision) — lok assumes every model is a simple text-only chat model |
| **Most impactful single fix** | Real `is_available()` + connection error classification — turns silent failures into actionable errors |
