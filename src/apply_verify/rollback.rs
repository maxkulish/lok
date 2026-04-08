//! Restores files to their pre-apply state.
//!
//! `Rollback::rollback` is best-effort: it walks `ApplyResult::modified_files`
//! in reverse order, restoring each file's original content (or deleting the
//! file if it was newly created). On per-file I/O failure it records the
//! failure in `RollbackReport::failed` and continues with the remaining files.

use crate::apply_verify::diff_applier::ApplyResult;
use std::path::{Path, PathBuf};

/// Entry point for rolling back an `ApplyResult`.
///
/// Unit struct used as a namespace for the `rollback` associated function.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rollback;

/// Summary of a rollback operation.
///
/// `restored`, `failed`, and `deleted` are disjoint: each file in
/// `ApplyResult::modified_files` appears in exactly one of them.
#[derive(Debug, Clone, Default)]
pub struct RollbackReport {
    /// Files whose original content was successfully written back.
    pub restored: Vec<PathBuf>,
    /// Files whose rollback failed (with per-file reason).
    pub failed: Vec<RollbackFailure>,
    /// Files that were newly created during apply and deleted during rollback.
    pub deleted: Vec<PathBuf>,
}

impl RollbackReport {
    /// `true` iff no files failed to roll back.
    pub fn is_fully_restored(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Per-file rollback failure.
#[derive(Debug, Clone)]
pub struct RollbackFailure {
    /// Path that could not be rolled back.
    pub path: PathBuf,
    /// Human-readable reason (usually an I/O error message).
    pub reason: String,
}

impl Rollback {
    /// Roll back every file in `apply_result.modified_files` in reverse order.
    ///
    /// Never returns `Result`: rollback is best-effort and reports per-file
    /// outcomes via `RollbackReport`.
    pub async fn rollback(_apply_result: &ApplyResult, _cwd: &Path) -> RollbackReport {
        todo!("implemented in sub-task 3")
    }
}
