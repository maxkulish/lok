//! Multi-backend LLM abstraction extracted from the `lok` orchestrator.
//!
//! This crate exposes the provider-agnostic [`Backend`] trait and the
//! concrete backends (Claude, Codex, Gemini, Ollama, optional Bedrock) so
//! downstream tools can run LLM queries without pulling in lok's full
//! orchestration layer.
