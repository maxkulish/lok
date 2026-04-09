//! Runs shell verification commands with bounded output and timeout.
//!
//! `Verification::run(cwd)` executes `sh -c <command>`, caps stdout+stderr
//! capture at `max_output_bytes`, measures real wall-clock elapsed time, and
//! on Unix places the child in a new process group so the entire process
//! tree can be reaped on timeout.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

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
    /// Maximum bytes to capture from each of stdout and stderr independently.
    /// Further output is dropped and `VerifyResult::truncated` is set to `true`.
    /// Note: worst-case total memory is `2 * max_output_bytes` (both streams
    /// at capacity). This per-stream cap avoids shared-counter complexity
    /// across concurrent tokio tasks.
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
    /// `true` iff stdout or stderr capture hit `max_output_bytes` and further
    /// output was dropped.
    pub truncated: bool,
}

impl Verification {
    /// Execute the command under `cwd` and return a structured result.
    ///
    /// The `cwd` parameter (rather than a struct field) ensures `RetryLoop`
    /// has a single source of truth for the working directory.
    pub async fn run(&self, cwd: &Path) -> VerifyResult {
        let start = Instant::now();

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&self.command)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // On Unix, place the child in its own process group so we can reap
        // the whole tree (sh + its descendants) on timeout. Without this,
        // `kill_on_drop` only kills the direct `sh` child and orphans any
        // grandchildren it spawned.
        #[cfg(unix)]
        cmd.process_group(0);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return VerifyResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("spawn failed: {e}"),
                    exit_code: None,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                    truncated: false,
                };
            }
        };

        let pid = child.id();
        let (stdout, stderr) = match (child.stdout.take(), child.stderr.take()) {
            (Some(out), Some(err)) => (out, err),
            _ => {
                return VerifyResult {
                    success: false,
                    stdout: String::new(),
                    stderr: "failed to capture stdout/stderr pipes".to_string(),
                    exit_code: None,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                    truncated: false,
                };
            }
        };
        let max_bytes = self.max_output_bytes;

        let stdout_handle = tokio::spawn(read_bounded(stdout, max_bytes));
        let stderr_handle = tokio::spawn(read_bounded(stderr, max_bytes));

        match tokio::time::timeout(self.timeout, child.wait()).await {
            Ok(Ok(status)) => {
                let (stdout_bytes, stdout_trunc) = stdout_handle.await.unwrap_or_default();
                let (stderr_bytes, stderr_trunc) = stderr_handle.await.unwrap_or_default();
                VerifyResult {
                    success: status.success(),
                    stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
                    exit_code: status.code(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                    truncated: stdout_trunc || stderr_trunc,
                }
            }
            Ok(Err(e)) => VerifyResult {
                success: false,
                stdout: String::new(),
                stderr: format!("wait failed: {e}"),
                exit_code: None,
                elapsed_ms: start.elapsed().as_millis() as u64,
                timed_out: false,
                truncated: false,
            },
            Err(_) => {
                // Timeout fired. Reap the entire process group so that
                // descendants (e.g. `sleep` spawned by `sh -c "sleep 30 & wait"`)
                // die along with the direct child.
                kill_process_group(pid);
                // Await the child so we don't leave a zombie; kill_on_drop will
                // also cover this but explicit wait drains the state machine.
                let _ = child.wait().await;
                let (stdout_bytes, stdout_trunc) = stdout_handle.await.unwrap_or_default();
                let (stderr_bytes, stderr_trunc) = stderr_handle.await.unwrap_or_default();
                VerifyResult {
                    success: false,
                    stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
                    exit_code: None,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    timed_out: true,
                    truncated: stdout_trunc || stderr_trunc,
                }
            }
        }
    }
}

/// Send `SIGKILL` to the process group led by `pid`. No-op on non-Unix or
/// if `pid` is `None` (child already reaped).
fn kill_process_group(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        // SAFETY: `pid` was obtained from `child.id()` while the child was
        // still alive. Killing the negated pid sends SIGKILL to the entire
        // process group. There are no mutable aliases or other Rust-side
        // invariants at risk - this is a direct syscall into libc.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

/// Read from `reader` into a byte vector, stopping once `max_bytes` is reached.
///
/// Returns `(bytes, truncated)`. When `truncated` is `true`, further output
/// from the pipe is drained (but dropped) so the writer does not block.
async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R, max_bytes: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;

    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                let remaining = max_bytes.saturating_sub(buf.len());
                if remaining == 0 {
                    truncated = true;
                    drain(&mut reader, &mut chunk).await;
                    break;
                }
                let take = n.min(remaining);
                buf.extend_from_slice(&chunk[..take]);
                if take < n {
                    truncated = true;
                    drain(&mut reader, &mut chunk).await;
                    break;
                }
            }
            Err(_) => break,
        }
    }

    (buf, truncated)
}

