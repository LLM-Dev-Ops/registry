//! Agentics execution context middleware
//!
//! This middleware enforces that every `/v1/*` request carries a valid execution
//! context from the calling Core. Requests without the required headers are
//! rejected with 400 Bad Request.
//!
//! On success the middleware inserts an [`ExecutionContext`] and a
//! [`SpanCollector`] (with the repo-level span already started) into the
//! request extensions, where downstream handlers can extract them.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use llm_registry_core::execution::{
    ExecutionContext, ExecutionId, SpanCollector, SpanId,
};
use tracing::debug;

use crate::error::ErrorResponse;

/// Header name for the execution-wide identifier.
pub const HEADER_EXECUTION_ID: &str = "x-execution-id";
/// Header name for the parent span ID from the calling Core.
pub const HEADER_PARENT_SPAN_ID: &str = "x-parent-span-id";

/// Middleware that enforces execution context headers on `/v1/*` routes.
///
/// Follows the same pattern as [`crate::auth::require_auth`]:
/// extract from headers → validate → insert into extensions → call next.
pub async fn require_execution_context(
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    let headers = request.headers();

    // Extract X-Execution-Id
    let execution_id = headers
        .get(HEADER_EXECUTION_ID)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            missing_header_response("Missing required header: X-Execution-Id")
        })?;

    // Extract X-Parent-Span-Id
    let parent_span_id_str = headers
        .get(HEADER_PARENT_SPAN_ID)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            missing_header_response("Missing required header: X-Parent-Span-Id")
        })?;

    let parent_span_id = SpanId::from_string(parent_span_id_str).map_err(|e| {
        invalid_header_response(&format!("Invalid X-Parent-Span-Id: {}", e))
    })?;

    let ctx = ExecutionContext {
        execution_id: ExecutionId::new(execution_id),
        parent_span_id,
    };

    debug!(
        execution_id = %ctx.execution_id,
        parent_span_id = %ctx.parent_span_id,
        "Execution context extracted"
    );

    // Create span collector (repo-level span started automatically)
    let collector = SpanCollector::new(&ctx);

    // Insert into request extensions for handler extraction
    request.extensions_mut().insert(ctx);
    request.extensions_mut().insert(collector);

    Ok(next.run(request).await)
}

fn missing_header_response(message: &str) -> Response {
    let body = ErrorResponse {
        status: 400,
        error: message.to_string(),
        code: Some("MISSING_EXECUTION_CONTEXT".to_string()),
        timestamp: chrono::Utc::now(),
        execution: None,
    };
    (StatusCode::BAD_REQUEST, Json(body)).into_response()
}

fn invalid_header_response(message: &str) -> Response {
    let body = ErrorResponse {
        status: 400,
        error: message.to_string(),
        code: Some("INVALID_EXECUTION_CONTEXT".to_string()),
        timestamp: chrono::Utc::now(),
        execution: None,
    };
    (StatusCode::BAD_REQUEST, Json(body)).into_response()
}
