//! Config Manager Adapter
//!
//! Thin adapter for consuming configuration-driven registry policies from LLM-Config-Manager.
//! Provides TTLs, retention rules, and validation constraints without modifying existing
//! registry indexing or metadata management logic.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, instrument, warn};

/// Errors from config manager consumption
#[derive(Error, Debug)]
pub enum ConfigAdapterError {
    #[error("Configuration not found: {0}")]
    NotFound(String),
    #[error("Configuration validation failed: {0}")]
    ValidationFailed(String),
    #[error("Config manager unavailable: {0}")]
    Unavailable(String),
    #[error("Invalid configuration format: {0}")]
    InvalidFormat(String),
}

/// Result type for config adapter operations
pub type ConfigResult<T> = Result<T, ConfigAdapterError>;

/// Environment for configuration (mirrors upstream)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
}

/// Registry policy consumed from config manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPolicy {
    /// Policy name
    pub name: String,
    /// Policy namespace
    pub namespace: String,
    /// Whether policy is enabled
    pub enabled: bool,
    /// Policy rules as JSON
    pub rules: serde_json::Value,
    /// Policy priority (higher = more important)
    pub priority: u32,
}

/// TTL configuration for registry assets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlConfig {
    /// Default TTL for new assets
    pub default_ttl: Duration,
    /// TTL for deprecated assets
    pub deprecated_ttl: Duration,
    /// TTL for archived assets
    pub archived_ttl: Duration,
    /// TTL for cache entries
    pub cache_ttl: Duration,
    /// Whether TTL is enforced
    pub enforce: bool,
}

impl Default for TtlConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_secs(365 * 24 * 60 * 60), // 1 year
            deprecated_ttl: Duration::from_secs(90 * 24 * 60 * 60), // 90 days
            archived_ttl: Duration::from_secs(30 * 24 * 60 * 60),  // 30 days
            cache_ttl: Duration::from_secs(3600),                   // 1 hour
            enforce: false,
        }
    }
}

/// Retention rules for registry data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionRules {
    /// Minimum versions to retain per asset
    pub min_versions: u32,
    /// Maximum versions to retain per asset
    pub max_versions: u32,
    /// Retain all versions for this duration
    pub retain_all_for: Duration,
    /// Delete deprecated versions after this duration
    pub delete_deprecated_after: Duration,
    /// Keep at least one active version
    pub keep_one_active: bool,
}

impl Default for RetentionRules {
    fn default() -> Self {
        Self {
            min_versions: 3,
            max_versions: 100,
            retain_all_for: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            delete_deprecated_after: Duration::from_secs(180 * 24 * 60 * 60), // 180 days
            keep_one_active: true,
        }
    }
}

/// Validation constraints for registry operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConstraints {
    /// Maximum asset size in bytes
    pub max_asset_size: u64,
    /// Maximum metadata size in bytes
    pub max_metadata_size: u64,
    /// Maximum number of tags per asset
    pub max_tags: u32,
    /// Maximum number of dependencies per asset
    pub max_dependencies: u32,
    /// Required metadata fields
    pub required_fields: Vec<String>,
    /// Allowed asset types
    pub allowed_asset_types: Vec<String>,
    /// Whether to enforce strict validation
    pub strict_mode: bool,
}

impl Default for ValidationConstraints {
    fn default() -> Self {
        Self {
            max_asset_size: 10 * 1024 * 1024 * 1024, // 10 GB
            max_metadata_size: 1024 * 1024,          // 1 MB
            max_tags: 50,
            max_dependencies: 100,
            required_fields: vec![
                "name".to_string(),
                "version".to_string(),
                "description".to_string(),
            ],
            allowed_asset_types: vec![
                "Model".to_string(),
                "Pipeline".to_string(),
                "TestSuite".to_string(),
                "Policy".to_string(),
                "Dataset".to_string(),
            ],
            strict_mode: false,
        }
    }
}

