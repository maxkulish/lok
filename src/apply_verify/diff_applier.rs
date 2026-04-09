//! Applies normalized `ParsedEdits` (from `EditParser`) to files on disk.
//!
//! `DiffApplier::apply` is the single entry point. It returns `Ok(ApplyResult)`
//! with the list of successfully modified files on success, or
//! `Err(ApplyError { kind, partial })` on failure - where `partial` carries
//! any files that were modified *before* the error so the caller can roll them
//! back via `Rollback::rollback`.

use crate::apply_verify::edit_parser::{EditFormat, ParsedEdits};
use crate::workflow::FileEdit;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Maximum characters of `old` text included in `OldTextNotFound::snippet`.
const SNIPPET_LEN: usize = 80;

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
        parsed: &ParsedEdits,
        cwd: &Path,
    ) -> Result<ApplyResult, ApplyError> {
        let mut partial = ApplyResult::empty(parsed.format);

        for edit in &parsed.edits {
            if edit.file.is_empty() {
                return Err(ApplyError {
                    kind: ApplyErrorKind::InvalidEdit {
                        reason: "empty file path".to_string(),
                    },
                    partial,
                });
            }

            let file_path = cwd.join(&edit.file);

            let backup = match parsed.format {
                EditFormat::FullFile => apply_full_file(edit, file_path, &partial).await?,
                EditFormat::JsonOldNew | EditFormat::UnifiedDiff => {
                    apply_find_replace(edit, file_path, parsed.format, &partial).await?
                }
            };

            partial.modified_files.push(backup);
        }

        Ok(partial)
    }
}

/// Apply a `FullFile` edit: capture original content (if any), create parent
/// dirs, write `edit.new` as the whole file.
async fn apply_full_file(
    edit: &FileEdit,
    file_path: PathBuf,
    partial: &ApplyResult,
) -> Result<FileBackup, ApplyError> {
    let original_content = match tokio::fs::read_to_string(&file_path).await {
        Ok(c) => Some(c),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(ApplyError {
                kind: ApplyErrorKind::ReadFailed {
                    path: file_path,
                    source: Arc::new(e),
                },
                partial: partial.clone(),
            });
        }
    };

    if original_content.is_none() {
        if let Some(parent) = file_path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return Err(ApplyError {
                        kind: ApplyErrorKind::WriteFailed {
                            path: file_path,
                            source: Arc::new(e),
                        },
                        partial: partial.clone(),
                    });
                }
            }
        }
    }

    if let Err(e) = tokio::fs::write(&file_path, &edit.new).await {
        return Err(ApplyError {
            kind: ApplyErrorKind::WriteFailed {
                path: file_path,
                source: Arc::new(e),
            },
            partial: partial.clone(),
        });
    }

    Ok(FileBackup {
        path: file_path,
        original_content,
        new_content: edit.new.clone(),
    })
}

