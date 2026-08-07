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
//! | `test-support` | no | Test helpers (`StubBackend`, `acquire_test_lock`, `write_exec_script`, `set_mock_health`, `BackendKey::for_test`). Used internally and by downstream test suites. |
//!
//! `set_mock_health` takes a [`backend::BackendKey`] rather than a backend name, since the
//! cache is keyed by configuration. [`backend::BackendKey::for_test`] builds the canonical
//! key for a name under a default configuration, which is the shortest migration for a
//! call that previously passed a bare name.
//!
//! # Backend cache
//!
//! Backends are cached in a **process-global** [`backend::BACKEND_CACHE`], keyed by
//! [`backend::BackendKey`] - the name together with the effective [`BackendConfig`] and
//! [`RetryPolicy`]. Two callers in one process asking for the same name with different
//! configurations each get an instance honouring their own.
//!
//! The retry policy is part of that identity because it decides whether the cached value
//! is wrapped in a [`RetryExecutor`], and because it is derived from the orchestration
//! config's defaults rather than from [`BackendConfig`] - so two consumers with identical
//! `BackendConfig` can still need different instances.
//!
//! ## What the key cannot capture
//!
//! Two providers read process-ambient state while constructing:
//! [`ClaudeBackend`] resolves `api_key_env` to a variable *name* and then reads that
//! variable, and `BedrockBackend` loads the ambient AWS profile, region and credential
//! chain. Neither value is in the key, so two consumers with identical configuration but
//! different credentials in the environment share one instance, carrying whichever
//! credentials were live when it was first built.
//!
//! Hashing a resolved secret would put credential material into a map key and into
//! `Debug` output, which is worse than the problem. A host that needs isolation should
//! give each tenant a distinct `api_key_env` name, which *is* part of the key.
//!
//! The cache remains process-global, so it is still shared by everything in the process.
//! Handing an embedding host its own cache instance is a larger change and is not done
//! here.
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
    create_backend, Backend, BackendConfig, BackendError, BackendKey, ClaudeBackend, CodexBackend,
    GeminiBackend, HealthStatus, Message, ModelInfo, OllamaBackend, QueryOutput, RetryDefaults,
    RetryExecutor, RetryPolicy, Role, SandboxMode, StepContext, StepOptions, TokenUsage,
    DEFAULT_TIMEOUT, NO_TIMEOUT, RETRY_LOG_TARGET,
};

#[cfg(feature = "bedrock")]
pub use backend::BedrockBackend;
