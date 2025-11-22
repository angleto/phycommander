/// Error recovery and resilience patterns

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Circuit breaker for fault tolerance
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    config: CircuitConfig,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_state_change: Arc<RwLock<Instant>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,      // Normal operation
    Open,        // Failing, reject requests
    HalfOpen,    // Testing if recovered
}

#[derive(Debug, Clone)]
pub struct CircuitConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout: Duration,
    pub half_open_max_calls: u32,
}

impl Default for CircuitConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            config,
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_state_change: Arc::new(RwLock::new(Instant::now())),
        }
    }

    pub async fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        // Check if we should attempt the call
        if !self.should_attempt().await {
            return Err(CircuitBreakerError::CircuitOpen);
        }

        // Execute the call
        match f() {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(e) => {
                self.on_failure().await;
                Err(CircuitBreakerError::CallFailed(e))
            }
        }
    }

    async fn should_attempt(&self) -> bool {
        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                let last_change = *self.last_state_change.read().await;
                if last_change.elapsed() >= self.config.timeout {
                    // Transition to half-open
                    *self.state.write().await = CircuitState::HalfOpen;
                    *self.last_state_change.write().await = Instant::now();
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited calls in half-open state
                self.success_count.load(Ordering::Relaxed) < self.config.half_open_max_calls
            }
        }
    }

    async fn on_success(&self) {
        let state = *self.state.read().await;
        self.success_count.fetch_add(1, Ordering::Relaxed);

        match state {
            CircuitState::HalfOpen => {
                if self.success_count.load(Ordering::Relaxed) >= self.config.success_threshold {
                    // Recovered, close circuit
                    *self.state.write().await = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                    *self.last_state_change.write().await = Instant::now();
                    tracing::info!("Circuit breaker closed - service recovered");
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    async fn on_failure(&self) {
        let state = *self.state.read().await;
        self.failure_count.fetch_add(1, Ordering::Relaxed);

        match state {
            CircuitState::Closed => {
                if self.failure_count.load(Ordering::Relaxed) >= self.config.failure_threshold {
                    // Open circuit
                    *self.state.write().await = CircuitState::Open;
                    *self.last_state_change.write().await = Instant::now();
                    tracing::warn!("Circuit breaker opened - too many failures");
                }
            }
            CircuitState::HalfOpen => {
                // Failed during test, go back to open
                *self.state.write().await = CircuitState::Open;
                self.success_count.store(0, Ordering::Relaxed);
                *self.last_state_change.write().await = Instant::now();
                tracing::warn!("Circuit breaker reopened - test failed");
            }
            _ => {}
        }
    }

    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }
}

#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    CircuitOpen,
    CallFailed(E),
}

/// Exponential backoff for retries
pub struct ExponentialBackoff {
    base_delay: Duration,
    max_delay: Duration,
    max_retries: u32,
    current_retry: AtomicU32,
}

impl ExponentialBackoff {
    pub fn new(base_delay: Duration, max_delay: Duration, max_retries: u32) -> Self {
        Self {
            base_delay,
            max_delay,
            max_retries,
            current_retry: AtomicU32::new(0),
        }
    }

    pub fn next_delay(&self) -> Option<Duration> {
        let retry = self.current_retry.fetch_add(1, Ordering::Relaxed);

        if retry >= self.max_retries {
            return None;
        }

        // Calculate exponential delay: base * 2^retry
        let delay_ms = self.base_delay.as_millis() * (2_u128.pow(retry));
        let delay = Duration::from_millis(delay_ms.min(self.max_delay.as_millis()) as u64);

        Some(delay)
    }

    pub fn reset(&self) {
        self.current_retry.store(0, Ordering::Relaxed);
    }
}

/// Retry with exponential backoff
pub async fn retry_with_backoff<F, T, E, Fut>(
    backoff: &ExponentialBackoff,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    loop {
        match f().await {
            Ok(result) => {
                backoff.reset();
                return Ok(result);
            }
            Err(e) => {
                if let Some(delay) = backoff.next_delay() {
                    tracing::warn!("Operation failed, retrying in {:?}", delay);
                    tokio::time::sleep(delay).await;
                } else {
                    tracing::error!("Max retries exceeded");
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_opens() {
        let config = CircuitConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            half_open_max_calls: 2,
        };
        let cb = CircuitBreaker::new(config);

        // Simulate failures
        for _ in 0..3 {
            let _ = cb.call(|| Result::<(), &str>::Err("error")).await;
        }

        assert_eq!(cb.get_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovers() {
        let config = CircuitConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            half_open_max_calls: 3,
        };
        let cb = CircuitBreaker::new(config);

        // Open circuit
        for _ in 0..2 {
            let _ = cb.call(|| Result::<(), &str>::Err("error")).await;
        }
        assert_eq!(cb.get_state().await, CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Successful calls should close circuit
        for _ in 0..2 {
            let _ = cb.call(|| Result::<(), &str>::Ok(())).await;
        }

        assert_eq!(cb.get_state().await, CircuitState::Closed);
    }

    #[test]
    fn test_exponential_backoff() {
        let backoff = ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
            5,
        );

        let delay1 = backoff.next_delay().unwrap();
        assert_eq!(delay1.as_millis(), 100);

        let delay2 = backoff.next_delay().unwrap();
        assert_eq!(delay2.as_millis(), 200);

        let delay3 = backoff.next_delay().unwrap();
        assert_eq!(delay3.as_millis(), 400);
    }

    #[test]
    fn test_backoff_max_retries() {
        let backoff = ExponentialBackoff::new(
            Duration::from_millis(10),
            Duration::from_secs(1),
            3,
        );

        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_none()); // Max retries reached
    }
}
