//! Retry logic with configurable backoff policies for pipeline node execution.

use std::time::Duration;

/// Backoff policy controlling the delay between retry attempts.
#[derive(Debug, Clone)]
pub enum BackoffPolicy {
    /// Fixed delay between retries.
    Fixed(Duration),
    /// Exponential backoff: base * 2^attempt, capped at max.
    Exponential { base: Duration, max: Duration },
    /// No delay between retries.
    None,
}

impl BackoffPolicy {
    /// Compute the delay for a given attempt number (0-indexed).
    pub fn delay_for_attempt(&self, attempt: usize) -> Duration {
        match self {
            BackoffPolicy::Fixed(d) => *d,
            BackoffPolicy::Exponential { base, max } => {
                let millis = base.as_millis() as u64 * 2u64.saturating_pow(attempt as u32);
                Duration::from_millis(millis).min(*max)
            }
            BackoffPolicy::None => Duration::ZERO,
        }
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        BackoffPolicy::Exponential {
            base: Duration::from_millis(500),
            max: Duration::from_secs(30),
        }
    }
}

/// Execute a handler with retry logic.
///
/// The closure `f` is called up to `max_retries + 1` times. Retries occur when:
/// - The outcome has status [`attractor_types::StageStatus::Retry`]
/// - The error satisfies [`attractor_types::AttractorError::is_retryable`]
///
/// Between retries, the function sleeps for the duration dictated by `policy`.
pub async fn execute_with_retry<F, Fut>(
    f: F,
    max_retries: usize,
    policy: &BackoffPolicy,
    node_id: &str,
) -> attractor_types::Result<attractor_types::Outcome>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = attractor_types::Result<attractor_types::Outcome>>,
{
    let mut last_err = None;
    for attempt in 0..=max_retries {
        match f().await {
            Ok(outcome) => {
                if outcome.status == attractor_types::StageStatus::Retry && attempt < max_retries {
                    let delay = policy.delay_for_attempt(attempt);
                    tracing::info!(node = %node_id, attempt, delay_ms = %delay.as_millis(), "Retrying");
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Ok(outcome);
            }
            Err(e) if e.is_retryable() && attempt < max_retries => {
                last_err = Some(e);
                let delay = policy.delay_for_attempt(attempt);
                tracing::warn!(node = %node_id, attempt, delay_ms = %delay.as_millis(), "Retryable error, retrying");
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        attractor_types::AttractorError::RetriesExhausted {
            node: node_id.to_string(),
            attempts: max_retries + 1,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use attractor_types::{AttractorError, Outcome, StageStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // 1. No retries needed — success on first try
    #[tokio::test]
    async fn success_on_first_try() {
        let result = execute_with_retry(
            || async { Ok(Outcome::success("done")) },
            3,
            &BackoffPolicy::None,
            "node_a",
        )
        .await;

        let outcome = result.unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert_eq!(outcome.notes, "done");
    }

    // 2. Retry on retryable error succeeds on second try
    #[tokio::test]
    async fn retry_on_retryable_error_succeeds() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();

        let result = execute_with_retry(
            move || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        Err(AttractorError::RateLimited {
                            provider: "test".into(),
                            retry_after_ms: 100,
                        })
                    } else {
                        Ok(Outcome::success("recovered"))
                    }
                }
            },
            3,
            &BackoffPolicy::None,
            "node_b",
        )
        .await;

        let outcome = result.unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    // 3. Max retries exhausted returns error
    #[tokio::test]
    async fn max_retries_exhausted() {
        let result = execute_with_retry(
            || async {
                Err(AttractorError::RateLimited {
                    provider: "test".into(),
                    retry_after_ms: 0,
                })
            },
            2,
            &BackoffPolicy::None,
            "node_c",
        )
        .await;

        // After 2 retries (3 total attempts), the last attempt's error is returned directly
        // because attempt == max_retries, so the `Err(e) => return Err(e)` branch fires.
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AttractorError::RateLimited { .. }));
    }

    // 4. Fixed backoff returns constant delay
    #[test]
    fn fixed_backoff_constant_delay() {
        let policy = BackoffPolicy::Fixed(Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(5), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(100), Duration::from_millis(200));
    }

    // 5. Exponential backoff doubles correctly and respects max
    #[test]
    fn exponential_backoff_doubles_and_caps() {
        let policy = BackoffPolicy::Exponential {
            base: Duration::from_millis(100),
            max: Duration::from_millis(500),
        };
        // attempt 0: 100 * 2^0 = 100
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        // attempt 1: 100 * 2^1 = 200
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        // attempt 2: 100 * 2^2 = 400
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));
        // attempt 3: 100 * 2^3 = 800, capped at 500
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(500));
        // attempt 10: still capped at 500
        assert_eq!(policy.delay_for_attempt(10), Duration::from_millis(500));
    }

    // 6. Retry on Retry status outcome
    #[tokio::test]
    async fn retry_on_retry_status_outcome() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();

        let result = execute_with_retry(
            move || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Ok(Outcome::with_label(StageStatus::Retry, "retry_edge"))
                    } else {
                        Ok(Outcome::success("finally"))
                    }
                }
            },
            5,
            &BackoffPolicy::None,
            "node_d",
        )
        .await;

        let outcome = result.unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert_eq!(outcome.notes, "finally");
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    // 7. Non-retryable error is returned immediately without retrying
    #[tokio::test]
    async fn non_retryable_error_no_retry() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();

        let result = execute_with_retry(
            move || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err(AttractorError::AuthError {
                        provider: "test".into(),
                    })
                }
            },
            5,
            &BackoffPolicy::None,
            "node_e",
        )
        .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AttractorError::AuthError { .. }));
        // Only called once — no retries for non-retryable errors
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    // 8. BackoffPolicy::None returns zero duration
    #[test]
    fn none_backoff_zero_delay() {
        let policy = BackoffPolicy::None;
        assert_eq!(policy.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(policy.delay_for_attempt(99), Duration::ZERO);
    }

    // 9. Default backoff is exponential with expected values
    #[test]
    fn default_backoff_is_exponential() {
        let policy = BackoffPolicy::default();
        // attempt 0: 500ms
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(500));
        // attempt 1: 1000ms
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(1000));
        // large attempt: capped at 30s
        assert_eq!(policy.delay_for_attempt(20), Duration::from_secs(30));
    }

    // 10. Retry status on final attempt is returned as-is (not retried)
    #[tokio::test]
    async fn retry_status_on_final_attempt_returned() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();

        let result = execute_with_retry(
            move || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Ok(Outcome::with_label(StageStatus::Retry, "retry_edge"))
                }
            },
            2,
            &BackoffPolicy::None,
            "node_f",
        )
        .await;

        // All 3 attempts returned Retry; the last one is returned as-is
        let outcome = result.unwrap();
        assert_eq!(outcome.status, StageStatus::Retry);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }
}
