//! Runs shell verification commands with bounded output and timeout.
//!
//! `Verification::run(cwd)` executes `sh -c <command>`, caps stdout+stderr
//! capture at `max_output_bytes`, measures real wall-clock elapsed time, and
//! on Unix places the child in a new process group so the entire process
//! tree can be reaped on timeout.

use std::path::Path;
use std::time::Duration;

/// A single verification command configuration.
///
/// `cwd` is intentionally NOT a struct field - it is passed to `run()` so
/// `RetryLoop` has a single source of truth for the working directory across
/// apply and verify stages.
#[derive(Debug, Clone)]
pub struct Verification {
    /// Shell command to run (will be wrapped with `sh -c`).
    pub command: String,
    /// Hard wall-clock timeout. On timeout the whole process group is killed.
    pub timeout: Duration,
    /// Maximum bytes to capture from stdout+stderr combined. Further output
    /// is dropped and `VerifyResult::truncated` is set to `true`.
    pub max_output_bytes: usize,
}

/// Structured verification result.
///
/// Never wrapped in `Result`: verify failure (non-zero exit, timeout, spawn
/// error) is a normal outcome, not a Rust error. Callers inspect `success`.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// `true` iff the process exited with status 0 and did not time out.
    pub success: bool,
    /// Captured stdout (possibly truncated - see `truncated`).
    pub stdout: String,
    /// Captured stderr (possibly truncated - see `truncated`).
    pub stderr: String,
    /// Exit code if the process exited normally. `None` on timeout, signal,
    /// or spawn failure.
    pub exit_code: Option<i32>,
    /// Actual wall-clock elapsed time from spawn to reap, in milliseconds.
    /// Measured via `std::time::Instant`, never derived from `timeout`.
    pub elapsed_ms: u64,
    /// `true` iff the process was killed because `timeout` was exceeded.
    pub timed_out: bool,
    /// `true` iff stdout+stderr capture hit `max_output_bytes` and further
    /// output was dropped.
    pub truncated: bool,
}

impl Verification {
    /// Execute the command under `cwd` and return a structured result.
    ///
    /// The `cwd` parameter (rather than a struct field) ensures `RetryLoop`
    /// has a single source of truth for the working directory.
    pub async fn run(&self, _cwd: &Path) -> VerifyResult {
        todo!("implemented in sub-task 4")
    }
}
