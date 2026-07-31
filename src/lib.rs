//! Multi-backend LLM abstraction extracted from the `lok` orchestrator.
//!
//! This crate exposes the provider-agnostic [`Backend`] trait and the
//! concrete backends (Claude, Codex, Gemini, Ollama, optional Bedrock) so
//! downstream tools can run LLM queries without pulling in lok's full
//! orchestration layer.

#![warn(missing_docs)]

/// The [`Backend`] trait, the concrete providers, and the types a call needs.
pub mod backend;

pub use backend::{
    create_backend, Backend, BackendConfig, BackendError, ClaudeBackend, CodexBackend,
    GeminiBackend, HealthStatus, Message, ModelInfo, OllamaBackend, QueryOutput, RetryDefaults,
    RetryExecutor, RetryPolicy, Role, SandboxMode, StepContext, StepOptions, TokenUsage,
    DEFAULT_TIMEOUT, NO_TIMEOUT,
};

#[cfg(feature = "bedrock")]
pub use backend::BedrockBackend;
