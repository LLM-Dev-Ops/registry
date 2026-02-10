//! API response types
//!
//! This module defines standard response wrappers and helper functions
//! for creating consistent HTTP responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use llm_registry_core::execution::ExecutionResult;
use serde::{Deserialize, Serialize};

/// Standard success response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Response data
    pub data: T,

    /// Response metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

/// Response metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMeta {
    /// Request ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Timestamp of response
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Additional metadata fields
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl ResponseMeta {
    /// Create new response metadata
    pub fn new() -> Self {
        Self {
            request_id: None,
            timestamp: chrono::Utc::now(),
            extra: std::collections::HashMap::new(),
        }
    }

    /// Set request ID
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Add extra field
    pub fn with_extra(mut self, key: String, value: serde_json::Value) -> Self {
        self.extra.insert(key, value);
        self
    }
}

impl Default for ResponseMeta {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ApiResponse<T> {
    /// Create a new API response
    pub fn new(data: T) -> Self {
        Self { data, meta: None }
    }

    /// Create a response with metadata
    pub fn with_meta(data: T, meta: ResponseMeta) -> Self {
        Self {
            data,
            meta: Some(meta),
        }
    }
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// Paginated response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// List of items
    pub items: Vec<T>,

    /// Pagination metadata
    pub pagination: PaginationMeta,
}

/// Pagination metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationMeta {
    /// Total number of items (without pagination)
    pub total: i64,

    /// Current offset
    pub offset: i64,

    /// Current limit
    pub limit: i64,

    /// Whether there are more results
    pub has_more: bool,
}

impl<T> PaginatedResponse<T> {
    /// Create a new paginated response
    pub fn new(items: Vec<T>, total: i64, offset: i64, limit: i64) -> Self {
        let has_more = offset + items.len() as i64 > total.min(offset + limit);

        Self {
            items,
            pagination: PaginationMeta {
                total,
                offset,
                limit,
                has_more,
            },
        }
    }
}

impl<T> IntoResponse for PaginatedResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// Empty response for operations with no return data
#[derive(Debug, Serialize, Deserialize)]
pub struct EmptyResponse {
    /// Success message
    pub message: String,
}

impl EmptyResponse {
    /// Create a new empty response
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Create a default success response
    pub fn success() -> Self {
        Self {
            message: "Success".to_string(),
        }
    }
}

impl IntoResponse for EmptyResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: HealthStatus,

    /// Service version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Component health checks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<std::collections::HashMap<String, ComponentHealth>>,
}

/// Health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is degraded but operational
    Degraded,
    /// Service is unhealthy
    Unhealthy,
}

/// Component health status
#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component status
    pub status: HealthStatus,

    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Optional metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl HealthResponse {
    /// Create a healthy response
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            version: None,
            checks: None,
        }
    }

    /// Create a response with version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Add a component health check
    pub fn with_check(mut self, name: impl Into<String>, health: ComponentHealth) -> Self {
        if self.checks.is_none() {
            self.checks = Some(std::collections::HashMap::new());
        }
        self.checks.as_mut().unwrap().insert(name.into(), health);
        self
    }

    /// Determine overall health status from component checks
    pub fn compute_status(mut self) -> Self {
        if let Some(checks) = &self.checks {
            let has_unhealthy = checks.values().any(|c| c.status == HealthStatus::Unhealthy);
            let has_degraded = checks.values().any(|c| c.status == HealthStatus::Degraded);

            self.status = if has_unhealthy {
                HealthStatus::Unhealthy
            } else if has_degraded {
                HealthStatus::Degraded
            } else {
                HealthStatus::Healthy
            };
        }
        self
    }
}

impl IntoResponse for HealthResponse {
    fn into_response(self) -> Response {
        let status_code = match self.status {
            HealthStatus::Healthy => StatusCode::OK,
            HealthStatus::Degraded => StatusCode::OK, // Still 200 but degraded
            HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
        };

        (status_code, Json(self)).into_response()
    }
}

