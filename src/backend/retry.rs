use super::{Backend, BackendError, HealthStatus, QueryOutput};
use async_trait::async_trait;
use colored::Colorize;
use rand::Rng;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// Policy for retrying transient backend failures
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 means no retries)
    pub max_retries: usize,
    /// Base delay for exponential backoff
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// Get the delay for a specific retry attempt using exponential backoff with jitter
    pub fn get_delay(&self, attempt: usize) -> Duration {
        if attempt == 0 {
            return Duration::from_secs(0);
        }

        // Exponential backoff: base * 2^(attempt-1)
        let exp = 2_u32.pow(attempt as u32 - 1);
        let delay = self.base_delay * exp;

        // Add jitter (±10%) to prevent thundering herd
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(0.9..1.1);
        let delay_with_jitter = Duration::from_secs_f64(delay.as_secs_f64() * jitter);

        delay_with_jitter.min(self.max_delay)
    }
}

/// A backend decorator that automatically retries transient failures
pub struct RetryExecutor {
    inner: Arc<dyn Backend>,
    policy: RetryPolicy,
}

impl RetryExecutor {
    /// Create a new RetryExecutor wrapping the given backend
    pub fn new(inner: Arc<dyn Backend>, policy: RetryPolicy) -> Self {
        Self { inner, policy }
    }
}

#[async_trait]
impl Backend for RetryExecutor {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn query(
        &self,
        ctx: crate::backend::StepContext<'_>,
    ) -> std::result::Result<QueryOutput, BackendError> {
        let mut last_error: Option<BackendError> = None;

        for attempt in 0..=self.policy.max_retries {
            if attempt > 0 {
                // Determine delay: respect server-provided retry_after if available,
                // otherwise use our exponential backoff policy.
                let delay = if let Some(BackendError::RateLimit {
                    retry_after_ms: Some(ms),
                    ..
                }) = last_error.as_ref()
                {
                    Duration::from_millis(*ms)
                } else {
                    self.policy.get_delay(attempt)
                };

                eprintln!(
                    "  {} Retrying {} (attempt {}/{}) in {:?}...",
                    "↻".yellow(),
                    self.inner.name(),
                    attempt,
                    self.policy.max_retries,
                    delay
                );
                sleep(delay).await;
            }

            match self.inner.query(ctx).await {
                Ok(output) => return Ok(output),
                Err(e) if e.is_retryable() => {
                    last_error = Some(e);
                }
                Err(e) => return Err(e), // Fatal error, don't retry
            }
        }

        Err(last_error.unwrap_or_else(|| BackendError::ExecutionFailed {
            message: "Retry loop exhausted without error".to_string(),
            exit_code: None,
        }))
    }

    fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
        self.inner.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::QueryOutput;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockBackend {
        failure_count: AtomicUsize,
        max_failures: usize,
    }

