# Gap Analysis: ollama-cli vs lok's Ollama Backend

**Date:** 2026-05-16
**Source:** [masgari/ollama-cli](https://github.com/masgari/ollama-cli) — a standalone Go CLI for remote Ollama servers (v0.1.4, uses `github.com/ollama/ollama/api` v0.23.2)

---

## What Each Tool Is

| Tool | Language | Purpose |
|------|----------|---------|
| **lok** (Rust) | Multi-LLM orchestration | Routes prompts to multiple backends, synthesizes results, runs declarative workflows |
| **ollama-cli** (Go) | Standalone Ollama client | Manages remote Ollama servers without installing Ollama locally |

They serve different roles but **overlap at the Ollama HTTP API layer** — and that's where lok has gaps.

---

## Current State of lok's Ollama Backend

`src/backend/ollama.rs` sends only the most minimal request:

```rust
ChatRequest {
    model: ...,
    messages: vec![ChatMessage { role: "user", content: prompt }],
    stream: false,
}
```

No system prompts. No options. No conversation history. No streaming. No auth headers.

---

## What ollama-cli Does That lok Doesn't

### 🚨 High Impact Gaps

#### 1. Richer `/api/chat` Requests
ollama-cli sends full chat requests with:
- **System prompts** (with security guardrails)
- **Conversation history** (multi-message arrays)
- **Model options** (temperature, top_p, etc.)
- **Image input** (multimodal models)
- **Streaming / non-streaming control**

#### 2. Custom HTTP Headers & Auth
ollama-cli supports per-config custom headers via `headerTransport` wrapping the HTTP transport. Useful for:
- `Authorization: Bearer <token>`
- `X-API-Key`
- Tracing headers (e.g., `X-Request-ID`)
- Proxy auth

Lok has **no mechanism** for auth headers — a blocker for remote Ollama servers behind auth proxies.

#### 3. TLS / HTTPS
ollama-cli has a `--tls` flag. Lok accepts any `base_url` but has no TLS-specific configuration.

#### 4. Multi-Profile Config Management
ollama-cli supports named configs (`-c pi5`, `-c pc`) stored as `~/.ollama-cli/{name}.yaml`. Lok's Ollama backend is a single flat config entry.

### 📋 Medium Impact Gaps

#### 5. Model Discovery
ollama-cli calls `GET /api/list` to show available models, plus scrapes ollama.com for library models. Lok has no equivalent — `lok backends` and `lok suggest` can't discover which models are actually on the server.

#### 6. Model Details
ollama-cli's `GetModelDetails` calls `POST /api/show` for model metadata (modelfile, parameters, template). Useful for debugging.

#### 7. Rich Statistics
ollama-cli shows timing breakdowns (load duration, prompt eval, token generation) and tokens/sec. Lok only reports total elapsed time.

### 🔧 Low Impact / Not Needed

| Feature | Why Not Needed |
|---------|---------------|
| Model management (pull, delete, copy) | Not lok's job — it's a query orchestrator, not a model manager |
| Shell completion | lok has its own CLI framework |
| Standalone Ollama CLI | lok delegates to the HTTP API directly, not through ollama-cli |
| Prompt injection security | lok workflows are authored by developers, not exposed to end-users |
| ollama.com model search | Outside lok's scope |

---

## Blind Spots in lok

| # | Blind Spot | Effect |
|---|-----------|--------|
| 1 | No system prompt support | Workflows can't set model instructions for Ollama |
| 2 | No model options (temperature, etc.) | Workflows can't tune local model behavior per step |
| 3 | No auth/header support | Remote Ollama behind auth proxies is unreachable |
| 4 | No TLS knob | HTTPS servers with custom cert requirements can't connect |
| 5 | No model discovery | `lok backends` / `lok suggest` blind to available models |
| 6 | No conversation context | Multi-turn workflows can't reference prior turns |
| 7 | No streaming | Long-running responses wait for full completion |
| 8 | No image input | Multimodal workflows impossible with Ollama backend |

---

## Backward Compatibility Analysis

| Change | Impact |
|--------|--------|
| Add `headers` field to `BackendConfig` | ✅ Safe — `#[serde(default)]` preserves old configs |
| Add `options` (temperature, etc.) | ✅ Safe — same pattern |
| Add `tls` field | ✅ Safe — `#[serde(default)]` |
| Expand `/api/chat` request body | ✅ Safe — Ollama API ignores unknown fields |
| Change `Backend::query()` signature | ❌ Breaking — affects all backend implementations |
| Add new `BackendConfig` fields without `#[serde(default)]` | ❌ Breaks due to `#[serde(deny_unknown_fields)]` |
| Remove fields from `QueryOutput` | ❌ Breaking |

**Key rule:** `BackendConfig` uses `#[serde(deny_unknown_fields)]`, so every new field **must** have `#[serde(default)]`.

---

## Recommendations

### High Priority (immediate gaps)

1. **System prompt support** — Add a `system` field to the Ollama chat request.
2. **Model options** — Add an `options` map to `BackendConfig` and pass it through to the `/api/chat` request body.
3. **Custom HTTP headers** — Add `headers: Option<HashMap<String, String>>` to `BackendConfig`, attach to reqwest `Client` builder.
4. **TLS configuration** — Add `danger_accept_invalid_certs: Option<bool>` or similar to `BackendConfig`.

### Medium Priority (workflow improvement)

5. **Model listing** — Add an optional `list_models()` method to the `Backend` trait (or keep it Ollama-specific).
6. **Conversation history** — Allow workflow steps to pass prior messages for multi-turn patterns.
7. **Better error classification** — Distinguish "model not found" from "server not running" from "timeout".

### Low Priority (nice-to-have)

8. Streaming output in workflow execution.
9. Image input for multimodal models.
10. Embeddings endpoint for RAG workflows.

---

## Architecture Notes

### How ollama-cli Wraps the Go SDK

ollama-cli uses the official `github.com/ollama/ollama/api` Go SDK via a `Client` interface:

```go
type Client interface {
    ListModels(ctx context.Context) (*api.ListResponse, error)
    GetModelDetails(ctx context.Context, modelName string) (*api.ShowResponse, error)
    DeleteModel(ctx context.Context, modelName string) error
    PullModel(ctx context.Context, modelName string) error
    ChatWithModel(ctx context.Context, modelName string, messages []api.Message, stream bool, options map[string]interface{}) (*api.ChatResponse, error)
}
```

The `api.Client` from the Go SDK calls the same REST endpoints lok hits directly (`POST /api/chat`, `GET /api/list`, etc.). Lok can **stay on the HTTP API directly** — no need to wrap the Go SDK.

### Config Schema in ollama-cli

```yaml
# ~/.ollama-cli/config.yaml
base_url: http://localhost:11434
host: localhost
port: 11434
tls: false
chat_enabled: false
check_updates: true
headers:
  Authorization: Bearer <token>
  X-Custom: value
```

This maps cleanly to additive fields on lok's `BackendConfig`.
