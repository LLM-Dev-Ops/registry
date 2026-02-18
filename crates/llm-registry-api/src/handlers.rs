//! API request handlers
//!
//! This module implements HTTP request handlers for all API endpoints.
//! Every `/v1/*` handler extracts the [`SpanCollector`] injected by the
//! execution middleware, creates agent-level spans for each service
//! invocation, attaches artifacts, and returns an [`ExecutionEnvelope`].

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use llm_registry_core::execution::{SpanArtifact, SpanCollector, SpanStatus};
use llm_registry_core::AssetId;
use llm_registry_service::{
    GetDependencyGraphRequest, RegisterAssetRequest, SearchAssetsRequest, ServiceRegistry,
    UpdateAssetRequest,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, instrument};

use crate::{
    error::{ApiError, ApiResult},
    responses::{
        created_with_execution, deleted_with_execution, ok_with_execution, ComponentHealth,
        ExecutionEnvelope, HealthResponse, PaginatedExecutionEnvelope, PaginationMeta,
    },
};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Service registry
    pub services: Arc<ServiceRegistry>,
}

impl AppState {
    /// Create new application state
    pub fn new(services: ServiceRegistry) -> Self {
        Self {
            services: Arc::new(services),
        }
    }
}

// ============================================================================
// Asset Management Handlers
// ============================================================================

/// Register a new asset
#[instrument(skip(state, collector))]
pub async fn register_asset(
    State(state): State<AppState>,
    Extension(collector): Extension<SpanCollector>,
    Json(request): Json<RegisterAssetRequest>,
) -> ApiResult<(StatusCode, Json<ExecutionEnvelope<llm_registry_service::RegisterAssetResponse>>)> {
    info!(
        "Registering asset: {}@{}",
        request.name, request.version
    );

    let span_id = collector.begin_agent_span("RegistrationService");

    let result = state
        .services
        .registration()
        .register_asset(request)
        .await;

    match result {
        Ok(response) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "registered_asset".to_string(),
                    content_type: Some("application/json".to_string()),
                    data: serde_json::to_value(&response.asset).unwrap_or_default(),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Ok);
            let exec = collector.finalize();
            Ok(created_with_execution(response, exec))
        }
        Err(e) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(e.to_string()),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::from(e).with_execution(exec))
        }
    }
}

/// Get asset by ID
#[instrument(skip(state, collector))]
pub async fn get_asset(
    State(state): State<AppState>,
    Extension(collector): Extension<SpanCollector>,
    Path(id): Path<String>,
) -> ApiResult<Json<ExecutionEnvelope<llm_registry_core::Asset>>> {
    debug!("Getting asset: {}", id);

    let asset_id = id.parse::<AssetId>().map_err(|e| {
        let err = ApiError::bad_request(format!("Invalid asset ID: {}", e));
        let exec = collector.finalize_failed("Invalid asset ID");
        err.with_execution(exec)
    })?;

    let span_id = collector.begin_agent_span("SearchService");

    let result = state
        .services
        .search()
        .get_asset(&asset_id)
        .await;

    match result {
        Ok(Some(asset)) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "asset".to_string(),
                    content_type: Some("application/json".to_string()),
                    data: serde_json::to_value(&asset).unwrap_or_default(),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Ok);
            let exec = collector.finalize();
            Ok(ok_with_execution(asset, exec))
        }
        Ok(None) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(format!("Asset not found: {}", id)),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::not_found(format!("Asset not found: {}", id)).with_execution(exec))
        }
        Err(e) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(e.to_string()),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::from(e).with_execution(exec))
        }
    }
}

/// List/search assets with pagination
#[instrument(skip(state, collector))]
pub async fn list_assets(
    State(state): State<AppState>,
    Extension(collector): Extension<SpanCollector>,
    Query(params): Query<SearchAssetsRequest>,
) -> ApiResult<Json<PaginatedExecutionEnvelope<llm_registry_core::Asset>>> {
    debug!("Searching assets with filters: {:?}", params);

    let span_id = collector.begin_agent_span("SearchService");

    let result = state
        .services
        .search()
        .search_assets(params)
        .await;

    match result {
        Ok(response) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "search_results".to_string(),
                    content_type: Some("application/json".to_string()),
                    data: serde_json::json!({
                        "total": response.total,
                        "count": response.assets.len(),
                    }),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Ok);
            let exec = collector.finalize();

            let has_more = response.offset + response.assets.len() as i64
                > response.total.min(response.offset + response.limit);

            Ok(Json(PaginatedExecutionEnvelope {
                items: response.assets,
                pagination: PaginationMeta {
                    total: response.total,
                    offset: response.offset,
                    limit: response.limit,
                    has_more,
                },
                execution: exec,
            }))
        }
        Err(e) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(e.to_string()),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::from(e).with_execution(exec))
        }
    }
}