    #[async_trait]
    impl Backend for MockBackend {
        fn name(&self) -> &str {
            "mock"
        }
        fn is_available(&self) -> bool {
            true
        }
        async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
            Ok(HealthStatus)
        }
        async fn query(
            &self,
            _ctx: crate::backend::StepContext<'_>,
        ) -> std::result::Result<QueryOutput, BackendError> {
            let count = self.failure_count.fetch_add(1, Ordering::SeqCst);
            if count < self.max_failures {
                Err(BackendError::Network {
                    message: "transient".into(),
                })
            } else {
                Ok(QueryOutput::from_text(
                    "success".into(),
                    "mock",
                    Duration::from_millis(0),
                ))
            }
        }
    }

    /// Returns a fixed BackendError on every call. Used to test non-retryable paths
    /// and rate-limit retry_after honoring.
    struct FixedErrorBackend {
        error: std::sync::Mutex<Option<BackendError>>,
        call_count: AtomicUsize,
    }

    impl FixedErrorBackend {
        fn new(err: BackendError) -> Self {
            Self {
                error: std::sync::Mutex::new(Some(err)),
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Backend for FixedErrorBackend {
        fn name(&self) -> &str {
            "fixed-error"
        }
        fn is_available(&self) -> bool {
            true
        }
        async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
            Ok(HealthStatus)
        }
        async fn query(
            &self,
            _ctx: crate::backend::StepContext<'_>,
        ) -> std::result::Result<QueryOutput, BackendError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            // Clone the held error for each call
            let guard = self.error.lock().unwrap();
            let err = guard.as_ref().unwrap();
            Err(match err {
                BackendError::Auth { message } => BackendError::Auth {
                    message: message.clone(),
                },
                BackendError::RateLimit {
                    message,
                    retry_after_ms,
                } => BackendError::RateLimit {
                    message: message.clone(),
                    retry_after_ms: *retry_after_ms,
                },
                BackendError::Parse { message } => BackendError::Parse {
                    message: message.clone(),
                },
                BackendError::Network { message } => BackendError::Network {
                    message: message.clone(),
                },
                _ => BackendError::ExecutionFailed {
                    message: "unsupported".into(),
                    exit_code: None,
                },
            })
        }
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let inner = Arc::new(MockBackend {
            failure_count: AtomicUsize::new(0),
            max_failures: 2,
        });
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let executor = RetryExecutor::new(inner, policy);
        let ctx = crate::backend::StepContext {
            prompt: "test",
            history: &[],
            model: None,
            cwd: std::path::Path::new("."),
            sandbox: None,
            apply_edits: false,
            schema: None,
            options: None,
            timeout: None,
        };
        let res = executor.query(ctx).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap().stdout, "success");
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let inner = Arc::new(MockBackend {
            failure_count: AtomicUsize::new(0),
            max_failures: 5,
        });
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let executor = RetryExecutor::new(inner, policy);
        let ctx = crate::backend::StepContext {
            prompt: "test",
            history: &[],
            model: None,
            cwd: std::path::Path::new("."),
            sandbox: None,
            apply_edits: false,
            schema: None,
            options: None,
            timeout: None,
        };
        let res = executor.query(ctx).await;
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), BackendError::Network { .. }));
    }

    #[test]
    fn test_get_delay_attempt_zero_is_zero() {
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        };
        assert_eq!(policy.get_delay(0), Duration::from_secs(0));
    }

    #[test]
    fn test_get_delay_grows_exponentially() {
        let policy = RetryPolicy {
            max_retries: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
        };
        // Attempt 1: ~100ms (base * 2^0 = 100), with ±10% jitter -> [90, 110]
        // Attempt 2: ~200ms (base * 2^1 = 200), with ±10% jitter -> [180, 220]
        // Attempt 3: ~400ms (base * 2^2 = 400), with ±10% jitter -> [360, 440]
        let d1 = policy.get_delay(1);
        let d2 = policy.get_delay(2);
        let d3 = policy.get_delay(3);
        assert!(
            d1 >= Duration::from_millis(90) && d1 <= Duration::from_millis(110),
            "d1 = {:?}",
            d1
        );
        assert!(
            d2 >= Duration::from_millis(180) && d2 <= Duration::from_millis(220),
            "d2 = {:?}",
            d2
        );
        assert!(
            d3 >= Duration::from_millis(360) && d3 <= Duration::from_millis(440),
            "d3 = {:?}",
            d3
        );
    }

    #[test]
    fn test_get_delay_clamped_at_max() {
        let policy = RetryPolicy {
            max_retries: 10,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
        };
        // Attempt 10: 1s * 2^9 = 512s, must clamp to 5s
        assert_eq!(policy.get_delay(10), Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_retry_executor_does_not_retry_non_retryable() {
        let inner = Arc::new(FixedErrorBackend::new(BackendError::Auth {
            message: "bad token".into(),
        }));
        let inner_clone = inner.clone();
        let policy = RetryPolicy {
            max_retries: 5,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let executor = RetryExecutor::new(inner, policy);
        let ctx = crate::backend::StepContext {
            prompt: "test",
            history: &[],
            model: None,
            cwd: std::path::Path::new("."),
            sandbox: None,
            apply_edits: false,
            schema: None,
            options: None,
            timeout: None,
        };
        let res = executor.query(ctx).await;
        assert!(matches!(res, Err(BackendError::Auth { .. })));
        // Auth is not retryable -> exactly one call, no retries
        assert_eq!(inner_clone.call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_executor_honors_rate_limit_retry_after() {
        let inner = Arc::new(FixedErrorBackend::new(BackendError::RateLimit {
            message: "slow down".into(),
            retry_after_ms: Some(50),
        }));
        let inner_clone = inner.clone();
        let policy = RetryPolicy {
            max_retries: 1,
            // Tiny base delay; if the retry_after is honored, total elapsed
            // should be at least ~50ms despite the small base.
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let executor = RetryExecutor::new(inner, policy);
        let start = std::time::Instant::now();
        let ctx = crate::backend::StepContext {
            prompt: "test",
            history: &[],
            model: None,
            cwd: std::path::Path::new("."),
            sandbox: None,
            apply_edits: false,
            schema: None,
            options: None,
            timeout: None,
        };
        let res = executor.query(ctx).await;
        let elapsed = start.elapsed();
        assert!(matches!(res, Err(BackendError::RateLimit { .. })));
        // 1 initial + 1 retry = 2 calls
        assert_eq!(inner_clone.call_count.load(Ordering::SeqCst), 2);
        // The retry sleep should be ~50ms (server-provided), not 1ms (base_delay).
        assert!(
            elapsed >= Duration::from_millis(45),
            "elapsed = {:?}, expected >= 45ms",
            elapsed
        );
    }
}