/// Apply a `JsonOldNew` or single-hunk `UnifiedDiff` edit via find-and-replace.
///
/// For `UnifiedDiff`, a missing `old` string produces `MultiHunkDiffNotContiguous`
/// instead of `OldTextNotFound` so the caller can surface a targeted hint to
/// the LLM ("emit JSON old/new or single-hunk diff").
async fn apply_find_replace(
    edit: &FileEdit,
    file_path: PathBuf,
    format: EditFormat,
    partial: &ApplyResult,
) -> Result<FileBackup, ApplyError> {
    let content = match tokio::fs::read_to_string(&file_path).await {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(ApplyError {
                kind: ApplyErrorKind::FileNotFound { path: file_path },
                partial: partial.clone(),
            });
        }
        Err(e) => {
            return Err(ApplyError {
                kind: ApplyErrorKind::ReadFailed {
                    path: file_path,
                    source: Arc::new(e),
                },
                partial: partial.clone(),
            });
        }
    };

    let match_count = content.matches(&edit.old).count();
    if match_count == 0 {
        let kind = if format == EditFormat::UnifiedDiff {
            ApplyErrorKind::MultiHunkDiffNotContiguous { path: file_path }
        } else {
            ApplyErrorKind::OldTextNotFound {
                path: file_path,
                snippet: edit.old.chars().take(SNIPPET_LEN).collect(),
            }
        };
        return Err(ApplyError {
            kind,
            partial: partial.clone(),
        });
    }
    if match_count > 1 {
        return Err(ApplyError {
            kind: ApplyErrorKind::AmbiguousMatch {
                path: file_path,
                count: match_count,
            },
            partial: partial.clone(),
        });
    }

    let new_content = content.replacen(&edit.old, &edit.new, 1);
    if let Err(e) = tokio::fs::write(&file_path, &new_content).await {
        return Err(ApplyError {
            kind: ApplyErrorKind::WriteFailed {
                path: file_path,
                source: Arc::new(e),
            },
            partial: partial.clone(),
        });
    }

    Ok(FileBackup {
        path: file_path,
        original_content: Some(content),
        new_content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_verify::edit_parser::ParsedEdits;
    use tempfile::tempdir;

    fn make_parsed(edits: Vec<FileEdit>, format: EditFormat) -> ParsedEdits {
        ParsedEdits {
            edits,
            format,
            summary: None,
        }
    }

    #[tokio::test]
    async fn test_apply_json_single_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        tokio::fs::write(&file, "hello world").await.unwrap();

        let parsed = make_parsed(
            vec![FileEdit {
                file: "hello.txt".to_string(),
                old: "hello".to_string(),
                new: "goodbye".to_string(),
            }],
            EditFormat::JsonOldNew,
        );

        let applier = DiffApplier;
        let result = applier.apply(&parsed, dir.path()).await.unwrap();

        assert_eq!(result.modified_files.len(), 1);
        assert_eq!(result.modified_files[0].original_content.as_deref(), Some("hello world"));
        assert_eq!(result.modified_files[0].new_content, "goodbye world");
        let on_disk = tokio::fs::read_to_string(&file).await.unwrap();
        assert_eq!(on_disk, "goodbye world");
    }

    #[tokio::test]
    async fn test_apply_old_text_not_found() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("f.txt");
        tokio::fs::write(&file, "hello world").await.unwrap();

        let parsed = make_parsed(
            vec![FileEdit {
                file: "f.txt".to_string(),
                old: "xyzzy_missing".to_string(),
                new: "anything".to_string(),
            }],
            EditFormat::JsonOldNew,
        );

        let err = DiffApplier.apply(&parsed, dir.path()).await.unwrap_err();
        match err.kind {
            ApplyErrorKind::OldTextNotFound { snippet, .. } => {
                assert!(snippet.contains("xyzzy_missing"));
            }
            other => panic!("expected OldTextNotFound, got {:?}", other),
        }
        assert!(err.partial.modified_files.is_empty());
    }

    #[tokio::test]
    async fn test_apply_ambiguous_match() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("dup.txt");
        tokio::fs::write(&file, "foo foo bar").await.unwrap();

        let parsed = make_parsed(
            vec![FileEdit {
                file: "dup.txt".to_string(),
                old: "foo".to_string(),
                new: "baz".to_string(),
            }],
            EditFormat::JsonOldNew,
        );

        let err = DiffApplier.apply(&parsed, dir.path()).await.unwrap_err();
        match err.kind {
            ApplyErrorKind::AmbiguousMatch { count, .. } => assert_eq!(count, 2),
            other => panic!("expected AmbiguousMatch, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_apply_file_not_found() {
        let dir = tempdir().unwrap();

        let parsed = make_parsed(
            vec![FileEdit {
                file: "nope.txt".to_string(),
                old: "a".to_string(),
                new: "b".to_string(),
            }],
            EditFormat::JsonOldNew,
        );

        let err = DiffApplier.apply(&parsed, dir.path()).await.unwrap_err();
        assert!(matches!(err.kind, ApplyErrorKind::FileNotFound { .. }));
    }

    #[tokio::test]
    async fn test_apply_full_file_overwrite() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("config.toml");
        tokio::fs::write(&file, "old content").await.unwrap();

        let parsed = make_parsed(
            vec![FileEdit {
                file: "config.toml".to_string(),
                old: String::new(),
                new: "new content entirely".to_string(),
            }],
            EditFormat::FullFile,
        );

        let result = DiffApplier.apply(&parsed, dir.path()).await.unwrap();
        assert_eq!(result.modified_files.len(), 1);
        assert_eq!(
            result.modified_files[0].original_content.as_deref(),
            Some("old content")
        );
        let on_disk = tokio::fs::read_to_string(&file).await.unwrap();
        assert_eq!(on_disk, "new content entirely");
    }

    #[tokio::test]
    async fn test_apply_full_file_create_new() {
        let dir = tempdir().unwrap();

        let parsed = make_parsed(
            vec![FileEdit {
                file: "nested/dir/new.rs".to_string(),
                old: String::new(),
                new: "fn main() {}".to_string(),
            }],
            EditFormat::FullFile,
        );

        let result = DiffApplier.apply(&parsed, dir.path()).await.unwrap();
        assert_eq!(result.modified_files.len(), 1);
        assert!(result.modified_files[0].original_content.is_none());
        let created = dir.path().join("nested/dir/new.rs");
        assert!(created.exists());
        let content = tokio::fs::read_to_string(&created).await.unwrap();
        assert_eq!(content, "fn main() {}");
    }

    #[tokio::test]
    async fn test_apply_unified_diff_single_hunk() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("src.rs");
        tokio::fs::write(&file, "fn a() { println!(\"a\"); }\n")
            .await
            .unwrap();

        let parsed = make_parsed(
            vec![FileEdit {
                file: "src.rs".to_string(),
                old: "println!(\"a\");".to_string(),
                new: "println!(\"b\");".to_string(),
            }],
            EditFormat::UnifiedDiff,
        );

        let result = DiffApplier.apply(&parsed, dir.path()).await.unwrap();
        assert_eq!(result.modified_files.len(), 1);
        let on_disk = tokio::fs::read_to_string(&file).await.unwrap();
        assert_eq!(on_disk, "fn a() { println!(\"b\"); }\n");
    }

    #[tokio::test]
    async fn test_apply_unified_diff_multi_hunk_fails() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("src.rs");
        tokio::fs::write(
            &file,
            "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {}\n",
        )
        .await
        .unwrap();

        // Simulate what parse_unified_diff would produce for a multi-hunk
        // non-contiguous diff: merged old text that does NOT appear in the file.
        let parsed = make_parsed(
            vec![FileEdit {
                file: "src.rs".to_string(),
                old: "fn a() {}\nfn c() {}".to_string(),
                new: "fn a_new() {}\nfn c_new() {}".to_string(),
            }],
            EditFormat::UnifiedDiff,
        );

        let err = DiffApplier.apply(&parsed, dir.path()).await.unwrap_err();
        assert!(
            matches!(err.kind, ApplyErrorKind::MultiHunkDiffNotContiguous { .. }),
            "expected MultiHunkDiffNotContiguous, got {:?}",
            err.kind
        );
    }

    #[tokio::test]
    async fn test_apply_multi_file_success() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        tokio::fs::write(&a, "alpha").await.unwrap();
        tokio::fs::write(&b, "beta").await.unwrap();

        let parsed = make_parsed(
            vec![
                FileEdit {
                    file: "a.txt".to_string(),
                    old: "alpha".to_string(),
                    new: "ALPHA".to_string(),
                },
                FileEdit {
                    file: "b.txt".to_string(),
                    old: "beta".to_string(),
                    new: "BETA".to_string(),
                },
            ],
            EditFormat::JsonOldNew,
        );

        let result = DiffApplier.apply(&parsed, dir.path()).await.unwrap();
        assert_eq!(result.modified_files.len(), 2);
        // Application order is preserved.
        assert!(result.modified_files[0].path.ends_with("a.txt"));
        assert!(result.modified_files[1].path.ends_with("b.txt"));
        assert_eq!(tokio::fs::read_to_string(&a).await.unwrap(), "ALPHA");
        assert_eq!(tokio::fs::read_to_string(&b).await.unwrap(), "BETA");
    }

    #[tokio::test]
    async fn test_apply_partial_failure() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.txt");
        tokio::fs::write(&a, "alpha").await.unwrap();
        // b.txt is intentionally missing.

        let parsed = make_parsed(
            vec![
                FileEdit {
                    file: "a.txt".to_string(),
                    old: "alpha".to_string(),
                    new: "ALPHA".to_string(),
                },
                FileEdit {
                    file: "b.txt".to_string(),
                    old: "beta".to_string(),
                    new: "BETA".to_string(),
                },
            ],
            EditFormat::JsonOldNew,
        );

        let err = DiffApplier.apply(&parsed, dir.path()).await.unwrap_err();
        assert!(matches!(err.kind, ApplyErrorKind::FileNotFound { .. }));
        // a.txt was successfully modified before the failure.
        assert_eq!(err.partial.modified_files.len(), 1);
        assert!(err.partial.modified_files[0].path.ends_with("a.txt"));
        // Note: a.txt on disk still holds ALPHA; rollback restores it.
        assert_eq!(tokio::fs::read_to_string(&a).await.unwrap(), "ALPHA");
    }

    #[tokio::test]
    async fn test_apply_empty_edits() {
        let dir = tempdir().unwrap();
        let parsed = make_parsed(vec![], EditFormat::JsonOldNew);
        let result = DiffApplier.apply(&parsed, dir.path()).await.unwrap();
        assert!(result.modified_files.is_empty());
        assert_eq!(result.format_applied, EditFormat::JsonOldNew);
    }

    #[tokio::test]
    async fn test_apply_empty_file_path_is_invalid_edit() {
        let dir = tempdir().unwrap();
        let parsed = make_parsed(
            vec![FileEdit {
                file: String::new(),
                old: "a".to_string(),
                new: "b".to_string(),
            }],
            EditFormat::JsonOldNew,
        );
        let err = DiffApplier.apply(&parsed, dir.path()).await.unwrap_err();
        assert!(matches!(err.kind, ApplyErrorKind::InvalidEdit { .. }));
    }
}
