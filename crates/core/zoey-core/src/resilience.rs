//! Resilience patterns: Circuit breakers, health checks, retry logic

use crate::{ZoeyError, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests fail fast
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker for preventing cascading failures
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_threshold: usize,
    success_threshold: usize,
    timeout: Duration,
    failure_count: Arc<RwLock<usize>>,
    success_count: Arc<RwLock<usize>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(failure_threshold: usize, success_threshold: usize, timeout: Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_threshold,
            success_threshold,
            timeout,
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
        }
    }

    /// Execute a function through the circuit breaker
    pub async fn call<F, T, E>(&self, f: F) -> Result<T>
    where
        F: std::future::Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Display,
    {
        // Check if circuit is open
        {
            let state = *self.state.read().unwrap();
            if state == CircuitState::Open {
                // Check if timeout has elapsed
                if let Some(last_failure) = *self.last_failure_time.read().unwrap() {
                    if last_failure.elapsed() >= self.timeout {
                        // Transition to half-open
                        *self.state.write().unwrap() = CircuitState::HalfOpen;
                        *self.success_count.write().unwrap() = 0;
                        debug!("Circuit breaker transitioning to half-open");
                    } else {
                        return Err(ZoeyError::other("Circuit breaker is open"));
                    }
                }
            }
        }

        // Execute the function
        match f.await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(e) => {
                self.on_failure();
                Err(ZoeyError::other(e.to_string()))
            }
        }
    }

    /// Handle successful call
    fn on_success(&self) {
        let state = *self.state.read().unwrap();

        match state {
            CircuitState::HalfOpen => {
                let mut success_count = self.success_count.write().unwrap();
                *success_count += 1;

                if *success_count >= self.success_threshold {
                    *self.state.write().unwrap() = CircuitState::Closed;
                    *self.failure_count.write().unwrap() = 0;
                    debug!("Circuit breaker closed");
                }
            }
            CircuitState::Closed => {
                *self.failure_count.write().unwrap() = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Handle failed call
    fn on_failure(&self) {
        let state = *self.state.read().unwrap();

        match state {
            CircuitState::Closed | CircuitState::HalfOpen => {
                let mut failure_count = self.failure_count.write().unwrap();
                *failure_count += 1;

                if *failure_count >= self.failure_threshold {
                    *self.state.write().unwrap() = CircuitState::Open;
                    *self.last_failure_time.write().unwrap() = Some(Instant::now());
                    warn!("Circuit breaker opened after {} failures", failure_count);
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        *self.state.read().unwrap()
    }

    /// Reset the circuit breaker
    pub fn reset(&self) {
        *self.state.write().unwrap() = CircuitState::Closed;
        *self.failure_count.write().unwrap() = 0;
        *self.success_count.write().unwrap() = 0;
        *self.last_failure_time.write().unwrap() = None;
        debug!("Circuit breaker reset");
    }
}

/// Health check result
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is degraded but operational
    Degraded,
    /// Service is unhealthy
    Unhealthy,
}

/// Health check information
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// Component name
    pub name: String,
    /// Health status
    pub status: HealthStatus,
    /// Optional message
    pub message: Option<String>,
    /// Last check timestamp
    pub last_check: Instant,
    /// Response time in milliseconds
    pub response_time_ms: u64,
}

/// Health checker for monitoring system health
pub struct HealthChecker {
    checks: Arc<RwLock<HashMap<String, HealthCheck>>>,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self {
            checks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a health check
    pub async fn check<F, T, E>(&self, name: &str, f: F) -> HealthStatus
    where
        F: std::future::Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Display,
    {
        let start = Instant::now();

        let status = match f.await {
            Ok(_) => HealthStatus::Healthy,
            Err(e) => {
                error!("Health check failed for {}: {}", name, e);
                HealthStatus::Unhealthy
            }
        };

        let response_time_ms = start.elapsed().as_millis() as u64;

        // Determine degraded status based on response time
        let final_status = if status == HealthStatus::Healthy && response_time_ms > 1000 {
            HealthStatus::Degraded
        } else {
            status
        };

        let check = HealthCheck {
            name: name.to_string(),
            status: final_status,
            message: None,
            last_check: Instant::now(),
            response_time_ms,
        };

        self.checks.write().unwrap().insert(name.to_string(), check);

        final_status
    }

    /// Get overall health status
    pub fn overall_health(&self) -> HealthStatus {
        let checks = self.checks.read().unwrap();

        if checks.is_empty() {
            return HealthStatus::Healthy;
        }

        let mut has_unhealthy = false;
        let mut has_degraded = false;

        for check in checks.values() {
            match check.status {
                HealthStatus::Unhealthy => has_unhealthy = true,
                HealthStatus::Degraded => has_degraded = true,
                HealthStatus::Healthy => {}
            }
        }

        if has_unhealthy {
            HealthStatus::Unhealthy
        } else if has_degraded {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    /// Get all health checks
    pub fn get_all_checks(&self) -> Vec<HealthCheck> {
        self.checks.read().unwrap().values().cloned().collect()
    }

    /// Get specific health check
    pub fn get_check(&self, name: &str) -> Option<HealthCheck> {
        self.checks.read().unwrap().get(name).cloned()
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: usize,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }
}

/// Execute a function with retry logic
pub async fn retry_with_backoff<F, T, E>(config: RetryConfig, mut f: F) -> Result<T>
where
    F: FnMut() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<T, E>> + Send>,
    >,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    let mut delay = config.initial_delay;

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempt += 1;

                if attempt > config.max_retries {
                    error!("All {} retry attempts failed", config.max_retries);
                    return Err(ZoeyError::other(format!(
                        "Retry failed after {} attempts: {}",
                        config.max_retries, e
                    )));
                }

                warn!("Attempt {} failed: {}. Retrying in {:?}", attempt, e, delay);
                tokio::time::sleep(delay).await;

                // Exponential backoff
                delay = Duration::from_millis(
                    ((delay.as_millis() as f64) * config.multiplier)
                        .min(config.max_delay.as_millis() as f64) as u64,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_closed() {
        let cb = CircuitBreaker::new(3, 2, Duration::from_secs(5));
        assert_eq!(cb.state(), CircuitState::Closed);

        // Successful call
        let result = cb.call(async { Ok::<_, String>(42) }).await;
        assert!(result.is_ok());
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens() {
        let cb = CircuitBreaker::new(3, 2, Duration::from_secs(5));

        // Three failures should open the circuit
        for _ in 0..3 {
            let _ = cb.call(async { Err::<(), _>("error") }).await;
        }

        assert_eq!(cb.state(), CircuitState::Open);

        // Next call should fail fast
        let result = cb.call(async { Ok::<_, String>(42) }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_checker() {
        let checker = HealthChecker::new();

        // Healthy check
        let status = checker
            .check("test_service", async { Ok::<_, String>(()) })
            .await;
        assert_eq!(status, HealthStatus::Healthy);

        // Unhealthy check
        let status = checker
            .check("failing_service", async { Err::<(), _>("error") })
            .await;
        assert_eq!(status, HealthStatus::Unhealthy);

        // Overall health should be unhealthy
        assert_eq!(checker.overall_health(), HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_retry_success() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
        };

        let mut attempts = 0;
        let result = retry_with_backoff(config, || {
            attempts += 1;
            Box::pin(async move {
                if attempts < 2 {
                    Err("not yet")
                } else {
                    Ok(42)
                }
            })
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_failure() {
        let config = RetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
        };

        let result =
            retry_with_backoff(config, || Box::pin(async { Err::<(), _>("always fails") })).await;

        assert!(result.is_err());
    }
}
