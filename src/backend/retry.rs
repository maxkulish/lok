use super::{Backend, BackendError, QueryOutput};
use async_trait::async_trait;
use colored::Colorize;
use rand::Rng;
use std::path::Path;
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
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
    ) -> std::result::Result<QueryOutput, BackendError> {
        let mut last_error: Option<BackendError> = None;

        for attempt in 0..=self.policy.max_retries {
            if attempt > 0 {
                // Determine delay: respect server-provided retry_after if available,
                // otherwise use our exponential backoff policy.
                let delay = if let Some(BackendError::RateLimit { retry_after_ms: Some(ms), .. }) = last_error.as_ref() {
                    Duration::from_millis(*ms)
                } else {
                    self.policy.get_delay(attempt)
                };

                eprintln!("  {} Retrying {} (attempt {}/{}) in {:?}...", 
                    "↻".yellow(), self.inner.name(), attempt, self.policy.max_retries, delay);
                sleep(delay).await;
            }

            match self.inner.query(prompt, cwd, model).await {
                Ok(output) => return Ok(output),
                Err(e) if e.is_retryable() => {
                    last_error = Some(e);
                }
                Err(e) => return Err(e), // Fatal error, don't retry
            }
        }

        Err(last_error.unwrap_or_else(|| BackendError::ExecutionFailed { 
            message: "Retry loop exhausted without error".to_string(), 
            exit_code: None 
        }))
    }

    fn is_available(&self) -> bool {
        self.inner.is_available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::QueryOutput;

    struct MockBackend {
        failure_count: std::sync::atomic::AtomicUsize,
        max_failures: usize,
    }

    #[async_trait]
    impl Backend for MockBackend {
        fn name(&self) -> &str { "mock" }
        fn is_available(&self) -> bool { true }
        async fn query(&self, _: &str, _: &Path, _: Option<&str>) -> std::result::Result<QueryOutput, BackendError> {
            let count = self.failure_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count < self.max_failures {
                Err(BackendError::Network { message: "transient".into() })
            } else {
                Ok(QueryOutput::from_text("success".into()))
            }
        }
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let inner = Arc::new(MockBackend { 
            failure_count: std::sync::atomic::AtomicUsize::new(0),
            max_failures: 2 
        });
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let executor = RetryExecutor::new(inner, policy);
        let res = executor.query("test", Path::new("."), None).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap().stdout, "success");
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let inner = Arc::new(MockBackend { 
            failure_count: std::sync::atomic::AtomicUsize::new(0),
            max_failures: 5 
        });
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let executor = RetryExecutor::new(inner, policy);
        let res = executor.query("test", Path::new("."), None).await;
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), BackendError::Network { .. }));
    }
}
