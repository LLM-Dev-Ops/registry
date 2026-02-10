//! API error handling
//!
//! This module converts service errors into HTTP responses with appropriate
//! status codes and error messages.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use llm_registry_core::execution::ExecutionResult;
use llm_registry_service::ServiceError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// API error type that can be converted to HTTP responses
#[derive(Debug)]
pub struct ApiError {
    status_code: StatusCode,
    message: String,
    error_code: Option<String>,
    /// Execution spans to include in the error response (for agentics tracing).
    execution: Option<ExecutionResult>,
}

impl ApiError {
    /// Create a new API error
    pub fn new(status_code: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status_code,
            message: message.into(),
            error_code: None,
            execution: None,
        }
    }

    /// Create an API error with an error code
    pub fn with_code(
        status_code: StatusCode,
        message: impl Into<String>,
        error_code: impl Into<String>,
    ) -> Self {
        Self {
            status_code,
            message: message.into(),
            error_code: Some(error_code.into()),
            execution: None,
        }
    }

    /// Attach execution spans to this error so they are included in the
    /// response body (requirement: failed requests must still return spans).
    pub fn with_execution(mut self, execution: ExecutionResult) -> Self {
        self.execution = Some(execution);
        self
    }

    /// Create a bad request error (400)
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// Create a not found error (404)
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// Create a conflict error (409)
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    /// Create an unprocessable entity error (422)
    pub fn unprocessable_entity(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, message)
    }

    /// Create an internal server error (500)
    pub fn internal_server_error(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Create an unauthorized error (401)
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    /// Create a forbidden error (403)
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiError {}

/// Error response JSON structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// HTTP status code
    pub status: u16,

    /// Error message
    pub error: String,

    /// Optional error code for programmatic handling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Timestamp of the error
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Execution spans (present when the request had a valid execution context).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionResult>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let error_response = ErrorResponse {
            status: self.status_code.as_u16(),
            error: self.message,
            code: self.error_code,
            timestamp: chrono::Utc::now(),
            execution: self.execution,
        };

        (self.status_code, Json(error_response)).into_response()
    }
}

/// Convert ServiceError to ApiError
impl From<ServiceError> for ApiError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::NotFound(msg) => {
                ApiError::with_code(StatusCode::NOT_FOUND, msg, "NOT_FOUND")
            }
            ServiceError::AlreadyExists { name, version } => ApiError::with_code(
                StatusCode::CONFLICT,
                format!("Asset {}@{} already exists", name, version),
                "ALREADY_EXISTS",
            ),
            ServiceError::ValidationFailed(msg) => ApiError::with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Validation failed: {}", msg),
                "VALIDATION_FAILED",
            ),
            ServiceError::ChecksumVerificationFailed(msg) => ApiError::with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Checksum verification failed: {}", msg),
                "CHECKSUM_MISMATCH",
            ),
            ServiceError::CircularDependency(msg) => ApiError::with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Circular dependency detected: {}", msg),
                "CIRCULAR_DEPENDENCY",
            ),
            ServiceError::DependencyNotFound(msg) => ApiError::with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Dependency not found: {}", msg),
                "DEPENDENCY_NOT_FOUND",
            ),
            ServiceError::VersionConflict(msg) => ApiError::with_code(
                StatusCode::CONFLICT,
                format!("Version conflict: {}", msg),
                "VERSION_CONFLICT",
            ),
            ServiceError::PolicyValidationFailed {
                policy_name,
                message,
            } => ApiError::with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Policy '{}' validation failed: {}", policy_name, message),
                "POLICY_VALIDATION_FAILED",
            ),
            ServiceError::InvalidInput(msg) => {
                ApiError::with_code(StatusCode::BAD_REQUEST, msg, "INVALID_INPUT")
            }
            ServiceError::NotPermitted(msg) => {
                ApiError::with_code(StatusCode::FORBIDDEN, msg, "NOT_PERMITTED")
            }
            ServiceError::Database(msg) => ApiError::with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", msg),
                "DATABASE_ERROR",
            ),
            ServiceError::Internal(msg) => ApiError::with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal error: {}", msg),
                "INTERNAL_ERROR",
            ),
        }
    }
}

/// Convert common errors to ApiError
impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::bad_request(format!("Invalid JSON: {}", err))
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::internal_server_error(format!("I/O error: {}", err))
    }
}

/// Result type for API handlers
pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_creation() {
        let err = ApiError::bad_request("Invalid request");
        assert_eq!(err.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "Invalid request");
    }

    #[test]
    fn test_service_error_conversion() {
        let service_err = ServiceError::NotFound("asset-123".to_string());
        let api_err: ApiError = service_err.into();
        assert_eq!(api_err.status_code, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse {
            status: 404,
            error: "Not found".to_string(),
            code: Some("NOT_FOUND".to_string()),
            timestamp: chrono::Utc::now(),
            execution: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":404"));
        assert!(json.contains("\"error\":\"Not found\""));
    }
}
