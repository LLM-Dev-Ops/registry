//! LLM Registry Server
//!
//! Main entry point for the LLM Registry HTTP server.
//! This binary sets up the database, services, and HTTP server with graceful shutdown.

mod config;
mod metrics;
mod telemetry;
mod tracing_setup;

use anyhow::{Context, Result};
use clap::Parser;
use llm_registry_api::build_api_server;
use llm_registry_db::{create_pool, PoolConfig, PostgresAssetRepository, PostgresEventStore};
use llm_registry_service::ServiceRegistry;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{error, info, warn};

use config::ServerConfig;

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration directory
    #[arg(short, long, env = "CONFIG_DIR", default_value = "config")]
    config_dir: String,

    /// Environment (development, production, etc.)
    #[arg(short, long, env = "ENVIRONMENT", default_value = "development")]
    environment: String,

    /// Server host
    #[arg(long, env = "SERVER_HOST")]
    host: Option<String>,

    /// Server port
    #[arg(short, long, env = "SERVER_PORT")]
    port: Option<u16>,

    /// Database URL
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Log level
    #[arg(long, env = "RUST_LOG")]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if it exists
    dotenvy::dotenv().ok();

    // Parse command-line arguments
    let args = Args::parse();

    // Load configuration
    let mut config = ServerConfig::load_or_default(&args.config_dir, &args.environment);

    // Override with command-line arguments
    if let Some(host) = args.host {
        config.server.host = host;
    }
    if let Some(port) = args.port {
        config.server.port = port;
    }
    if let Some(database_url) = args.database_url {
        config.database.url = database_url;
    }
    if let Some(log_level) = args.log_level {
        config.logging.level = log_level;
    }

    // Initialize telemetry
    let telemetry_config = telemetry::TelemetryConfig::new()
        .with_log_level(config.logging.level.clone())
        .with_json_format(config.logging.json_format)
        .with_timestamps(config.logging.include_timestamps)
        .with_thread_ids(config.logging.include_thread_ids)
        .with_target(config.logging.include_target);

    telemetry::init_with_config(telemetry_config);

    info!("Starting LLM Registry Server");
    info!("Environment: {}", args.environment);
    info!("Server: {}", config.bind_address());
    info!("Database: {}", mask_database_url(&config.database.url));

    // Setup database connection pool with retries.
    // The server must bind the HTTP port promptly so Cloud Run startup probes
    // pass, so we retry DB connections rather than crashing on first failure.
    let pool = setup_database_with_retries(&config).await?;

    // Create repositories
    let asset_repository = Arc::new(PostgresAssetRepository::new(pool.clone()));
    let event_store = Arc::new(PostgresEventStore::new(pool.clone()));

    // Create service registry (wrapped in Arc for sharing between servers)
    let services = Arc::new(ServiceRegistry::new(asset_repository, event_store));

    // Build API server
    let app = build_api_server((*services).clone());

    // Parse HTTP bind address
    let http_addr: SocketAddr = config
        .bind_address()
        .parse()
        .context("Invalid HTTP bind address")?;

    info!("HTTP Server listening on http://{}", http_addr);

    // Create HTTP TCP listener
    let http_listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("Failed to bind HTTP server")?;

    // Start gRPC server if enabled
    let grpc_handle = if config.grpc.enabled {
        let grpc_addr: SocketAddr = format!("{}:{}", config.grpc.host, config.grpc.port)
            .parse()
            .context("Invalid gRPC bind address")?;

        info!("gRPC Server listening on grpc://{}", grpc_addr);

        // Build gRPC service
        let grpc_service = llm_registry_api::RegistryServiceImpl::new(Arc::clone(&services));

        // Spawn gRPC server in background
        Some(tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(llm_registry_api::RegistryServiceServer::new(grpc_service))
                .serve(grpc_addr)
                .await
        }))
    } else {
        info!("gRPC Server disabled");
        None
    };

    // Serve HTTP with graceful shutdown
    let http_result = if config.server.graceful_shutdown {
        axum::serve(http_listener, app.into_make_service())
            .with_graceful_shutdown(shutdown_signal(config.server.shutdown_timeout_seconds))
            .await
            .context("HTTP Server error")
    } else {
        axum::serve(http_listener, app.into_make_service())
            .await
            .context("HTTP Server error")
    };

    // Wait for gRPC server if it was started
    if let Some(handle) = grpc_handle {
        tokio::select! {
            res = async { http_result } => {
                res?;
            },
            res = handle => {
                res.context("gRPC task panicked")?
                    .map_err(|e| anyhow::anyhow!("gRPC Server error: {}", e))?;
            },
        }
    } else {
        http_result?;
    }

    info!("Server shutdown complete");
    Ok(())
}

