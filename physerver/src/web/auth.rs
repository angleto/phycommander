/// Authentication middleware for API endpoints

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// API key configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub api_keys: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_keys: vec![],
        }
    }
}

impl AuthConfig {
    pub fn with_key(mut self, key: String) -> Self {
        self.enabled = true;
        self.api_keys.push(key);
        self
    }

    pub fn validate_key(&self, key: &str) -> bool {
        if !self.enabled {
            return true; // Auth disabled
        }
        self.api_keys.iter().any(|k| k == key)
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    State(config): State<Arc<AuthConfig>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth if disabled
    if !config.enabled {
        return Ok(next.run(request).await);
    }

    // Check for API key in header
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            // Also check Authorization: Bearer <token>
            headers
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });

    match api_key {
        Some(key) if config.validate_key(key) => Ok(next.run(request).await),
        Some(_) => Err(StatusCode::UNAUTHORIZED),
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Rate limiting (simple token bucket implementation)
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    buckets: Mutex<HashMap<String, TokenBucket>>,
    max_tokens: u32,
    refill_rate: Duration,
}

struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(max_tokens: u32, refill_rate: Duration) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            max_tokens,
            refill_rate,
        }
    }

    pub fn check_rate_limit(&self, client_id: &str) -> bool {
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(client_id.to_string()).or_insert(TokenBucket {
            tokens: self.max_tokens,
            last_refill: Instant::now(),
        });

        // Refill tokens based on time elapsed
        let elapsed = bucket.last_refill.elapsed();
        let refills = (elapsed.as_millis() / self.refill_rate.as_millis()) as u32;
        if refills > 0 {
            bucket.tokens = (bucket.tokens + refills).min(self.max_tokens);
            bucket.last_refill = Instant::now();
        }

        // Check if we have tokens
        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            true
        } else {
            false
        }
    }
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Use client IP or API key as identifier
    let client_id = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("X-Forwarded-For").and_then(|v| v.to_str().ok()))
        .unwrap_or("unknown");

    if limiter.check_rate_limit(client_id) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_validation() {
        let config = AuthConfig::default()
            .with_key("test-key-123".to_string())
            .with_key("another-key".to_string());

        assert!(config.validate_key("test-key-123"));
        assert!(config.validate_key("another-key"));
        assert!(!config.validate_key("invalid-key"));
    }

    #[test]
    fn test_auth_config_disabled() {
        let config = AuthConfig::default();
        assert!(!config.enabled);
        assert!(config.validate_key("any-key")); // Should pass when disabled
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(5, Duration::from_millis(100));

        // Should allow first 5 requests
        for _ in 0..5 {
            assert!(limiter.check_rate_limit("client1"));
        }

        // 6th request should be rate limited
        assert!(!limiter.check_rate_limit("client1"));

        // Different client should have own bucket
        assert!(limiter.check_rate_limit("client2"));
    }

    #[test]
    fn test_rate_limiter_refill() {
        let limiter = RateLimiter::new(2, Duration::from_millis(50));

        // Consume tokens
        assert!(limiter.check_rate_limit("client"));
        assert!(limiter.check_rate_limit("client"));
        assert!(!limiter.check_rate_limit("client"));

        // Wait for refill
        std::thread::sleep(Duration::from_millis(100));

        // Should have tokens again
        assert!(limiter.check_rate_limit("client"));
    }
}