impl ComponentHealth {
    /// Create a healthy component
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: None,
            metrics: None,
        }
    }

    /// Create a degraded component
    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Degraded,
            message: Some(message.into()),
            metrics: None,
        }
    }

    /// Create an unhealthy component
    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: Some(message.into()),
            metrics: None,
        }
    }

    /// Add metrics
    pub fn with_metrics(
        mut self,
        metrics: std::collections::HashMap<String, serde_json::Value>,
    ) -> Self {
        self.metrics = Some(metrics);
        self
    }
}

/// Helper function to create a success response
pub fn ok<T>(data: T) -> ApiResponse<T> {
    ApiResponse::new(data)
}

/// Helper function to create a created response (201)
pub fn created<T>(data: T) -> (StatusCode, Json<ApiResponse<T>>)
where
    T: Serialize,
{
    (StatusCode::CREATED, Json(ApiResponse::new(data)))
}

/// Helper function to create a no content response (204)
pub fn no_content() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// Helper function to create a deleted response
pub fn deleted() -> (StatusCode, Json<EmptyResponse>) {
    (
        StatusCode::OK,
        Json(EmptyResponse::new("Resource deleted successfully")),
    )
}

// ============================================================================
// Agentics Execution Envelope
// ============================================================================

/// Response envelope that wraps data alongside the execution span tree.
///
/// Every `/v1/*` response includes the full span hierarchy (repo + agent spans)
/// so that the calling Core can reconstruct the execution graph.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionEnvelope<T> {
    /// Original response data.
    pub data: T,
    /// Execution trace (repo span + nested agent spans).
    pub execution: ExecutionResult,
    /// Optional response metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

impl<T> ExecutionEnvelope<T> {
    pub fn new(data: T, execution: ExecutionResult) -> Self {
        Self {
            data,
            execution,
            meta: None,
        }
    }
}

impl<T> IntoResponse for ExecutionEnvelope<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// Paginated response with execution spans.
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedExecutionEnvelope<T> {
    /// List of items.
    pub items: Vec<T>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
    /// Execution trace.
    pub execution: ExecutionResult,
}

impl<T> IntoResponse for PaginatedExecutionEnvelope<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// Helper: wrap data + execution into a 200 OK envelope.
pub fn ok_with_execution<T>(data: T, execution: ExecutionResult) -> Json<ExecutionEnvelope<T>> {
    Json(ExecutionEnvelope::new(data, execution))
}

/// Helper: wrap data + execution into a 201 Created envelope.
pub fn created_with_execution<T: Serialize>(
    data: T,
    execution: ExecutionResult,
) -> (StatusCode, Json<ExecutionEnvelope<T>>) {
    (StatusCode::CREATED, Json(ExecutionEnvelope::new(data, execution)))
}

/// Helper: deleted response with execution spans.
pub fn deleted_with_execution(
    execution: ExecutionResult,
) -> (StatusCode, Json<ExecutionEnvelope<EmptyResponse>>) {
    (
        StatusCode::OK,
        Json(ExecutionEnvelope::new(
            EmptyResponse::new("Resource deleted successfully"),
            execution,
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_creation() {
        let response = ApiResponse::new("test data");
        assert_eq!(response.data, "test data");
        assert!(response.meta.is_none());
    }

    #[test]
    fn test_paginated_response() {
        let items = vec![1, 2, 3];
        let response = PaginatedResponse::new(items, 10, 0, 5);

        assert_eq!(response.items.len(), 3);
        assert_eq!(response.pagination.total, 10);
        assert_eq!(response.pagination.offset, 0);
        assert_eq!(response.pagination.limit, 5);
    }

    #[test]
    fn test_health_response_status_computation() {
        let response = HealthResponse::healthy()
            .with_check("db", ComponentHealth::healthy())
            .with_check("cache", ComponentHealth::degraded("Slow response"))
            .compute_status();

        assert_eq!(response.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_response_meta() {
        let meta = ResponseMeta::new()
            .with_request_id("req-123".to_string())
            .with_extra("key".to_string(), serde_json::json!("value"));

        assert_eq!(meta.request_id, Some("req-123".to_string()));
        assert!(meta.extra.contains_key("key"));
    }
}
