//! Helper spawned by `tests/backend_public_api.rs` to prove the library writes
//! nothing to the terminal on its own.
//!
//! This has to be a separate process. Rust runs tests as parallel threads of a
//! single process and stdout/stderr are process-wide, so redirecting them
//! in-process would race with every other test. The test spawns this binary and
//! captures only its streams.
//!
//! Usage: `silence_probe <success|retry|sandbox> <logger|no-logger>`
//!
//! In `no-logger` mode nothing installs a `log` implementation, which is the
//! situation an embedding consumer is in by default; both streams must stay
//! empty. In `logger` mode a marker logger writes each record as
//! `RECORD:<level>:<message>`, proving the warnings were routed through `log`
//! rather than deleted.

use lokomotiv::{
    Backend, BackendConfig, BackendError, CodexBackend, QueryOutput, RetryExecutor, RetryPolicy,
    SandboxMode, StepContext,
};
use std::sync::Arc;
use std::time::Duration;

/// Writes every record to stdout with a fixed marker so the parent can assert
/// on delivery without depending on any formatter.
struct MarkerLogger;

impl log::Log for MarkerLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        println!("RECORD:{}:{}", record.level(), record.args());
    }
    fn flush(&self) {}
}

static MARKER: MarkerLogger = MarkerLogger;

/// Always fails with a retryable error, so `RetryExecutor` exhausts its budget
/// deterministically without a network, a closed port, or a sleep.
#[derive(Debug)]
struct AlwaysRetryable;

#[async_trait::async_trait]
impl Backend for AlwaysRetryable {
    fn name(&self) -> &str {
        "always-retryable"
    }
    async fn query(&self, _ctx: StepContext<'_>) -> Result<QueryOutput, BackendError> {
        Err(BackendError::Network {
            message: "synthetic failure".to_string(),
        })
    }
    fn is_available(&self) -> bool {
        true
    }
}

/// Succeeds immediately, for the successful-query leg of the silence contract.
#[derive(Debug)]
struct AlwaysSucceeds;

#[async_trait::async_trait]
impl Backend for AlwaysSucceeds {
    fn name(&self) -> &str {
        "always-succeeds"
    }
    async fn query(&self, _ctx: StepContext<'_>) -> Result<QueryOutput, BackendError> {
        Ok(QueryOutput::from_text(
            "ok".to_string(),
            "always-succeeds",
            Duration::ZERO,
        ))
    }
    fn is_available(&self) -> bool {
        true
    }
}

/// Drive a successful query end to end, through the retry decorator so the
/// wrapper is on the path even though it never fires.
async fn exercise_success() {
    let executor = RetryExecutor::new(
        Arc::new(AlwaysSucceeds) as Arc<dyn Backend>,
        RetryPolicy {
            max_retries: 2,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        },
    );
    let cwd = std::env::current_dir().expect("cwd");
    let out = executor
        .query(StepContext::from_prompt("probe", &cwd, None))
        .await
        .expect("successful query");
    debug_assert_eq!(out.stdout, "ok");
}

/// Drive the retry path. Zero delay keeps the probe instant.
async fn exercise_retry() {
    let policy = RetryPolicy {
        max_retries: 2,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
    };
    let executor = RetryExecutor::new(Arc::new(AlwaysRetryable) as Arc<dyn Backend>, policy);
    let cwd = std::env::current_dir().expect("cwd");
    let ctx = StepContext::from_prompt("probe", &cwd, None);
    // Expected to fail after exhausting retries; only the streams matter.
    let _ = executor.query(ctx).await;
}

/// Drive the read-only-sandbox warning through the public `query()` path.
///
/// `resolve_effective_sandbox` is private, so it cannot be called directly.
/// The warning fires while the argv is assembled, before the subprocess runs,
/// so a fake executable that exits immediately is enough.
async fn exercise_sandbox() {
    let script = lokomotiv::backend::write_exec_script(b"#!/bin/sh\nexit 0\n");
    let cfg = BackendConfig {
        enabled: true,
        command: Some(script.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let backend = CodexBackend::new(&cfg).expect("codex backend from fake script");
    let cwd = std::env::current_dir().expect("cwd");

    let mut ctx = StepContext::from_prompt("probe", &cwd, None);
    ctx.apply_edits = true;
    ctx.sandbox = Some(SandboxMode::ReadOnly);

    // Expected to fail parsing the fake script's empty output; streams matter.
    let _ = backend.query(ctx).await;
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let scenario = args.next().unwrap_or_default();
    let logging = args.next().unwrap_or_default();

    if logging == "logger" {
        log::set_logger(&MARKER).expect("install marker logger");
        log::set_max_level(log::LevelFilter::Trace);
    }

    match scenario.as_str() {
        "success" => exercise_success().await,
        "retry" => exercise_retry().await,
        "sandbox" => exercise_sandbox().await,
        other => {
            // Deliberately not a panic: a panic writes to stderr and would be
            // indistinguishable from the leak this binary exists to detect.
            std::process::exit(if other.is_empty() { 2 } else { 3 });
        }
    }
}