/// Setup database connection pool with exponential backoff retries.
///
/// Cloud Run cold starts may race with Cloud SQL proxy readiness, so we
/// retry a few times before giving up.
async fn setup_database_with_retries(config: &ServerConfig) -> Result<PgPool> {
    const MAX_RETRIES: u32 = 5;
    let mut delay = Duration::from_secs(2);

    for attempt in 1..=MAX_RETRIES {
        info!("Connecting to database (attempt {}/{})", attempt, MAX_RETRIES);

        match setup_database(config).await {
            Ok(pool) => return Ok(pool),
            Err(e) => {
                if attempt == MAX_RETRIES {
                    error!("Database connection failed after {} attempts: {:#}", MAX_RETRIES, e);
                    return Err(e);
                }
                warn!(
                    "Database connection attempt {} failed: {}. Retrying in {:?}...",
                    attempt, e, delay
                );
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
        }
    }
    unreachable!()
}

/// Setup database connection pool
async fn setup_database(config: &ServerConfig) -> Result<PgPool> {
    let pool_config = PoolConfig::new(&config.database.url)
        .min_connections(config.database.min_connections)
        .max_connections(config.database.max_connections)
        .connect_timeout(Duration::from_secs(config.database.connect_timeout_seconds))
        .idle_timeout(Duration::from_secs(config.database.idle_timeout_seconds))
        .max_lifetime(Duration::from_secs(config.database.max_lifetime_seconds))
        .run_migrations(config.database.run_migrations)
        .enable_logging(config.logging.level != "error");

    let pool = create_pool(&pool_config)
        .await
        .context("Failed to create database connection pool")?;

    info!("Database connection established");
    Ok(pool)
}

/// Graceful shutdown signal handler
///
/// Waits for SIGTERM or SIGINT (Ctrl+C) and then initiates graceful shutdown
/// with a timeout.
async fn shutdown_signal(timeout_seconds: u64) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, starting graceful shutdown");
        },
        _ = terminate => {
            info!("Received SIGTERM, starting graceful shutdown");
        },
    }

    // Give the server time to finish processing requests
    info!("Waiting up to {} seconds for graceful shutdown", timeout_seconds);
}

/// Mask sensitive parts of database URL for logging
fn mask_database_url(url: &str) -> String {
    // Simple masking: hide password if present
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            if let Some(scheme_end) = url.find("://") {
                let scheme = &url[..scheme_end + 3];
                let user = &url[scheme_end + 3..colon_pos];
                let rest = &url[at_pos..];
                return format!("{}{}:***{}", scheme, user, rest);
            }
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_database_url() {
        let url = "postgresql://user:password@localhost:5432/dbname";
        let masked = mask_database_url(url);
        assert_eq!(masked, "postgresql://user:***@localhost:5432/dbname");
    }

    #[test]
    fn test_mask_database_url_no_password() {
        let url = "postgresql://localhost:5432/dbname";
        let masked = mask_database_url(url);
        assert_eq!(masked, "postgresql://localhost:5432/dbname");
    }
}