/// Combined registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Current environment
    pub environment: Environment,
    /// TTL configuration
    pub ttl: TtlConfig,
    /// Retention rules
    pub retention: RetentionRules,
    /// Validation constraints
    pub validation: ValidationConstraints,
    /// Active policies
    pub policies: Vec<RegistryPolicy>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            environment: Environment::Development,
            ttl: TtlConfig::default(),
            retention: RetentionRules::default(),
            validation: ValidationConstraints::default(),
            policies: vec![],
        }
    }
}

/// Trait for config manager consumption
#[async_trait]
pub trait ConfigConsumer: Send + Sync {
    /// Get the current registry configuration
    async fn get_config(&self) -> ConfigResult<RegistryConfig>;

    /// Get TTL configuration
    async fn get_ttl_config(&self) -> ConfigResult<TtlConfig>;

    /// Get retention rules
    async fn get_retention_rules(&self) -> ConfigResult<RetentionRules>;

    /// Get validation constraints
    async fn get_validation_constraints(&self) -> ConfigResult<ValidationConstraints>;

    /// Get active policies
    async fn get_policies(&self) -> ConfigResult<Vec<RegistryPolicy>>;

    /// Refresh configuration from upstream
    async fn refresh(&self) -> ConfigResult<()>;
}

/// Config Manager Adapter for consuming registry policies
///
/// This adapter provides a thin integration layer for consuming
/// configuration from LLM-Config-Manager without modifying existing
/// registry logic or public APIs.
pub struct ConfigManagerAdapter {
    /// Current environment
    environment: Environment,
    /// Cached configuration
    config: Arc<tokio::sync::RwLock<RegistryConfig>>,
    /// Configuration namespace
    namespace: String,
    /// Remote endpoint (if configured)
    endpoint: Option<String>,
    /// Last refresh timestamp
    last_refresh: Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl ConfigManagerAdapter {
    /// Create a new config manager adapter with defaults
    pub fn new(environment: Environment) -> Self {
        Self {
            environment,
            config: Arc::new(tokio::sync::RwLock::new(RegistryConfig {
                environment,
                ..Default::default()
            })),
            namespace: "llm.registry".to_string(),
            endpoint: None,
            last_refresh: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Create adapter with remote endpoint
    pub fn with_endpoint(environment: Environment, endpoint: String) -> Self {
        let mut adapter = Self::new(environment);
        adapter.endpoint = Some(endpoint);
        adapter
    }

    /// Set the configuration namespace
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.namespace = namespace;
        self
    }

    /// Get the current environment
    pub fn environment(&self) -> Environment {
        self.environment
    }

    /// Check if configuration is stale and needs refresh
    #[instrument(skip(self))]
    pub async fn is_stale(&self, max_age: Duration) -> bool {
        let last_refresh = self.last_refresh.read().await;
        match *last_refresh {
            Some(timestamp) => {
                let age = chrono::Utc::now() - timestamp;
                age.num_seconds() > max_age.as_secs() as i64
            }
            None => true,
        }
    }

    /// Apply environment-specific overrides
    #[instrument(skip(self, base_config))]
    async fn apply_environment_overrides(&self, mut base_config: RegistryConfig) -> RegistryConfig {
        match self.environment {
            Environment::Production => {
                // Stricter settings for production
                base_config.validation.strict_mode = true;
                base_config.ttl.enforce = true;
                base_config.retention.keep_one_active = true;
            }
            Environment::Staging => {
                // Moderate settings for staging
                base_config.validation.strict_mode = true;
                base_config.ttl.enforce = false;
            }
            Environment::Development => {
                // Relaxed settings for development
                base_config.validation.strict_mode = false;
                base_config.ttl.enforce = false;
                base_config.validation.max_asset_size = 100 * 1024 * 1024 * 1024; // 100 GB for dev
            }
        }

        debug!(
            environment = ?self.environment,
            strict_mode = base_config.validation.strict_mode,
            "Applied environment overrides"
        );

        base_config
    }
}

impl Default for ConfigManagerAdapter {
    fn default() -> Self {
        Self::new(Environment::Development)
    }
}

#[async_trait]
impl ConfigConsumer for ConfigManagerAdapter {
    #[instrument(skip(self))]
    async fn get_config(&self) -> ConfigResult<RegistryConfig> {
        let config = self.config.read().await;
        Ok(config.clone())
    }

    #[instrument(skip(self))]
    async fn get_ttl_config(&self) -> ConfigResult<TtlConfig> {
        let config = self.config.read().await;
        Ok(config.ttl.clone())
    }

    #[instrument(skip(self))]
    async fn get_retention_rules(&self) -> ConfigResult<RetentionRules> {
        let config = self.config.read().await;
        Ok(config.retention.clone())
    }

    #[instrument(skip(self))]
    async fn get_validation_constraints(&self) -> ConfigResult<ValidationConstraints> {
        let config = self.config.read().await;
        Ok(config.validation.clone())
    }

    #[instrument(skip(self))]
    async fn get_policies(&self) -> ConfigResult<Vec<RegistryPolicy>> {
        let config = self.config.read().await;
        Ok(config.policies.clone())
    }

    #[instrument(skip(self))]
    async fn refresh(&self) -> ConfigResult<()> {
        // In production, this would fetch from the upstream config manager
        // For Phase 2B, we apply environment overrides to defaults

        if self.endpoint.is_some() {
            warn!(
                namespace = %self.namespace,
                "Config manager remote fetch not yet connected - using defaults with overrides"
            );
        }

        let base_config = RegistryConfig {
            environment: self.environment,
            ..Default::default()
        };

        let config = self.apply_environment_overrides(base_config).await;

        {
            let mut cached = self.config.write().await;
            *cached = config;
        }

        {
            let mut last_refresh = self.last_refresh.write().await;
            *last_refresh = Some(chrono::Utc::now());
        }

        debug!(
            environment = ?self.environment,
            namespace = %self.namespace,
            "Configuration refreshed"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_adapter_creation() {
        let adapter = ConfigManagerAdapter::new(Environment::Development);
        assert_eq!(adapter.environment(), Environment::Development);
    }

    #[tokio::test]
    async fn test_get_default_config() {
        let adapter = ConfigManagerAdapter::new(Environment::Production);
        adapter.refresh().await.unwrap();

        let config = adapter.get_config().await.unwrap();
        assert_eq!(config.environment, Environment::Production);
        assert!(config.validation.strict_mode);
    }

    #[tokio::test]
    async fn test_ttl_defaults() {
        let adapter = ConfigManagerAdapter::default();
        adapter.refresh().await.unwrap();

        let ttl = adapter.get_ttl_config().await.unwrap();
        assert_eq!(ttl.default_ttl, Duration::from_secs(365 * 24 * 60 * 60));
    }

    #[tokio::test]
    async fn test_retention_defaults() {
        let adapter = ConfigManagerAdapter::default();
        adapter.refresh().await.unwrap();

        let retention = adapter.get_retention_rules().await.unwrap();
        assert_eq!(retention.min_versions, 3);
        assert!(retention.keep_one_active);
    }

    #[tokio::test]
    async fn test_validation_constraints() {
        let adapter = ConfigManagerAdapter::default();
        adapter.refresh().await.unwrap();

        let constraints = adapter.get_validation_constraints().await.unwrap();
        assert!(constraints.required_fields.contains(&"name".to_string()));
        assert!(constraints.allowed_asset_types.contains(&"Model".to_string()));
    }

    #[tokio::test]
    async fn test_is_stale() {
        let adapter = ConfigManagerAdapter::default();

        // Should be stale before first refresh
        assert!(adapter.is_stale(Duration::from_secs(60)).await);

        adapter.refresh().await.unwrap();

        // Should not be stale immediately after refresh
        assert!(!adapter.is_stale(Duration::from_secs(60)).await);
    }
}
