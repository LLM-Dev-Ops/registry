//! API route definitions
//!
//! This module defines all API routes and builds the router.

use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Router,
};

use crate::{
    auth::{optional_auth, require_auth, AuthState},
    auth_handlers::{generate_api_key, login, logout, me, refresh_token, AuthHandlerState},
    execution_middleware::require_execution_context,
    graphql::{build_schema, graphql_handler, graphql_playground},
    handlers::{
        delete_asset, get_asset, get_dependencies, get_dependents, health_check, list_assets,
        metrics, register_asset, update_asset, version_info, AppState,
    },
};

/// Build the API router with all routes
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Health and info endpoints
        .route("/health", get(health_check))
        .route("/metrics", get(metrics))
        .route("/version", get(version_info))
        // API v1 routes
        .nest("/v1", build_v1_routes())
        .with_state(state)
}

/// Build the API router with authentication enabled
///
/// This function builds a complete router with authentication endpoints
/// and middleware. Protected routes require JWT authentication.
pub fn build_router_with_auth(
    state: AppState,
    auth_handler_state: AuthHandlerState,
    auth_state: AuthState,
) -> Router {
    // Build public routes
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics))
        .route("/version", get(version_info))
        .with_state(state.clone());

    // Build auth routes (public)
    let auth_routes = Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh_token))
        .with_state(auth_handler_state.clone());

    // Build protected auth routes
    let protected_auth_routes = Router::new()
        .route("/me", get(me))
        .route("/logout", post(logout))
        .route("/api-keys", post(generate_api_key))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_auth,
        ))
        .with_state(auth_handler_state);

    // Build v1 routes (with optional authentication on some endpoints)
    let v1_routes = build_v1_routes().with_state(state);

    // Combine all routes
    Router::new()
        .merge(public_routes)
        .nest("/v1/auth", auth_routes)
        .nest("/v1/auth", protected_auth_routes)
        .nest("/v1", v1_routes)
}

/// Build the API router with GraphQL support
///
/// This function builds a complete router with REST API, GraphQL API,
/// authentication, and GraphQL Playground.
pub fn build_router_with_graphql(
    state: AppState,
    auth_handler_state: AuthHandlerState,
    auth_state: AuthState,
) -> Router {
    // Build GraphQL schema
    let schema = build_schema(state.services.clone());

    // Build public routes
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics))
        .route("/version", get(version_info))
        .route("/graphql/playground", get(graphql_playground))
        .with_state(state.clone());

    // Build GraphQL route with optional authentication
    let graphql_route = Router::new()
        .route("/graphql", post(graphql_handler))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            optional_auth,
        ))
        .with_state(schema);

    // Build auth routes (public)
    let auth_routes = Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh_token))
        .with_state(auth_handler_state.clone());

    // Build protected auth routes
    let protected_auth_routes = Router::new()
        .route("/me", get(me))
        .route("/logout", post(logout))
        .route("/api-keys", post(generate_api_key))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_auth,
        ))
        .with_state(auth_handler_state);

    // Build v1 routes
    let v1_routes = build_v1_routes().with_state(state);

    // Combine all routes
    Router::new()
        .merge(public_routes)
        .merge(graphql_route)
        .nest("/v1/auth", auth_routes)
        .nest("/v1/auth", protected_auth_routes)
        .nest("/v1", v1_routes)
}

/// Build v1 API routes
///
/// All v1 routes require an execution context (X-Execution-Id and
/// X-Parent-Span-Id headers) enforced by the execution middleware.
fn build_v1_routes() -> Router<AppState> {
    Router::new()
        // Asset management
        .route("/assets", post(register_asset))
        .route("/assets", get(list_assets))
        .route("/assets/{id}", get(get_asset))
        .route("/assets/{id}", patch(update_asset))
        .route("/assets/{id}", delete(delete_asset))
        // Dependencies
        .route("/assets/{id}/dependencies", get(get_dependencies))
        .route("/assets/{id}/dependents", get(get_dependents))
        // Execution context middleware — rejects requests without valid
        // X-Execution-Id and X-Parent-Span-Id headers.
        .layer(middleware::from_fn(require_execution_context))
}

/// Route configuration
#[derive(Debug, Clone)]
pub struct RouteConfig {
    /// API base path
    pub base_path: String,

    /// API version
    pub version: String,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            base_path: "/".to_string(),
            version: "v1".to_string(),
        }
    }
}

impl RouteConfig {
    /// Create a new route config
    pub fn new() -> Self {
        Self::default()
    }

    /// Set base path
    pub fn with_base_path(mut self, path: impl Into<String>) -> Self {
        self.base_path = path.into();
        self
    }

    /// Set API version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_config_default() {
        let config = RouteConfig::default();
        assert_eq!(config.base_path, "/");
        assert_eq!(config.version, "v1");
    }

    #[test]
    fn test_route_config_builder() {
        let config = RouteConfig::new()
            .with_base_path("/api")
            .with_version("v2");

        assert_eq!(config.base_path, "/api");
        assert_eq!(config.version, "v2");
    }
}
