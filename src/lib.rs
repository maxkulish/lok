//! Multi-backend LLM abstraction extracted from the `lok` orchestrator.
//!
//! This crate exposes the provider-agnostic [`Backend`] trait and the
//! concrete backends (Claude, Codex, Gemini, Ollama, optional Bedrock) so
//! downstream tools can run LLM queries without pulling in lok's full
//! orchestration layer.
//!
//! # Quick start
//!
//! Add `lokomotiv` to your `Cargo.toml` with **no default features** so the
//! CLI-only dependencies (`clap`, `indicatif`, …) are not compiled:
//!
//! ```toml
//! [dependencies]
//! lokomotiv = { version = "20260603", default-features = false }
//! ```
//!
//! Then build an Ollama backend and run a query:
//!
//! ```no_run
//! use std::path::Path;
//! use lokomotiv::{
//!     Backend, BackendConfig, RetryDefaults, RetryPolicy, StepContext,
//!     create_backend,
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = BackendConfig {
//!         command: Some("http://localhost:11434".into()),
//!         model: Some("llama3.2".into()),
//!         max_retries: Some(2),
//!         retry_delay_ms: Some(500),
//!         ..Default::default()
//!     };
//!
//!     let defaults = RetryDefaults {
//!         max_retries: 3,
//!         retry_delay_ms: 1000,
//!     };
//!     let policy = RetryPolicy::from_backend_config(&config, defaults);
//!
//!     let backend = create_backend("ollama", &config, policy)?;
//!
//!     let ctx = StepContext::from_prompt(
//!         "What is the capital of France?",
//!         Path::new("."),
//!         None,
//!     );
//!
//!     let output = backend.query(ctx).await?;
//!     println!("Model: {}", output.model.unwrap_or_default());
//!     println!("Answer: {}", output.stdout);
//!
//!     if let Some(usage) = output.usage {
//!         println!("Tokens: {} prompt + {} completion",
//!             usage.prompt_tokens, usage.completion_tokens);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Cargo features
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `default` | yes | Enables `cli` (see below). Library consumers should use `default-features = false`. |
//! | `cli` | yes | CLI-only dependencies (`clap`, `indicatif`, `colored`, …). Required by the `lok` and `lokomotiv` binaries. |
//! | `bedrock` | no | AWS Bedrock backend via `aws-sdk-bedrockruntime`. Adds a compile-time dependency on the AWS SDK. |
//! | `test-support` | no | Test helpers (`StubBackend`, `acquire_test_lock`, `write_exec_script`). Used internally and by downstream test suites. |
//!
//! # Backend cache
//!
//! Backends are cached in a **process-global** [`backend::BACKEND_CACHE`] keyed by name
//! alone. Two callers in one process asking for the same name with different
//! configurations share the first instance built. This is a known constraint
//! documented on [`backend::BACKEND_CACHE`]; it avoids forcing every provider to carry
//! its own cache while keeping the health-probe layer simple.
//!
//! # Versioning
//!
//! `lokomotiv` shares its version number with the `lok` binary. Both are
//! published from the same repository under a single `Cargo.toml`. The version
//! follows a date-based scheme (`YYYYMMDD.N.0`) rather than semver, reflecting
//! the fact that the library and binary evolve together. Pin to a specific
//! version in your `Cargo.toml` to avoid unexpected changes.

#![deny(missing_docs)]

/// The [`Backend`] trait, the concrete providers, and the types a call needs.
pub mod backend;

pub use backend::{
    create_backend, Backend, BackendConfig, BackendError, ClaudeBackend, CodexBackend,
    GeminiBackend, HealthStatus, Message, ModelInfo, OllamaBackend, QueryOutput, RetryDefaults,
    RetryExecutor, RetryPolicy, Role, SandboxMode, StepContext, StepOptions, TokenUsage,
    DEFAULT_TIMEOUT, NO_TIMEOUT, RETRY_LOG_TARGET,
};

#[cfg(feature = "bedrock")]
pub use backend::BedrockBackend;
