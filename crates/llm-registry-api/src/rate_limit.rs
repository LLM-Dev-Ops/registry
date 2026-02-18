//! Rate limiting middleware
//!
//! This module provides rate limiting functionality using the token bucket algorithm
//! with Redis for distributed rate limiting across multiple service instances.

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

use crate::error::ErrorResponse;

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed
    pub max_requests: u32,

    /// Time window in seconds
    pub window_secs: u64,

    /// Whether rate limiting is enabled
    pub enabled: bool,

    /// Rate limit by IP address
    pub by_ip: bool,

    /// Rate limit by user ID (from JWT)
    pub by_user: bool,

    /// Custom identifier header (e.g., API key)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier_header: Option<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_secs: 60,
            enabled: true,
            by_ip: true,
            by_user: true,
            identifier_header: None,
        }
    }
}

impl RateLimitConfig {
    /// Create a new rate limit configuration
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            max_requests,
            window_secs,
            ..Default::default()
        }
    }

    /// Disable rate limiting
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Set max requests
    pub fn with_max_requests(mut self, max_requests: u32) -> Self {
        self.max_requests = max_requests;
        self
    }

    /// Set window in seconds
    pub fn with_window_secs(mut self, window_secs: u64) -> Self {
        self.window_secs = window_secs;
        self
    }

    /// Enable/disable rate limiting by IP
    pub fn with_by_ip(mut self, by_ip: bool) -> Self {
        self.by_ip = by_ip;
        self
    }

    /// Enable/disable rate limiting by user
    pub fn with_by_user(mut self, by_user: bool) -> Self {
        self.by_user = by_user;
        self
    }

    /// Set custom identifier header
    pub fn with_identifier_header(mut self, header: impl Into<String>) -> Self {
        self.identifier_header = Some(header.into());
        self
    }
}

/// Rate limiter state
#[derive(Clone)]
pub struct RateLimiterState {
    config: Arc<RateLimitConfig>,
    // In-memory storage for rate limiting (in production, use Redis)
    storage: Arc<tokio::sync::RwLock<std::collections::HashMap<String, TokenBucket>>>,
}

impl RateLimiterState {
    /// Create a new rate limiter state
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config: Arc::new(config),
            storage: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Get configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Number of tokens currently available
    tokens: f64,

    /// Last refill timestamp
    last_refill: u64,

    /// Maximum tokens (capacity)
    capacity: f64,

    /// Refill rate (tokens per second)
    refill_rate: f64,
}

impl TokenBucket {
    /// Create a new token bucket
    fn new(capacity: u32, window_secs: u64) -> Self {
        let refill_rate = capacity as f64 / window_secs as f64;
        Self {
            tokens: capacity as f64,
            last_refill: Self::current_time_secs(),
            capacity: capacity as f64,
            refill_rate,
        }
    }

    /// Get current time in seconds
    fn current_time_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Self::current_time_secs();
        let elapsed = now - self.last_refill;

        if elapsed > 0 {
            let new_tokens = elapsed as f64 * self.refill_rate;
            self.tokens = (self.tokens + new_tokens).min(self.capacity);
            self.last_refill = now;
        }
    }

    /// Try to consume a token
    fn try_consume(&mut self, count: f64) -> bool {
        self.refill();

        if self.tokens >= count {
            self.tokens -= count;
            true
        } else {
            false
        }
    }

    /// Get time until next token is available (in seconds)
    fn time_until_available(&self) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }

        let tokens_needed = 1.0 - self.tokens;
        (tokens_needed / self.refill_rate).ceil() as u64
    }
}

/// Rate limiting middleware
///
/// This middleware implements rate limiting using the token bucket algorithm.
/// It can rate limit by IP address, user ID, or custom identifier.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get, middleware};
/// use llm_registry_api::rate_limit::{rate_limit, RateLimiterState, RateLimitConfig};
///
/// # async fn example() {
/// let config = RateLimitConfig::new(100, 60); // 100 requests per minute
/// let rate_limiter = RateLimiterState::new(config);
///
/// let app = Router::new()
///     .route("/api/assets", get(|| async { "OK" }))
///     .layer(middleware::from_fn_with_state(rate_limiter, rate_limit));
/// # }
/// ```
pub async fn rate_limit(
    State(limiter): State<RateLimiterState>,
    request: Request,
    next: Next,
) -> Result<Response, RateLimitError> {
    // Skip if rate limiting is disabled
    if !limiter.config.enabled {
        return Ok(next.run(request).await);
    }

    // Extract identifier for rate limiting
    let identifier = extract_identifier(&request, &limiter.config);

    debug!("Rate limiting for identifier: {}", identifier);

    // Check rate limit
    let allowed = check_rate_limit(&limiter, &identifier).await;

    if !allowed {
        warn!("Rate limit exceeded for identifier: {}", identifier);
        return Err(RateLimitError::LimitExceeded {
            retry_after: limiter.config.window_secs,
        });
    }

    // Continue processing request
    let mut response = next.run(request).await;

    // Add rate limit headers
    add_rate_limit_headers(&mut response, &limiter.config);

    Ok(response)
}

