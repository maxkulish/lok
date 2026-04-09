//! Restores files to their pre-apply state.
//!
//! `Rollback::rollback` is best-effort: it walks `ApplyResult::modified_files`
//! in reverse order, restoring each file's original content (or deleting the
//! file if it was newly created). On per-file I/O failure it records the
//! failure in `RollbackReport::failed` and continues with the remaining files.

use crate::apply_verify::diff_applier::{ApplyResult, FileBackup};
use std::io;
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
    pub async fn rollback(apply_result: &ApplyResult, _cwd: &Path) -> RollbackReport {
        let mut report = RollbackReport::default();

        for backup in apply_result.modified_files.iter().rev() {
            rollback_one(backup, &mut report).await;
        }

        report
    }
}

async fn rollback_one(backup: &FileBackup, report: &mut RollbackReport) {
    match &backup.original_content {
        Some(original) => {
            match tokio::fs::write(&backup.path, original).await {
                Ok(()) => report.restored.push(backup.path.clone()),
                Err(e) => report.failed.push(RollbackFailure {
                    path: backup.path.clone(),
                    reason: format!("failed to restore: {e}"),
                }),
            }
        }
        None => {
            match tokio::fs::remove_file(&backup.path).await {
                Ok(()) => report.deleted.push(backup.path.clone()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    // Already gone - treat as successfully deleted.
                    report.deleted.push(backup.path.clone());
                }
                Err(e) => report.failed.push(RollbackFailure {
                    path: backup.path.clone(),
                    reason: format!("failed to delete created file: {e}"),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_verify::edit_parser::EditFormat;
    use tempfile::tempdir;

    fn make_apply_result(files: Vec<FileBackup>) -> ApplyResult {
        ApplyResult {
            modified_files: files,
            format_applied: EditFormat::JsonOldNew,
        }
    }

    #[tokio::test]
    async fn test_rollback_single_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a.txt");
        tokio::fs::write(&file, "modified").await.unwrap();

        let result = make_apply_result(vec![FileBackup {
            path: file.clone(),
            original_content: Some("original".to_string()),
            new_content: "modified".to_string(),
        }]);

        let report = Rollback::rollback(&result, dir.path()).await;
        assert_eq!(report.restored.len(), 1);
        assert!(report.failed.is_empty());
        assert!(report.deleted.is_empty());
        assert!(report.is_fully_restored());
        assert_eq!(tokio::fs::read_to_string(&file).await.unwrap(), "original");
    }

    #[tokio::test]
    async fn test_rollback_reverse_order() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        let c = dir.path().join("c.txt");
        tokio::fs::write(&a, "a_modified").await.unwrap();
        tokio::fs::write(&b, "b_modified").await.unwrap();
        tokio::fs::write(&c, "c_modified").await.unwrap();

        let result = make_apply_result(vec![
            FileBackup {
                path: a.clone(),
                original_content: Some("a_orig".to_string()),
                new_content: "a_modified".to_string(),
            },
            FileBackup {
                path: b.clone(),
                original_content: Some("b_orig".to_string()),
                new_content: "b_modified".to_string(),
            },
            FileBackup {
                path: c.clone(),
                original_content: Some("c_orig".to_string()),
                new_content: "c_modified".to_string(),
            },
        ]);

        let report = Rollback::rollback(&result, dir.path()).await;
        assert_eq!(report.restored.len(), 3);
        // Reverse order: c, b, a
        assert_eq!(report.restored[0], c);
        assert_eq!(report.restored[1], b);
        assert_eq!(report.restored[2], a);
        // Content restored
        assert_eq!(tokio::fs::read_to_string(&a).await.unwrap(), "a_orig");
        assert_eq!(tokio::fs::read_to_string(&b).await.unwrap(), "b_orig");
        assert_eq!(tokio::fs::read_to_string(&c).await.unwrap(), "c_orig");
    }

    #[tokio::test]
    async fn test_rollback_deletes_new_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("created.rs");
        tokio::fs::write(&file, "fn new() {}").await.unwrap();

        let result = make_apply_result(vec![FileBackup {
            path: file.clone(),
            original_content: None,
            new_content: "fn new() {}".to_string(),
        }]);

        let report = Rollback::rollback(&result, dir.path()).await;
        assert_eq!(report.deleted.len(), 1);
        assert_eq!(report.deleted[0], file);
        assert!(report.restored.is_empty());
        assert!(report.failed.is_empty());
        assert!(!file.exists());
    }

    #[tokio::test]
    async fn test_rollback_continues_on_failure() {
        let dir = tempdir().unwrap();
        let ok_file = dir.path().join("ok.txt");
        tokio::fs::write(&ok_file, "current").await.unwrap();

        // This path points inside a non-existent directory; restore will fail.
        let bad_file = dir.path().join("no/such/dir/bad.txt");

        let result = make_apply_result(vec![
            FileBackup {
                path: ok_file.clone(),
                original_content: Some("ok_orig".to_string()),
                new_content: "current".to_string(),
            },
            FileBackup {
                path: bad_file.clone(),
                original_content: Some("bad_orig".to_string()),
                new_content: "bad_modified".to_string(),
            },
        ]);

        let report = Rollback::rollback(&result, dir.path()).await;
        // Despite bad_file failing, ok_file must still be restored.
        assert_eq!(report.restored.len(), 1);
        assert_eq!(report.restored[0], ok_file);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].path, bad_file);
        assert_eq!(tokio::fs::read_to_string(&ok_file).await.unwrap(), "ok_orig");
    }

    #[tokio::test]
    async fn test_rollback_mixed_restore_and_delete() {
        let dir = tempdir().unwrap();
        let existing = dir.path().join("existing.txt");
        let created = dir.path().join("created.txt");
        tokio::fs::write(&existing, "modified").await.unwrap();
        tokio::fs::write(&created, "new").await.unwrap();

        let result = make_apply_result(vec![
            FileBackup {
                path: existing.clone(),
                original_content: Some("was_here".to_string()),
                new_content: "modified".to_string(),
            },
            FileBackup {
                path: created.clone(),
                original_content: None,
                new_content: "new".to_string(),
            },
        ]);

        let report = Rollback::rollback(&result, dir.path()).await;
        assert_eq!(report.restored.len(), 1);
        assert_eq!(report.deleted.len(), 1);
        assert!(report.failed.is_empty());
        assert_eq!(tokio::fs::read_to_string(&existing).await.unwrap(), "was_here");
        assert!(!created.exists());
    }

    #[tokio::test]
    async fn test_rollback_empty_result_is_noop() {
        let dir = tempdir().unwrap();
        let result = make_apply_result(vec![]);
        let report = Rollback::rollback(&result, dir.path()).await;
        assert!(report.restored.is_empty());
        assert!(report.failed.is_empty());
        assert!(report.deleted.is_empty());
        assert!(report.is_fully_restored());
    }

    #[tokio::test]
    async fn test_rollback_delete_tolerates_already_missing() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("ghost.txt");
        // File never existed - rollback of a "created" backup should tolerate it.

        let result = make_apply_result(vec![FileBackup {
            path: file.clone(),
            original_content: None,
            new_content: "ghost".to_string(),
        }]);

        let report = Rollback::rollback(&result, dir.path()).await;
        assert_eq!(report.deleted.len(), 1);
        assert!(report.failed.is_empty());
    }

    #[tokio::test]
    async fn test_is_fully_restored_true() {
        let report = RollbackReport::default();
        assert!(report.is_fully_restored());
    }

    #[tokio::test]
    async fn test_is_fully_restored_false() {
        let report = RollbackReport {
            restored: vec![],
            failed: vec![RollbackFailure {
                path: PathBuf::from("/nope"),
                reason: "boom".to_string(),
            }],
            deleted: vec![],
        };
        assert!(!report.is_fully_restored());
    }
}
