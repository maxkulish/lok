//! External-consumer integration test for the `lokomotiv` library target.
//!
//! This test references only `lokomotiv::` paths, exactly as a downstream crate
//! would, to prove the public API is complete and no `pub(crate)` leaks remain.

use lokomotiv::{
    Backend, BackendConfig, BackendError, QueryOutput, RetryDefaults, RetryPolicy, StepContext,
    TokenUsage,
};
use std::time::Duration;

#[test]
fn public_api_exposes_backend_trait_objects() {
    let cfg = BackendConfig {
        enabled: true,
        command: Some("http://localhost:11434".to_string()),
        model: Some("llama3.2".to_string()),
        ..Default::default()
    };
    let retry = RetryPolicy::from_backend_config(
        &cfg,
        RetryDefaults {
            max_retries: 0,
            retry_delay_ms: 1000,
        },
    );
    let backend = lokomotiv::create_backend("ollama", &cfg, retry).expect("ollama backend");
    assert_eq!(backend.name(), "ollama");
}

#[test]
fn backend_config_builds_from_toml_string() {
    let toml_str = r#"
command = "ollama"
model = "llama3.2"
timeout = "30s"
max_retries = 3
retry_delay_ms = 500
"#;
    let cfg: BackendConfig = toml::from_str(toml_str).expect("parse backend config");
    assert_eq!(cfg.command, Some("ollama".to_string()));
    assert_eq!(cfg.model, Some("llama3.2".to_string()));
    assert_eq!(cfg.timeout, Some(Duration::from_secs(30)));
    assert_eq!(cfg.max_retries, Some(3));
    assert_eq!(cfg.retry_delay_ms, Some(500));
}

#[test]
fn step_context_from_prompt_is_constructible() {
    let ctx = StepContext::from_prompt("hello", std::path::Path::new("/tmp"), Some("m"));
    assert_eq!(ctx.prompt, "hello");
    assert_eq!(ctx.cwd, std::path::Path::new("/tmp"));
    assert_eq!(ctx.model, Some("m"));
    assert_eq!(ctx.sandbox, None);
    assert!(!ctx.apply_edits);
}

#[test]
fn error_and_output_types_are_public() {
    let err = BackendError::Timeout {
        message: "timed out".to_string(),
        elapsed_ms: 1000,
    };
    assert!(err.is_retryable());

    let out = QueryOutput::from_text("hi".to_string(), "test", Duration::from_millis(10));
    assert_eq!(out.stdout, "hi");

    let usage = TokenUsage::new(1, 2);
    assert_eq!(usage.total_tokens, 3);
}

#[test]
fn retry_defaults_match_lok_config_defaults() {
    let defaults = RetryDefaults::default();
    assert_eq!(defaults.max_retries, 0);
    assert_eq!(defaults.retry_delay_ms, 1000);
}

#[tokio::test]
#[ignore = "requires a running local ollama instance"]
async fn ollama_query_round_trip() {
    use lokomotiv::OllamaBackend;

    let cfg = BackendConfig {
        enabled: true,
        command: Some("http://localhost:11434".to_string()),
        model: Some("llama3.2".to_string()),
        ..Default::default()
    };
    let backend = OllamaBackend::new(&cfg).expect("construct ollama backend");
    let ctx = StepContext::from_prompt(
        "Say exactly the word 'pong' and nothing else.",
        std::path::Path::new("."),
        None,
    );
    let output = backend.query(ctx).await.expect("ollama query");
    assert!(!output.stdout.is_empty());
    assert_eq!(output.backend, "ollama");
}