/// Drain any remaining bytes from `reader` so the writer side does not block
/// on a full pipe. Everything read here is discarded.
async fn drain<R: AsyncRead + Unpin>(reader: &mut R, chunk: &mut [u8; 8192]) {
    loop {
        match reader.read(chunk).await {
            Ok(0) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn verification(command: &str, timeout_ms: u64) -> Verification {
        Verification {
            command: command.to_string(),
            timeout: Duration::from_millis(timeout_ms),
            max_output_bytes: 64 * 1024,
        }
    }

    #[tokio::test]
    async fn test_verify_success() {
        let dir = tempdir().unwrap();
        let result = verification("exit 0", 5_000).run(dir.path()).await;
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn test_verify_failure_exit_code() {
        let dir = tempdir().unwrap();
        let result = verification("exit 1", 5_000).run(dir.path()).await;
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn test_verify_captures_stdout() {
        let dir = tempdir().unwrap();
        let result = verification("echo hello_world", 5_000)
            .run(dir.path())
            .await;
        assert!(result.success);
        assert!(result.stdout.contains("hello_world"));
    }

    #[tokio::test]
    async fn test_verify_captures_stderr() {
        let dir = tempdir().unwrap();
        let result = verification("echo bad >&2; exit 1", 5_000)
            .run(dir.path())
            .await;
        assert!(!result.success);
        assert!(result.stderr.contains("bad"));
    }

    #[tokio::test]
    async fn test_verify_captures_both_streams() {
        let dir = tempdir().unwrap();
        let result = verification("echo out; echo err >&2", 5_000)
            .run(dir.path())
            .await;
        assert!(result.success);
        assert!(result.stdout.contains("out"));
        assert!(result.stderr.contains("err"));
    }

    #[tokio::test]
    async fn test_verify_invalid_command_exits_127() {
        let dir = tempdir().unwrap();
        let result = verification("nonexistent_binary_xyz_123", 5_000)
            .run(dir.path())
            .await;
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(127));
        assert!(!result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_verify_timeout_real_elapsed() {
        let dir = tempdir().unwrap();
        let result = verification("sleep 10", 200).run(dir.path()).await;
        assert!(!result.success);
        assert!(result.timed_out);
        assert_eq!(result.exit_code, None);
        // Real elapsed time should be in the vicinity of the timeout, not
        // hardcoded to `timeout.as_millis()`. We allow a generous upper bound
        // to accommodate slow CI runners.
        assert!(
            result.elapsed_ms >= 150,
            "elapsed_ms = {} should be >= 150",
            result.elapsed_ms
        );
        assert!(
            result.elapsed_ms <= 5_000,
            "elapsed_ms = {} should be <= 5000",
            result.elapsed_ms
        );
    }

    #[tokio::test]
    async fn test_verify_output_truncated() {
        let dir = tempdir().unwrap();
        let mut v = verification("yes hello | head -c 100000", 5_000);
        v.max_output_bytes = 1024;
        let result = v.run(dir.path()).await;
        assert!(result.truncated);
        assert!(result.stdout.len() <= 1024);
    }

    #[tokio::test]
    async fn test_verify_elapsed_ms_nonzero() {
        let dir = tempdir().unwrap();
        // Quick commands still report a measurable elapsed time.
        let result = verification("sleep 0.05", 5_000).run(dir.path()).await;
        assert!(result.success);
        assert!(
            result.elapsed_ms >= 40,
            "elapsed_ms = {} should be >= 40",
            result.elapsed_ms
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_verify_timeout_kills_process_group() {
        let dir = tempdir().unwrap();
        // Spawn a grandchild sleeper via sh -c. With process_group(0) + the
        // libc::kill(-pid, SIGKILL) path, the grandchild is reaped when the
        // timeout fires - so `run` returns quickly instead of waiting 30s.
        let start = Instant::now();
        let result = verification("sleep 30 & wait", 200).run(dir.path()).await;
        let elapsed = start.elapsed();
        assert!(result.timed_out);
        // Must complete well before the 30s grandchild sleep would finish.
        assert!(
            elapsed < Duration::from_secs(5),
            "verification took {:?}; process group kill likely failed",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_verify_uses_passed_cwd() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker.txt");
        tokio::fs::write(&marker, "present").await.unwrap();
        let result = verification("test -f marker.txt && echo yes", 5_000)
            .run(dir.path())
            .await;
        assert!(result.success);
        assert!(result.stdout.contains("yes"));
    }
}