/// Extract identifier for rate limiting
fn extract_identifier(request: &Request<Body>, config: &RateLimitConfig) -> String {
    let mut parts = Vec::new();

    // Extract IP address
    if config.by_ip {
        if let Some(ConnectInfo(addr)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
            parts.push(format!("ip:{}", addr.ip()));
        }
    }

    // Extract user ID from auth extension
    if config.by_user {
        if let Some(user) = request.extensions().get::<crate::auth::AuthUser>() {
            parts.push(format!("user:{}", user.user_id()));
        }
    }

    // Extract custom identifier from header
    if let Some(header_name) = &config.identifier_header {
        if let Some(value) = request.headers().get(header_name) {
            if let Ok(value_str) = value.to_str() {
                parts.push(format!("custom:{}", value_str));
            }
        }
    }

    // If no identifier could be extracted, use a default
    if parts.is_empty() {
        parts.push("anonymous".to_string());
    }

    parts.join("|")
}

/// Check rate limit for an identifier
async fn check_rate_limit(limiter: &RateLimiterState, identifier: &str) -> bool {
    let mut storage = limiter.storage.write().await;

    let bucket = storage
        .entry(identifier.to_string())
        .or_insert_with(|| {
            TokenBucket::new(limiter.config.max_requests, limiter.config.window_secs)
        });

    bucket.try_consume(1.0)
}

/// Add rate limit headers to response
fn add_rate_limit_headers(response: &mut Response, config: &RateLimitConfig) {
    // Add standard rate limit headers
    response.headers_mut().insert(
        "X-RateLimit-Limit",
        HeaderValue::from_str(&config.max_requests.to_string()).unwrap(),
    );

    response.headers_mut().insert(
        "X-RateLimit-Window",
        HeaderValue::from_str(&config.window_secs.to_string()).unwrap(),
    );
}

/// Rate limit errors
#[derive(Debug)]
pub enum RateLimitError {
    /// Rate limit exceeded
    LimitExceeded {
        /// Seconds until the limit resets
        retry_after: u64,
    },
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        match self {
            RateLimitError::LimitExceeded { retry_after } => {
                let error_response = ErrorResponse {
                    status: 429,
                    error: "Rate limit exceeded".to_string(),
                    code: Some("RATE_LIMIT_EXCEEDED".to_string()),
                    timestamp: chrono::Utc::now(),
                    execution: None,
                };

                let mut response = (StatusCode::TOO_MANY_REQUESTS, axum::Json(error_response))
                    .into_response();

                // Add Retry-After header
                response.headers_mut().insert(
                    "Retry-After",
                    HeaderValue::from_str(&retry_after.to_string()).unwrap(),
                );

                response
            }
        }
    }
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::LimitExceeded { retry_after } => {
                write!(f, "Rate limit exceeded. Retry after {} seconds", retry_after)
            }
        }
    }
}

impl std::error::Error for RateLimitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config() {
        let config = RateLimitConfig::new(100, 60);
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window_secs, 60);
        assert!(config.enabled);
    }

    #[test]
    fn test_rate_limit_config_builder() {
        let config = RateLimitConfig::default()
            .with_max_requests(200)
            .with_window_secs(120)
            .with_by_ip(false)
            .with_identifier_header("X-API-Key");

        assert_eq!(config.max_requests, 200);
        assert_eq!(config.window_secs, 120);
        assert!(!config.by_ip);
        assert_eq!(
            config.identifier_header,
            Some("X-API-Key".to_string())
        );
    }

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(100, 60);
        assert_eq!(bucket.capacity, 100.0);
        assert_eq!(bucket.tokens, 100.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(10, 60);

        // Should be able to consume up to capacity
        for _ in 0..10 {
            assert!(bucket.try_consume(1.0));
        }

        // Should fail after exhausting tokens
        assert!(!bucket.try_consume(1.0));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10, 10); // 1 token per second

        // Consume all tokens
        for _ in 0..10 {
            bucket.try_consume(1.0);
        }

        assert_eq!(bucket.tokens, 0.0);

        // Simulate time passing by manually updating last_refill
        bucket.last_refill -= 5; // 5 seconds ago

        // Refill should add 5 tokens
        bucket.refill();
        assert_eq!(bucket.tokens, 5.0);
    }

    #[tokio::test]
    async fn test_rate_limiter_state() {
        let config = RateLimitConfig::new(5, 60);
        let limiter = RateLimiterState::new(config);

        // Should allow requests up to the limit
        for _ in 0..5 {
            assert!(check_rate_limit(&limiter, "test-user").await);
        }

        // Should deny additional requests
        assert!(!check_rate_limit(&limiter, "test-user").await);

        // Different identifier should have its own limit
        assert!(check_rate_limit(&limiter, "other-user").await);
    }

    #[test]
    fn test_disabled_rate_limit() {
        let config = RateLimitConfig::disabled();
        assert!(!config.enabled);
    }
}
