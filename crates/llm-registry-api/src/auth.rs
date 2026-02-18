//! Authentication middleware
//!
//! This module provides JWT-based authentication middleware for protecting API routes.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::{
    error::ErrorResponse,
    jwt::{Claims, JwtManager, TokenError},
};

/// Extension for storing authenticated user claims in requests
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// JWT claims for the authenticated user
    pub claims: Claims,
}

impl AuthUser {
    /// Create new authenticated user
    pub fn new(claims: Claims) -> Self {
        Self { claims }
    }

    /// Get user ID
    pub fn user_id(&self) -> &str {
        &self.claims.sub
    }

    /// Get user email
    pub fn email(&self) -> Option<&str> {
        self.claims.email.as_deref()
    }

    /// Check if user has a role
    pub fn has_role(&self, role: &str) -> bool {
        self.claims.has_role(role)
    }

    /// Check if user has any of the roles
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        self.claims.has_any_role(roles)
    }

    /// Check if user has all of the roles
    pub fn has_all_roles(&self, roles: &[&str]) -> bool {
        self.claims.has_all_roles(roles)
    }
}

/// Authentication state containing JWT manager
#[derive(Clone)]
pub struct AuthState {
    jwt_manager: Arc<JwtManager>,
}

impl AuthState {
    /// Create new auth state
    pub fn new(jwt_manager: JwtManager) -> Self {
        Self {
            jwt_manager: Arc::new(jwt_manager),
        }
    }

    /// Get JWT manager reference
    pub fn jwt_manager(&self) -> &JwtManager {
        &self.jwt_manager
    }
}

/// Required authentication middleware
///
/// This middleware requires a valid JWT token in the Authorization header.
/// If authentication fails, it returns a 401 Unauthorized response.
///
/// # Usage
///
/// ```rust,no_run
/// use axum::{Router, routing::get, middleware};
/// use llm_registry_api::auth::{require_auth, AuthState};
/// use llm_registry_api::jwt::{JwtConfig, JwtManager};
///
/// # async fn example() {
/// let jwt_manager = JwtManager::new(JwtConfig::default()).unwrap();
/// let auth_state = AuthState::new(jwt_manager);
///
/// let app = Router::new()
///     .route("/protected", get(|| async { "Protected content" }))
///     .layer(middleware::from_fn_with_state(auth_state.clone(), require_auth));
/// # }
/// ```
pub async fn require_auth(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    debug!("Authenticating request");

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    // Extract token from header
    let token = JwtManager::extract_token_from_header(auth_header)
        .map_err(|_| AuthError::InvalidToken)?;

    // Validate token
    let claims = auth_state
        .jwt_manager
        .validate_token(token)
        .map_err(|e| match e {
            TokenError::Expired => AuthError::ExpiredToken,
            TokenError::NotYetValid => AuthError::InvalidToken,
            _ => AuthError::InvalidToken,
        })?;

    debug!("User authenticated: {}", claims.sub);

    // Add user to request extensions
    request.extensions_mut().insert(AuthUser::new(claims));

    Ok(next.run(request).await)
}

