//! Orchestrates the parse -> apply -> verify -> rollback -> re-query cycle.
//!
//! `RetryLoop::execute` takes initial raw LLM output and drives the full
//! cycle, delegating LLM re-queries to an `EditRequester` trait implementor
//! (which the caller provides) so `apply_verify` stays decoupled from the
//! `Backend` trait.

use crate::apply_verify::diff_applier::{ApplyResult, DiffApplier};
use crate::apply_verify::verification::{Verification, VerifyResult};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Top-level retry orchestrator.
#[derive(Debug, Clone)]
pub struct RetryLoop {
    /// Maximum retry attempts (single budget covering parse/apply/verify).
    ///
    /// `max_retries = 0` means "run once, no retries".
    pub max_retries: u32,
    /// Verification configuration used after each apply.
    pub verify: Verification,
    /// When `true`, a parse error exits the loop immediately with
    /// `success: false`. When `false`, a parse error triggers a re-query
    /// with `RetryReason::ParseError`.
    pub stop_on_parse_error: bool,
}

/// Final outcome of a retry loop execution.
#[derive(Debug, Clone)]
pub struct RetryLoopOutcome {
    /// `true` iff an attempt ended with `verify.success == true`.
    pub success: bool,
    /// One record per attempt, in order (attempt 0 is the initial call).
    pub attempts: Vec<AttemptRecord>,
    /// Verify result from the successful attempt, if any.
    pub final_verify: Option<VerifyResult>,
    /// Apply result from the successful attempt, if any. Always `None` on
    /// failed outcomes (unsuccessful attempts are rolled back).
    pub final_apply: Option<ApplyResult>,
}

/// Per-attempt record stored in `RetryLoopOutcome::attempts`.
#[derive(Debug, Clone)]
pub struct AttemptRecord {
    /// 0-indexed attempt number.
    pub attempt_num: u32,
    /// Owned copy of the raw LLM output this attempt tried to apply.
    pub raw_output: String,
    /// Parse error message, if parse failed.
    pub parse_error: Option<String>,
    /// Apply error message, if apply failed or requester failed.
    pub apply_error: Option<String>,
    /// Verify result, if verify ran at all.
    pub verify_result: Option<VerifyResult>,
    /// `true` iff rollback was invoked (either after apply-partial or verify
    /// failure).
    pub rolled_back: bool,
}

/// Why the loop is asking for a new edit.
///
/// An enum (rather than `Option<&VerifyResult> + Option<&str>`) because a
/// parse or apply failure occurs *before* verify runs, so there is no
/// `VerifyResult` to reference.
#[derive(Debug)]
pub enum RetryReason<'a> {
    /// `EditParser::parse` failed. The string is the parse error display.
    ParseError(&'a str),
    /// `DiffApplier::apply` failed. `partial_paths` lists files already
    /// written (and about to be rolled back) for the current attempt.
    ApplyError {
        /// Human-readable apply error (`ApplyErrorKind` display).
        message: &'a str,
        /// Paths of files that were successfully written before the failure.
        partial_paths: &'a [PathBuf],
    },
    /// Apply succeeded but `Verification::run` returned `success: false`
    /// (non-zero exit, timeout, or spawn failure).
    VerifyError(&'a VerifyResult),
}

/// Context passed to `EditRequester::request_edits` so the implementor can
/// build a remediation prompt.
#[derive(Debug)]
pub struct RetryContext<'a> {
    /// 1-indexed attempt number of the *next* attempt the requester is being
    /// asked to provide edits for.
    pub attempt: u32,
    /// The raw LLM output from the previous attempt (what failed).
    pub previous_raw: &'a str,
    /// Why the previous attempt failed.
    pub reason: RetryReason<'a>,
}

/// Callback trait for fetching new edits on a retry.
///
/// A trait (rather than a boxed closure) keeps `apply_verify` decoupled from
/// the `Backend` trait and avoids async-closure complexity.
#[async_trait]
pub trait EditRequester: Send + Sync {
    /// Request a new batch of edits given the previous failure context.
    ///
    /// Returns the raw LLM output as a `String` on success. On `Err`, the
    /// retry loop records the message in the current `AttemptRecord::apply_error`
    /// and exits with `success: false`.
    async fn request_edits(&self, context: &RetryContext<'_>) -> Result<String, String>;
}

impl RetryLoop {
    /// Execute the full retry cycle.
    ///
    /// `cwd` is the working directory passed to both `applier.apply` and
    /// `self.verify.run(cwd)` so there is a single source of truth.
    pub async fn execute(
        &self,
        _initial_raw: String,
        _cwd: &Path,
        _applier: &DiffApplier,
        _requester: &dyn EditRequester,
    ) -> RetryLoopOutcome {
        todo!("implemented in sub-task 5")
    }
}
