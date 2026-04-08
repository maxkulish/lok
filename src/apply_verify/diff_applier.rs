//! Applies normalized `ParsedEdits` (from `EditParser`) to files on disk.
//!
//! `DiffApplier::apply` is the single entry point. It returns `Ok(ApplyResult)`
//! with the list of successfully modified files on success, or
//! `Err(ApplyError { kind, partial })` on failure - where `partial` carries
//! any files that were modified *before* the error so the caller can roll them
//! back via `Rollback::rollback`.

use crate::apply_verify::edit_parser::{EditFormat, ParsedEdits};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Entry point for applying parsed edits to disk.
///
/// This is a unit struct (consistent with `EditParser`) so it can later be
/// extended with configuration (max file size, dry-run flag, etc.) without
/// breaking existing call sites.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffApplier;

/// Per-file record of what changed during apply, used by `Rollback` to restore.
///
/// `original_content = None` means the file did not exist before apply
/// (created via `FullFile`); rollback should delete the file instead of
/// restoring content.
#[derive(Debug, Clone)]
pub struct FileBackup {
    /// Absolute path to the modified file.
    pub path: PathBuf,
    /// File contents immediately before apply. `None` if the file was created.
    pub original_content: Option<String>,
    /// File contents written during apply.
    pub new_content: String,
}

/// Successful apply result, or the partial state inside an `ApplyError`.
///
/// `modified_files` is in application order so `Rollback` can reverse it.
#[derive(Debug, Clone)]
pub struct ApplyResult {
    /// Files modified, in the order they were written.
    pub modified_files: Vec<FileBackup>,
    /// Which `EditFormat` variant drove this apply.
    pub format_applied: EditFormat,
}

impl ApplyResult {
    /// An empty result with the given format - used as the starting accumulator.
    pub fn empty(format: EditFormat) -> Self {
        Self {
            modified_files: Vec::new(),
            format_applied: format,
        }
    }
}

/// Apply failure with the partial result captured so callers can roll back.
///
/// `partial.modified_files` holds every file that was successfully written
/// before the failure. When the failure happened on the first file, `partial`
/// is empty (`ApplyResult::empty`).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{kind}")]
pub struct ApplyError {
    /// Categorized cause of the failure.
    pub kind: ApplyErrorKind,
    /// Files already modified before the failure - ready for rollback.
    pub partial: ApplyResult,
}

/// Categorized reasons apply can fail.
///
/// `ReadFailed` and `WriteFailed` wrap `io::Error` in `Arc` so the enum can
/// derive `Clone` (needed because `ApplyError` is stored in `AttemptRecord`
/// which derives `Clone`).
#[derive(Debug, Clone, thiserror::Error)]
pub enum ApplyErrorKind {
    /// The target file did not exist when apply tried to read it.
    #[error("file not found: {}", path.display())]
    FileNotFound {
        /// Path of the missing file.
        path: PathBuf,
    },

    /// I/O error while reading the file's pre-apply contents.
    #[error("failed to read {}: {source}", path.display())]
    ReadFailed {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error, wrapped in `Arc` for `Clone`.
        source: Arc<io::Error>,
    },

    /// I/O error while writing the new file contents.
    #[error("failed to write {}: {source}", path.display())]
    WriteFailed {
        /// Path that failed to write.
        path: PathBuf,
        /// Underlying I/O error, wrapped in `Arc` for `Clone`.
        source: Arc<io::Error>,
    },

    /// The `old` text from the edit was not found in the file contents.
    #[error("old text not found in {}: {snippet}", path.display())]
    OldTextNotFound {
        /// Path that was searched.
        path: PathBuf,
        /// First ~80 characters of the missing `old` text, for diagnostics.
        snippet: String,
    },

    /// The `old` text matched multiple locations - ambiguous.
    #[error("ambiguous match in {}: {count} occurrences", path.display())]
    AmbiguousMatch {
        /// Path that was searched.
        path: PathBuf,
        /// Number of matches found.
        count: usize,
    },

    /// A multi-hunk unified diff could not be applied because the merged
    /// `old` text (concatenation of all hunks) is not a contiguous substring
    /// of the file. Caller should re-prompt the LLM for JSON old/new or a
    /// single-hunk diff.
    #[error(
        "multi-hunk diff not contiguous in {}: re-emit as JSON old/new or single-hunk diff",
        path.display()
    )]
    MultiHunkDiffNotContiguous {
        /// Path that could not be patched.
        path: PathBuf,
    },

    /// The `FileEdit` itself was malformed (empty path, etc.).
    #[error("invalid edit: {reason}")]
    InvalidEdit {
        /// Human-readable reason.
        reason: String,
    },
}

impl DiffApplier {
    /// Apply the parsed edits to files under `cwd`.
    ///
    /// Returns `Ok(ApplyResult)` with all modified files on success, or
    /// `Err(ApplyError { kind, partial })` where `partial.modified_files`
    /// holds any files that were successfully written before the failure.
    pub async fn apply(
        &self,
        _parsed: &ParsedEdits,
        _cwd: &Path,
    ) -> Result<ApplyResult, ApplyError> {
        todo!("implemented in sub-task 2")
    }
}