/// Update asset metadata
#[instrument(skip(state, collector))]
pub async fn update_asset(
    State(state): State<AppState>,
    Extension(collector): Extension<SpanCollector>,
    Path(id): Path<String>,
    Json(mut request): Json<UpdateAssetRequest>,
) -> ApiResult<Json<ExecutionEnvelope<llm_registry_service::UpdateAssetResponse>>> {
    info!("Updating asset: {}", id);

    let asset_id = id.parse::<AssetId>().map_err(|e| {
        let err = ApiError::bad_request(format!("Invalid asset ID: {}", e));
        let exec = collector.finalize_failed("Invalid asset ID");
        err.with_execution(exec)
    })?;

    // Set asset ID from path
    request.asset_id = asset_id;

    let span_id = collector.begin_agent_span("RegistrationService");

    let result = state
        .services
        .registration()
        .update_asset(request)
        .await;

    match result {
        Ok(response) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "updated_asset".to_string(),
                    content_type: Some("application/json".to_string()),
                    data: serde_json::to_value(&response.asset).unwrap_or_default(),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Ok);
            let exec = collector.finalize();
            Ok(ok_with_execution(response, exec))
        }
        Err(e) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(e.to_string()),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::from(e).with_execution(exec))
        }
    }
}

/// Delete asset
#[instrument(skip(state, collector))]
pub async fn delete_asset(
    State(state): State<AppState>,
    Extension(collector): Extension<SpanCollector>,
    Path(id): Path<String>,
) -> ApiResult<(StatusCode, Json<ExecutionEnvelope<crate::responses::EmptyResponse>>)> {
    info!("Deleting asset: {}", id);

    let asset_id = id.parse::<AssetId>().map_err(|e| {
        let err = ApiError::bad_request(format!("Invalid asset ID: {}", e));
        let exec = collector.finalize_failed("Invalid asset ID");
        err.with_execution(exec)
    })?;

    let span_id = collector.begin_agent_span("RegistrationService");

    let result = state
        .services
        .registration()
        .delete_asset(&asset_id)
        .await;

    match result {
        Ok(()) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "deleted_asset_id".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(id),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Ok);
            let exec = collector.finalize();
            Ok(deleted_with_execution(exec))
        }
        Err(e) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(e.to_string()),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::from(e).with_execution(exec))
        }
    }
}

// ============================================================================
// Dependency Handlers
// ============================================================================

/// Get dependency graph for an asset
#[instrument(skip(state, collector))]
pub async fn get_dependencies(
    State(state): State<AppState>,
    Extension(collector): Extension<SpanCollector>,
    Path(id): Path<String>,
    Query(params): Query<DependencyGraphParams>,
) -> ApiResult<Json<ExecutionEnvelope<llm_registry_service::DependencyGraphResponse>>> {
    debug!("Getting dependency graph for asset: {}", id);

    let asset_id = id.parse::<AssetId>().map_err(|e| {
        let err = ApiError::bad_request(format!("Invalid asset ID: {}", e));
        let exec = collector.finalize_failed("Invalid asset ID");
        err.with_execution(exec)
    })?;

    let request = GetDependencyGraphRequest {
        asset_id,
        max_depth: params.max_depth.unwrap_or(-1),
    };

    let span_id = collector.begin_agent_span("SearchService");

    let result = state
        .services
        .search()
        .get_dependency_graph(request)
        .await;

    match result {
        Ok(response) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "dependency_graph".to_string(),
                    content_type: Some("application/json".to_string()),
                    data: serde_json::to_value(&response).unwrap_or_default(),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Ok);
            let exec = collector.finalize();
            Ok(ok_with_execution(response, exec))
        }
        Err(e) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(e.to_string()),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::from(e).with_execution(exec))
        }
    }
}

/// Query parameters for dependency graph
#[derive(Debug, Deserialize)]
pub struct DependencyGraphParams {
    /// Maximum depth to traverse (-1 for unlimited)
    pub max_depth: Option<i32>,
}

