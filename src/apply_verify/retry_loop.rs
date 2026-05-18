//! Orchestrates the parse -> apply -> verify -> rollback -> re-query cycle.
//!
//! `RetryLoop::execute` takes initial raw LLM output and drives the full
//! cycle, delegating LLM re-queries to an `EditRequester` trait implementor
//! (which the caller provides) so `apply_verify` stays decoupled from the
//! `Backend` trait.

use crate::apply_verify::diff_applier::{ApplyResult, DiffApplier};
use crate::apply_verify::edit_parser::EditParser;
use crate::apply_verify::rollback::Rollback;
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
#[allow(clippy::enum_variant_names)]
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
        initial_raw: String,
        cwd: &Path,
        applier: &DiffApplier,
        requester: &dyn EditRequester,
    ) -> RetryLoopOutcome {
        let mut attempts: Vec<AttemptRecord> = Vec::new();
        let mut raw = initial_raw;

        for attempt_num in 0..=self.max_retries {
            let mut record = AttemptRecord {
                attempt_num,
                raw_output: raw.clone(),
                parse_error: None,
                apply_error: None,
                verify_result: None,
                rolled_back: false,
            };

            // --- Parse ---
            let parsed = match EditParser::parse(&raw) {
                Ok(p) => p,
                Err(e) => {
                    let err_msg = e.to_string();
                    record.parse_error = Some(err_msg.clone());

                    if self.stop_on_parse_error || attempt_num == self.max_retries {
                        attempts.push(record);
                        return RetryLoopOutcome {
                            success: false,
                            attempts,
                            final_verify: None,
                            final_apply: None,
                        };
                    }

                    // Re-query
                    let ctx = RetryContext {
                        attempt: attempt_num + 2,
                        previous_raw: &raw,
                        reason: RetryReason::ParseError(&err_msg),
                    };
                    match requester.request_edits(&ctx).await {
                        Ok(new_raw) => {
                            attempts.push(record);
                            raw = new_raw;
                            continue;
                        }
                        Err(req_err) => {
                            record.apply_error = Some(req_err);
                            attempts.push(record);
                            return RetryLoopOutcome {
                                success: false,
                                attempts,
                                final_verify: None,
                                final_apply: None,
                            };
                        }
                    }
                }
            };

            // --- Apply ---
            let apply_result = match applier.apply(&parsed, cwd).await {
                Ok(r) => r,
                Err(apply_err) => {
                    let err_msg = apply_err.kind.to_string();
                    record.apply_error = Some(err_msg.clone());

                    // Rollback partial
                    if !apply_err.partial.modified_files.is_empty() {
                        Rollback::rollback(&apply_err.partial, cwd).await;
                        record.rolled_back = true;
                    }

                    if attempt_num == self.max_retries {
                        attempts.push(record);
                        return RetryLoopOutcome {
                            success: false,
                            attempts,
                            final_verify: None,
                            final_apply: None,
                        };
                    }

                    let partial_paths: Vec<PathBuf> = apply_err
                        .partial
                        .modified_files
                        .iter()
                        .map(|f| f.path.clone())
                        .collect();
                    let ctx = RetryContext {
                        attempt: attempt_num + 2,
                        previous_raw: &raw,
                        reason: RetryReason::ApplyError {
                            message: &err_msg,
                            partial_paths: &partial_paths,
                        },
                    };
                    match requester.request_edits(&ctx).await {
                        Ok(new_raw) => {
                            attempts.push(record);
                            raw = new_raw;
                            continue;
                        }
                        Err(req_err) => {
                            record.apply_error = Some(req_err);
                            attempts.push(record);
                            return RetryLoopOutcome {
                                success: false,
                                attempts,
                                final_verify: None,
                                final_apply: None,
                            };
                        }
                    }
                }
            };

            // --- Verify ---
            let verify_result = self.verify.run(cwd).await;
            record.verify_result = Some(verify_result.clone());

            if verify_result.success {
                attempts.push(record);
                return RetryLoopOutcome {
                    success: true,
                    attempts,
                    final_verify: Some(verify_result),
                    final_apply: Some(apply_result),
                };
            }

            // Verify failed - rollback
            Rollback::rollback(&apply_result, cwd).await;
            record.rolled_back = true;

            if attempt_num == self.max_retries {
                let final_vr = verify_result;
                attempts.push(record);
                return RetryLoopOutcome {
                    success: false,
                    attempts,
                    final_verify: Some(final_vr),
                    final_apply: None,
                };
            }

            // Re-query with verify error context
            let ctx = RetryContext {
                attempt: attempt_num + 2,
                previous_raw: &raw,
                reason: RetryReason::VerifyError(&verify_result),
            };
            match requester.request_edits(&ctx).await {
                Ok(new_raw) => {
                    attempts.push(record);
                    raw = new_raw;
                }
                Err(req_err) => {
                    record.apply_error = Some(req_err);
                    attempts.push(record);
                    return RetryLoopOutcome {
                        success: false,
                        attempts,
                        final_verify: Some(verify_result),
                        final_apply: None,
                    };
                }
            }
        }

        // Should not reach here given the loop bounds, but just in case.
        RetryLoopOutcome {
            success: false,
            attempts,
            final_verify: None,
            final_apply: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;
    use tempfile::tempdir;

    /// A mock `EditRequester` that returns canned responses per attempt and
    /// records the `RetryReason` variant it was called with.
    struct MockEditRequester {
        /// Canned responses: index 0 = first retry request, index 1 = second, etc.
        responses: Vec<Result<String, String>>,
        /// Records the reason variant name for each call.
        reasons_seen: Mutex<Vec<String>>,
    }

    impl MockEditRequester {
        fn new(responses: Vec<Result<String, String>>) -> Self {
            Self {
                responses,
                reasons_seen: Mutex::new(Vec::new()),
            }
        }

        fn reasons(&self) -> Vec<String> {
            self.reasons_seen.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl EditRequester for MockEditRequester {
        async fn request_edits(&self, context: &RetryContext<'_>) -> Result<String, String> {
            let variant = match &context.reason {
                RetryReason::ParseError(_) => "ParseError".to_string(),
                RetryReason::ApplyError { .. } => "ApplyError".to_string(),
                RetryReason::VerifyError(_) => "VerifyError".to_string(),
            };
            let mut reasons = self.reasons_seen.lock().unwrap();
            let idx = reasons.len();
            reasons.push(variant);
            self.responses
                .get(idx)
                .cloned()
                .unwrap_or(Err("no more canned responses".to_string()))
        }
    }

    fn make_retry_loop(max_retries: u32, verify_cmd: &str, stop_on_parse: bool) -> RetryLoop {
        RetryLoop {
            max_retries,
            verify: Verification {
                command: verify_cmd.to_string(),
                timeout: Duration::from_secs(5),
                max_output_bytes: 64 * 1024,
            },
            stop_on_parse_error: stop_on_parse,
        }
    }

    fn json_edit(file: &str, old: &str, new: &str) -> String {
        format!(r#"{{"edits": [{{"file": "{file}", "old": "{old}", "new": "{new}"}}]}}"#,)
    }

    // -----------------------------------------------------------------------
    // Test: success on first attempt
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_success_first_attempt() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "old_text").await.unwrap();

        let raw = json_edit("a.txt", "old_text", "new_text");
        let rl = make_retry_loop(0, "exit 0", false);
        let requester = MockEditRequester::new(vec![]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(outcome.success);
        assert_eq!(outcome.attempts.len(), 1);
        assert!(outcome.final_verify.is_some());
        assert!(outcome.final_apply.is_some());
        assert!(!outcome.attempts[0].rolled_back);
        assert_eq!(tokio::fs::read_to_string(&f).await.unwrap(), "new_text");
    }

    // -----------------------------------------------------------------------
    // Test: success on second attempt after verify failure + rollback
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_success_on_retry_after_verify_failure() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        // First attempt: apply succeeds, verify fails (exit 1)
        let raw = json_edit("a.txt", "original", "bad_change");
        // Second attempt: apply succeeds, verify succeeds
        let retry_raw = json_edit("a.txt", "original", "good_change");

        let rl = make_retry_loop(1, "test \"$(cat a.txt)\" = good_change", false);
        let requester = MockEditRequester::new(vec![Ok(retry_raw)]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(outcome.success);
        assert_eq!(outcome.attempts.len(), 2);
        // First attempt was rolled back
        assert!(outcome.attempts[0].rolled_back);
        assert!(outcome.attempts[0]
            .verify_result
            .as_ref()
            .is_some_and(|v| !v.success));
        // Second attempt succeeded
        assert!(!outcome.attempts[1].rolled_back);
        assert_eq!(requester.reasons(), vec!["VerifyError"]);
        assert_eq!(tokio::fs::read_to_string(&f).await.unwrap(), "good_change");
    }

    // -----------------------------------------------------------------------
    // Test: max retries exhausted
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_max_retries_exhausted() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        let raw = json_edit("a.txt", "original", "bad");
        let retry_raw = json_edit("a.txt", "original", "still_bad");

        let rl = make_retry_loop(1, "exit 1", false);
        let requester = MockEditRequester::new(vec![Ok(retry_raw)]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(!outcome.success);
        assert_eq!(outcome.attempts.len(), 2);
        assert!(outcome.attempts[0].rolled_back);
        assert!(outcome.attempts[1].rolled_back);
        assert!(outcome.final_verify.is_some());
        assert!(outcome.final_apply.is_none());
        // File rolled back to original
        assert_eq!(tokio::fs::read_to_string(&f).await.unwrap(), "original");
    }

    // -----------------------------------------------------------------------
    // Test: parse error with stop_on_parse_error = true aborts immediately
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_parse_error_stop() {
        let dir = tempdir().unwrap();
        let raw = "this is not valid json or diff or anything";
        let rl = make_retry_loop(3, "exit 0", true);
        let requester = MockEditRequester::new(vec![]);
        let outcome = rl
            .execute(raw.to_string(), dir.path(), &DiffApplier, &requester)
            .await;

        assert!(!outcome.success);
        assert_eq!(outcome.attempts.len(), 1);
        assert!(outcome.attempts[0].parse_error.is_some());
        assert!(outcome.final_verify.is_none());
        // Requester never called
        assert!(requester.reasons().is_empty());
    }

    // -----------------------------------------------------------------------
    // Test: parse error with stop_on_parse_error = false retries
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_parse_error_retries() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "old_text").await.unwrap();

        let bad_raw = "not parseable output";
        let good_raw = json_edit("a.txt", "old_text", "new_text");

        let rl = make_retry_loop(1, "exit 0", false);
        let requester = MockEditRequester::new(vec![Ok(good_raw)]);
        let outcome = rl
            .execute(bad_raw.to_string(), dir.path(), &DiffApplier, &requester)
            .await;

        assert!(outcome.success);
        assert_eq!(outcome.attempts.len(), 2);
        assert!(outcome.attempts[0].parse_error.is_some());
        assert!(outcome.attempts[1].parse_error.is_none());
        assert_eq!(requester.reasons(), vec!["ParseError"]);
    }

    // -----------------------------------------------------------------------
    // Test: apply error triggers rollback and requester sees ApplyError
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_apply_error_triggers_rollback_and_retry() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        // First attempt references text that doesn't exist in the file
        let bad_edit = json_edit("a.txt", "nonexistent_text", "replacement");
        // Second attempt is correct
        let good_edit = json_edit("a.txt", "original", "fixed");

        let rl = make_retry_loop(1, "exit 0", false);
        let requester = MockEditRequester::new(vec![Ok(good_edit)]);
        let outcome = rl
            .execute(bad_edit, dir.path(), &DiffApplier, &requester)
            .await;

        assert!(outcome.success);
        assert_eq!(outcome.attempts.len(), 2);
        assert!(outcome.attempts[0].apply_error.is_some());
        // No partial files were modified (error on the first edit), so no rollback
        assert!(!outcome.attempts[0].rolled_back);
        assert_eq!(requester.reasons(), vec!["ApplyError"]);
        assert_eq!(tokio::fs::read_to_string(&f).await.unwrap(), "fixed");
    }

    // -----------------------------------------------------------------------
    // Test: apply partial failure rolls back already-modified files
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_apply_partial_failure_rolls_back() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.txt");
        tokio::fs::write(&a, "a_original").await.unwrap();
        // b.txt does not exist - edit targeting it with old text will fail

        // Two edits: first succeeds on a.txt, second fails because b.txt
        // has old text that won't match (file doesn't even exist)
        let raw = r#"{"edits": [
                {"file": "a.txt", "old": "a_original", "new": "a_modified"},
                {"file": "b.txt", "old": "b_original", "new": "b_modified"}
            ]}"#
        .to_string();

        let rl = make_retry_loop(0, "exit 0", false);
        let requester = MockEditRequester::new(vec![]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(!outcome.success);
        assert_eq!(outcome.attempts.len(), 1);
        assert!(outcome.attempts[0].apply_error.is_some());
        // a.txt was partially applied, so rollback happened
        assert!(outcome.attempts[0].rolled_back);
        // a.txt restored to original
        assert_eq!(tokio::fs::read_to_string(&a).await.unwrap(), "a_original");
    }

    // -----------------------------------------------------------------------
    // Test: verify failure triggers rollback and requester sees VerifyError
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_verify_failure_triggers_rollback() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        let raw = json_edit("a.txt", "original", "changed");
        let rl = make_retry_loop(0, "exit 1", false);
        let requester = MockEditRequester::new(vec![]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(!outcome.success);
        assert!(outcome.attempts[0].rolled_back);
        assert!(outcome.attempts[0].verify_result.is_some());
        // File rolled back
        assert_eq!(tokio::fs::read_to_string(&f).await.unwrap(), "original");
    }

    // -----------------------------------------------------------------------
    // Test: attempt records capture each attempt
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_attempt_records() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        let raw = json_edit("a.txt", "original", "v1");
        let retry1 = json_edit("a.txt", "original", "v2");
        let retry2 = json_edit("a.txt", "original", "v3");

        // All verify with exit 1 - all fail
        let rl = make_retry_loop(2, "exit 1", false);
        let requester = MockEditRequester::new(vec![Ok(retry1), Ok(retry2)]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(!outcome.success);
        assert_eq!(outcome.attempts.len(), 3);
        assert_eq!(outcome.attempts[0].attempt_num, 0);
        assert_eq!(outcome.attempts[1].attempt_num, 1);
        assert_eq!(outcome.attempts[2].attempt_num, 2);
        assert!(outcome.attempts[0].raw_output.contains("v1"));
        assert!(outcome.attempts[1].raw_output.contains("v2"));
        assert!(outcome.attempts[2].raw_output.contains("v3"));
    }

    // -----------------------------------------------------------------------
    // Test: requester error surfaced in outcome
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_requester_error_surfaced() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        let raw = json_edit("a.txt", "original", "bad");
        let rl = make_retry_loop(1, "exit 1", false);
        let requester = MockEditRequester::new(vec![Err("LLM unavailable".to_string())]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(!outcome.success);
        assert_eq!(outcome.attempts.len(), 1);
        assert_eq!(
            outcome.attempts[0].apply_error.as_deref(),
            Some("LLM unavailable")
        );
    }

    // -----------------------------------------------------------------------
    // Test: max_retries = 0 runs exactly once
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_max_retries_zero_runs_once() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        let raw = json_edit("a.txt", "original", "changed");
        let rl = make_retry_loop(0, "exit 1", false);
        let requester = MockEditRequester::new(vec![]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(!outcome.success);
        assert_eq!(outcome.attempts.len(), 1);
        // Requester never called
        assert!(requester.reasons().is_empty());
    }

    // -----------------------------------------------------------------------
    // Test: parse error on last retry exits without re-query
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_parse_error_on_last_retry_exits() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("a.txt");
        tokio::fs::write(&f, "original").await.unwrap();

        // First attempt parses/applies/verify-fails, retry with bad output
        let raw = json_edit("a.txt", "original", "bad");
        let bad_retry = "completely unparseable garbage".to_string();

        let rl = make_retry_loop(1, "exit 1", false);
        let requester = MockEditRequester::new(vec![Ok(bad_retry)]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(!outcome.success);
        assert_eq!(outcome.attempts.len(), 2);
        // First attempt: verify failed
        assert!(outcome.attempts[0].verify_result.is_some());
        // Second attempt: parse failed (last retry, no re-query)
        assert!(outcome.attempts[1].parse_error.is_some());
        assert_eq!(requester.reasons(), vec!["VerifyError"]);
    }

    // -----------------------------------------------------------------------
    // Integration: real parse -> apply -> verify end-to-end
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_integration_end_to_end() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("config.txt");
        tokio::fs::write(&f, "debug = false").await.unwrap();

        let raw = json_edit("config.txt", "debug = false", "debug = true");
        // Verify by checking file content
        let rl = make_retry_loop(0, "grep -q 'debug = true' config.txt", false);
        let requester = MockEditRequester::new(vec![]);
        let outcome = rl.execute(raw, dir.path(), &DiffApplier, &requester).await;

        assert!(outcome.success);
        assert_eq!(outcome.attempts.len(), 1);
        let vr = outcome.final_verify.as_ref().unwrap();
        assert!(vr.success);
        assert_eq!(vr.exit_code, Some(0));
        assert!(vr.elapsed_ms > 0);
        let ar = outcome.final_apply.as_ref().unwrap();
        assert_eq!(ar.modified_files.len(), 1);
        assert_eq!(tokio::fs::read_to_string(&f).await.unwrap(), "debug = true");
    }
}
