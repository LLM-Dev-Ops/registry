//! LLM Registry API Layer
//!
//! This crate provides the REST API layer for the LLM Registry using Axum.
//! It includes request handlers, middleware, error handling, and response types.
//!
//! # Architecture
//!
//! The API layer is organized into:
//!
//! - **Handlers**: Request handlers for all API endpoints
//! - **Routes**: Route definitions and router configuration
//! - **Middleware**: Tower middleware for logging, CORS, compression, etc.
//! - **Error Handling**: Conversion of service errors to HTTP responses
//! - **Responses**: Standard response wrappers and types
//!
//! # Example
//!
//! ```rust,no_run
//! use llm_registry_api::{build_router, AppState};
//! use llm_registry_service::ServiceRegistry;
//! use std::sync::Arc;
//!
//! # async fn example(
//! #     services: ServiceRegistry,
//! # ) {
//! // Create application state
//! let state = AppState::new(services);
//!
//! // Build router
//! let app = build_router(state);
//!
//! // Run server (example)
//! // axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
//! //     .serve(app.into_make_service())
//! //     .await
//! //     .unwrap();
//! # }
//! ```

pub mod auth;
pub mod auth_handlers;
pub mod error;
pub mod execution_middleware;
pub mod graphql;
pub mod grpc;
pub mod handlers;
pub mod jwt;
pub mod metrics_middleware;
pub mod middleware;
pub mod rate_limit;
pub mod rbac;
pub mod responses;
pub mod routes;

// Re-export main types for convenience
pub use auth::{AuthState, AuthUser, optional_auth, require_auth, require_role};
pub use auth_handlers::{AuthHandlerState, LoginRequest, LoginResponse, RefreshTokenRequest};
pub use error::{ApiError, ApiResult, ErrorResponse};
pub use graphql::{
    build_schema, graphql_handler, graphql_playground, AppSchema, Mutation as GraphQLMutation,
    Query as GraphQLQuery,
};
pub use grpc::{build_grpc_server, serve_grpc, RegistryServiceImpl, RegistryServiceServer};
pub use handlers::{AppState, ExecutionAcceptedResponse, ExecutionRecordRequest, VersionInfo};
pub use jwt::{Claims, JwtConfig, JwtManager, TokenPair};
pub use middleware::{CorsConfig, MiddlewareConfig, UuidRequestIdGenerator};
pub use rate_limit::{rate_limit, RateLimitConfig, RateLimiterState};
pub use execution_middleware::require_execution_context;
pub use rbac::{Permission, RbacPolicy, Role};
pub use responses::{
    created, created_with_execution, deleted, deleted_with_execution, no_content, ok,
    ok_with_execution, ApiResponse, ComponentHealth, EmptyResponse, ExecutionEnvelope,
    HealthResponse, HealthStatus, PaginatedExecutionEnvelope, PaginatedResponse, ResponseMeta,
};
pub use routes::{build_router, build_router_with_auth, build_router_with_graphql, RouteConfig};

use axum::Router;
use llm_registry_service::ServiceRegistry;

/// Build a complete API server with middleware
///
/// This is a convenience function that builds a router with all middleware
/// configured using default settings.
///
/// # Arguments
///
/// * `services` - The service registry to use
///
/// # Example
///
/// ```rust,no_run
/// use llm_registry_api::build_api_server;
/// use llm_registry_service::ServiceRegistry;
/// use std::sync::Arc;
///
/// # async fn example(services: ServiceRegistry) {
/// let app = build_api_server(services);
/// # }
/// ```
pub fn build_api_server(services: ServiceRegistry) -> Router {
    let state = AppState::new(services);
    let router = build_router(state);

    // Apply middleware layers
    router
        .layer(middleware::cors_layer())
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(middleware::trace_layer())
        .layer(tower_http::request_id::SetRequestIdLayer::x_request_id(
            middleware::UuidRequestIdGenerator::default(),
        ))
        .layer(tower_http::request_id::PropagateRequestIdLayer::x_request_id())
}

/// Build API server with custom middleware configuration
///
/// # Arguments
///
/// * `services` - The service registry to use
/// * `middleware_config` - Custom middleware configuration
///
/// # Example
///
/// ```rust,no_run
/// use llm_registry_api::{build_api_server_with_config, MiddlewareConfig, CorsConfig};
/// use llm_registry_service::ServiceRegistry;
///
/// # async fn example(services: ServiceRegistry) {
/// let middleware_config = MiddlewareConfig::new()
///     .with_compression(true)
///     .with_timeout(60);
///
/// let app = build_api_server_with_config(services, middleware_config);
/// # }
/// ```
pub fn build_api_server_with_config(
    services: ServiceRegistry,
    middleware_config: MiddlewareConfig,
) -> Router {
    let state = AppState::new(services);
    let mut router = build_router(state);

    // Apply CORS if configured
    router = router.layer(middleware_config.cors.into_layer());

    // Apply compression if enabled
    if middleware_config.enable_compression {
        router = router.layer(tower_http::compression::CompressionLayer::new());
    }

    // Apply tracing if enabled
    if middleware_config.enable_tracing {
        router = router.layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(
                    tower_http::trace::DefaultMakeSpan::new()
                        .include_headers(true)
                        .level(tracing::Level::INFO),
                )
                .on_response(
                    tower_http::trace::DefaultOnResponse::new()
                        .include_headers(true)
                        .latency_unit(tower_http::LatencyUnit::Millis)
                        .level(tracing::Level::INFO),
                ),
        );
    }

    // Apply request ID generation
    router = router
        .layer(tower_http::request_id::SetRequestIdLayer::x_request_id(
            middleware::UuidRequestIdGenerator::default(),
        ))
        .layer(tower_http::request_id::PropagateRequestIdLayer::x_request_id());

    router
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_compiles() {
        // This test just verifies the library compiles
        // Actual functionality tests would require mock services
    }
}