/// Get reverse dependencies (dependents)
#[instrument(skip(state, collector))]
pub async fn get_dependents(
    State(state): State<AppState>,
    Extension(collector): Extension<SpanCollector>,
    Path(id): Path<String>,
) -> ApiResult<Json<ExecutionEnvelope<Vec<llm_registry_core::Asset>>>> {
    debug!("Getting dependents for asset: {}", id);

    let asset_id = id.parse::<AssetId>().map_err(|e| {
        let err = ApiError::bad_request(format!("Invalid asset ID: {}", e));
        let exec = collector.finalize_failed("Invalid asset ID");
        err.with_execution(exec)
    })?;

    let span_id = collector.begin_agent_span("SearchService");

    let result = state
        .services
        .search()
        .get_reverse_dependencies(&asset_id)
        .await;

    match result {
        Ok(dependents) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "dependents".to_string(),
                    content_type: Some("application/json".to_string()),
                    data: serde_json::json!({ "count": dependents.len() }),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Ok);
            let exec = collector.finalize();
            Ok(ok_with_execution(dependents, exec))
        }
        Err(e) => {
            let _ = collector.attach_artifact(
                span_id,
                SpanArtifact {
                    name: "error".to_string(),
                    content_type: Some("text/plain".to_string()),
                    data: serde_json::Value::String(e.to_string()),
                },
            );
            collector.end_agent_span(span_id, SpanStatus::Failed);
            let exec = collector.finalize();
            Err(ApiError::from(e).with_execution(exec))
        }
    }
}

// ============================================================================
// Health & Metrics Handlers (NOT instrumented with execution spans —
// these are infrastructure endpoints outside the /v1 execution boundary)
// ============================================================================

/// Health check endpoint
#[instrument(skip(state))]
pub async fn health_check(State(state): State<AppState>) -> ApiResult<HealthResponse> {
    debug!("Health check requested");

    // For now, simple health check
    // In production, you'd check database connectivity, etc.
    let mut response = HealthResponse::healthy()
        .with_version(env!("CARGO_PKG_VERSION"));

    // Add database health check
    // Try to perform a simple database operation
    let db_health = match state.services.search().list_all_tags().await {
        Ok(_) => ComponentHealth::healthy(),
        Err(e) => ComponentHealth::unhealthy(format!("Database error: {}", e)),
    };

    response = response
        .with_check("database", db_health)
        .with_check("service", ComponentHealth::healthy())
        .compute_status();

    Ok(response)
}

/// Metrics endpoint (Prometheus format)
///
/// This endpoint exposes Prometheus metrics for monitoring.
/// Metrics are collected throughout the application lifecycle.
#[instrument]
pub async fn metrics() -> ApiResult<String> {
    debug!("Metrics requested");

    // Return basic info - actual metrics are handled by the server binary
    // which has access to the prometheus registry
    let metrics = format!(
        "# HELP llm_registry_info Registry information\n\
         # TYPE llm_registry_info gauge\n\
         llm_registry_info{{version=\"{}\"}} 1\n",
        env!("CARGO_PKG_VERSION")
    );

    Ok(metrics)
}

// ============================================================================
// Version & Info Handlers
// ============================================================================

/// Get API version information
#[instrument]
pub async fn version_info() -> ApiResult<Json<crate::responses::ApiResponse<VersionInfo>>> {
    let info = VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_version: "v1".to_string(),
        build_timestamp: option_env!("BUILD_TIMESTAMP")
            .unwrap_or("unknown")
            .to_string(),
    };

    Ok(Json(crate::responses::ok(info)))
}

/// Version information
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Semantic version
    pub version: String,

    /// API version
    pub api_version: String,

    /// Build timestamp
    pub build_timestamp: String,
}

// ============================================================================
// Execution Ingestion Handler (data-core fanout)
// ============================================================================

/// Payload from data-core execution fanout
#[derive(Debug, Deserialize)]
pub struct ExecutionRecordRequest {
    /// Source system
    pub source: String,

    /// Event type
    pub event_type: String,

    /// Execution identifier
    pub execution_id: String,

    /// ISO-8601 timestamp
    pub timestamp: String,

    /// Lineage/execution data
    pub payload: serde_json::Value,
}

/// Response for accepted execution records
#[derive(Debug, Serialize)]
pub struct ExecutionAcceptedResponse {
    pub status: String,
    pub execution_id: String,
}

/// Accept an execution record from data-core fanout.
///
/// This endpoint lives outside the execution-context middleware because it
/// *receives* execution records rather than participating in the span system.
#[instrument(skip(request), fields(execution_id = %request.execution_id, source = %request.source))]
pub async fn receive_execution(
    Json(request): Json<ExecutionRecordRequest>,
) -> (StatusCode, Json<ExecutionAcceptedResponse>) {
    info!(
        execution_id = %request.execution_id,
        source = %request.source,
        event_type = %request.event_type,
        "Accepted execution record from data-core"
    );

    (
        StatusCode::ACCEPTED,
        Json(ExecutionAcceptedResponse {
            status: "accepted".to_string(),
            execution_id: request.execution_id,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info_creation() {
        let info = VersionInfo {
            version: "0.1.0".to_string(),
            api_version: "v1".to_string(),
            build_timestamp: "2024-01-01".to_string(),
        };

        assert_eq!(info.version, "0.1.0");
        assert_eq!(info.api_version, "v1");
    }
}