/// Optional authentication middleware
///
/// This middleware attempts to authenticate the user but does not fail if
/// authentication is unsuccessful. Use this for endpoints that have optional
/// authentication (e.g., public content that can be personalized for logged-in users).
///
/// # Usage
///
/// ```rust,no_run
/// use axum::{Router, routing::get, middleware};
/// use llm_registry_api::auth::{optional_auth, AuthState};
/// use llm_registry_api::jwt::{JwtConfig, JwtManager};
///
/// # async fn example() {
/// let jwt_manager = JwtManager::new(JwtConfig::default()).unwrap();
/// let auth_state = AuthState::new(jwt_manager);
///
/// let app = Router::new()
///     .route("/public", get(|| async { "Public content" }))
///     .layer(middleware::from_fn_with_state(auth_state.clone(), optional_auth));
/// # }
/// ```
pub async fn optional_auth(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Response {
    debug!("Attempting optional authentication");

    // Try to extract and validate token
    if let Some(auth_header) = request.headers().get(AUTHORIZATION) {
        if let Ok(header_str) = auth_header.to_str() {
            if let Ok(token) = JwtManager::extract_token_from_header(header_str) {
                if let Ok(claims) = auth_state.jwt_manager.validate_token(token) {
                    debug!("User optionally authenticated: {}", claims.sub);
                    request.extensions_mut().insert(AuthUser::new(claims));
                }
            }
        }
    }

    next.run(request).await
}

/// Role-based authentication middleware
///
/// This middleware requires authentication AND checks if the user has one of the
/// specified roles.
///
/// # Usage
///
/// ```rust,no_run
/// use axum::{Router, routing::get, middleware};
/// use llm_registry_api::auth::{require_role, AuthState};
/// use llm_registry_api::jwt::{JwtConfig, JwtManager};
///
/// # async fn example() {
/// let jwt_manager = JwtManager::new(JwtConfig::default()).unwrap();
/// let auth_state = AuthState::new(jwt_manager);
///
/// let roles = vec!["admin".to_string(), "moderator".to_string()];
///
/// let app = Router::new()
///     .route("/admin", get(|| async { "Admin content" }))
///     .layer(middleware::from_fn_with_state(
///         (auth_state.clone(), roles),
///         require_role,
///     ));
/// # }
/// ```
pub async fn require_role(
    State((auth_state, allowed_roles)): State<(AuthState, Vec<String>)>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    debug!("Authenticating request with role check");

    // First authenticate
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    let token = JwtManager::extract_token_from_header(auth_header)
        .map_err(|_| AuthError::InvalidToken)?;

    let claims = auth_state
        .jwt_manager
        .validate_token(token)
        .map_err(|e| match e {
            TokenError::Expired => AuthError::ExpiredToken,
            TokenError::NotYetValid => AuthError::InvalidToken,
            _ => AuthError::InvalidToken,
        })?;

    // Check roles
    let role_refs: Vec<&str> = allowed_roles.iter().map(|s| s.as_str()).collect();
    if !claims.has_any_role(&role_refs) {
        warn!("User {} lacks required role", claims.sub);
        return Err(AuthError::InsufficientPermissions);
    }

    debug!("User authenticated with role: {}", claims.sub);
    request.extensions_mut().insert(AuthUser::new(claims));

    Ok(next.run(request).await)
}

/// Extract authenticated user from request
///
/// This is a helper function to extract the AuthUser from request extensions.
/// Use this in your handlers after authentication middleware.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{extract::Extension};
/// use llm_registry_api::auth::AuthUser;
///
/// async fn my_handler(Extension(user): Extension<AuthUser>) -> String {
///     format!("Hello, user {}", user.user_id())
/// }
/// ```
pub fn extract_user(request: &Request<Body>) -> Result<&AuthUser, AuthError> {
    request
        .extensions()
        .get::<AuthUser>()
        .ok_or(AuthError::Unauthenticated)
}

/// Authentication errors
#[derive(Debug)]
pub enum AuthError {
    /// Missing authentication token
    MissingToken,

    /// Invalid token format or signature
    InvalidToken,

    /// Token has expired
    ExpiredToken,

    /// User is not authenticated
    Unauthenticated,

    /// User lacks required permissions
    InsufficientPermissions,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingToken => (
                StatusCode::UNAUTHORIZED,
                "Missing authentication token",
            ),
            AuthError::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "Invalid authentication token",
            ),
            AuthError::ExpiredToken => (
                StatusCode::UNAUTHORIZED,
                "Authentication token has expired",
            ),
            AuthError::Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                "Authentication required",
            ),
            AuthError::InsufficientPermissions => (
                StatusCode::FORBIDDEN,
                "Insufficient permissions",
            ),
        };

        let error_response = ErrorResponse {
            status: status.as_u16(),
            error: message.to_string(),
            code: None,
            timestamp: chrono::Utc::now(),
            execution: None,
        };

        (status, axum::Json(error_response)).into_response()
    }
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "Missing authentication token"),
            AuthError::InvalidToken => write!(f, "Invalid authentication token"),
            AuthError::ExpiredToken => write!(f, "Authentication token has expired"),
            AuthError::Unauthenticated => write!(f, "Authentication required"),
            AuthError::InsufficientPermissions => write!(f, "Insufficient permissions"),
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jwt::{JwtConfig, JwtManager};
    use axum::{
        body::Body,
        extract::Extension,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    fn create_test_jwt_manager() -> JwtManager {
        let config = JwtConfig::new("test-secret-key")
            .with_issuer("test")
            .with_audience("test");
        JwtManager::new(config).unwrap()
    }

    async fn protected_handler(Extension(user): axum::extract::Extension<AuthUser>) -> String {
        format!("Hello, {}", user.user_id())
    }

    #[tokio::test]
    async fn test_require_auth_with_valid_token() {
        let jwt_manager = create_test_jwt_manager();
        let token = jwt_manager.generate_token("user123").unwrap();
        let auth_state = AuthState::new(jwt_manager);

        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                require_auth,
            ));

        let request = Request::builder()
            .uri("/protected")
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_auth_without_token() {
        let jwt_manager = create_test_jwt_manager();
        let auth_state = AuthState::new(jwt_manager);

        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                require_auth,
            ));

        let request = Request::builder()
            .uri("/protected")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_require_auth_with_invalid_token() {
        let jwt_manager = create_test_jwt_manager();
        let auth_state = AuthState::new(jwt_manager);

        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                require_auth,
            ));

        let request = Request::builder()
            .uri("/protected")
            .header(AUTHORIZATION, "Bearer invalid.token.here")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_optional_auth_with_token() {
        let jwt_manager = create_test_jwt_manager();
        let token = jwt_manager.generate_token("user123").unwrap();
        let auth_state = AuthState::new(jwt_manager);

        async fn handler(user: Option<axum::extract::Extension<AuthUser>>) -> String {
            match user {
                Some(Extension(u)) => format!("Hello, {}", u.user_id()),
                None => "Hello, guest".to_string(),
            }
        }

        let app = Router::new()
            .route("/public", get(handler))
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                optional_auth,
            ));

        let request = Request::builder()
            .uri("/public")
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_optional_auth_without_token() {
        let jwt_manager = create_test_jwt_manager();
        let auth_state = AuthState::new(jwt_manager);

        async fn handler() -> &'static str {
            "Public content"
        }

        let app = Router::new()
            .route("/public", get(handler))
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                optional_auth,
            ));

        let request = Request::builder()
            .uri("/public")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_auth_user() {
        let claims = crate::jwt::Claims::new("user123", "test", "test", 3600)
            .with_email("user@example.com")
            .with_role("admin");

        let auth_user = AuthUser::new(claims);

        assert_eq!(auth_user.user_id(), "user123");
        assert_eq!(auth_user.email(), Some("user@example.com"));
        assert!(auth_user.has_role("admin"));
        assert!(!auth_user.has_role("moderator"));
    }
}
