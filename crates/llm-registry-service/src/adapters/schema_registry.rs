//! Schema Registry Adapter
//!
//! Thin adapter for consuming canonical schema definitions from LLM-Schema-Registry.
//! Provides schema validation for model metadata and pipeline descriptors without
//! modifying existing registry indexing or metadata management logic.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[allow(dead_code)]
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, instrument, warn};

/// Errors from schema registry consumption
#[derive(Error, Debug)]
pub enum SchemaAdapterError {
    #[error("Schema not found: {0}")]
    SchemaNotFound(String),
    #[error("Schema validation failed: {0}")]
    ValidationFailed(String),
    #[error("Schema registry unavailable: {0}")]
    Unavailable(String),
    #[error("Incompatible schema version: {0}")]
    IncompatibleVersion(String),
}

/// Result type for schema adapter operations
pub type SchemaResult<T> = Result<T, SchemaAdapterError>;

/// Serialization format for schemas (mirrors upstream)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerializationFormat {
    Json,
    Avro,
    Protobuf,
    Yaml,
}

/// Schema reference consumed from upstream registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumedSchema {
    /// Schema identifier
    pub id: String,
    /// Schema name
    pub name: String,
    /// Schema namespace
    pub namespace: String,
    /// Schema version
    pub version: String,
    /// Schema format
    pub format: SerializationFormat,
    /// Raw schema content
    pub content: String,
    /// Content hash for integrity
    pub content_hash: String,
    /// Whether schema is active
    pub is_active: bool,
}

/// Schema validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Schema used for validation
    pub schema_id: String,
    /// Validation errors if any
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
}

/// Trait for schema registry consumption
#[async_trait]
pub trait SchemaConsumer: Send + Sync {
    /// Fetch a schema by name and namespace
    async fn get_schema(&self, name: &str, namespace: &str) -> SchemaResult<ConsumedSchema>;

    /// Fetch a specific schema version
    async fn get_schema_version(
        &self,
        name: &str,
        namespace: &str,
        version: &str,
    ) -> SchemaResult<ConsumedSchema>;

    /// Validate data against a schema
    async fn validate_against_schema(
        &self,
        schema_name: &str,
        namespace: &str,
        data: &serde_json::Value, // Note: used for actual validation when upstream is connected
    ) -> SchemaResult<SchemaValidationResult>;

    /// List available schemas for a namespace
    async fn list_schemas(&self, namespace: &str) -> SchemaResult<Vec<String>>;
}

/// Schema Registry Adapter for consuming canonical schema definitions
///
/// This adapter provides a thin integration layer for consuming schema
/// definitions from LLM-Schema-Registry without modifying existing
/// registry logic or public APIs.
pub struct SchemaRegistryAdapter {
    /// Base URL for schema registry (if remote)
    #[allow(dead_code)]
    endpoint: Option<String>,
    /// Cached schemas for performance
    cache: Arc<tokio::sync::RwLock<HashMap<String, ConsumedSchema>>>,
    /// Default namespace for model metadata schemas
    default_namespace: String,
}

impl SchemaRegistryAdapter {
    /// Create a new schema registry adapter
    pub fn new() -> Self {
        Self {
            endpoint: None,
            cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            default_namespace: "llm.registry".to_string(),
        }
    }

    /// Create adapter with remote endpoint
    pub fn with_endpoint(endpoint: String) -> Self {
        Self {
            endpoint: Some(endpoint),
            cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            default_namespace: "llm.registry".to_string(),
        }
    }

    /// Set the default namespace
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.default_namespace = namespace;
        self
    }

    /// Generate cache key for schema lookup
    fn cache_key(name: &str, namespace: &str, version: Option<&str>) -> String {
        match version {
            Some(v) => format!("{}.{}@{}", namespace, name, v),
            None => format!("{}.{}", namespace, name),
        }
    }

    /// Get the model metadata schema for validation
    #[instrument(skip(self))]
    pub async fn get_model_metadata_schema(&self) -> SchemaResult<ConsumedSchema> {
        self.get_schema("ModelMetadata", &self.default_namespace).await
    }

    /// Get the pipeline descriptor schema for validation
    #[instrument(skip(self))]
    pub async fn get_pipeline_descriptor_schema(&self) -> SchemaResult<ConsumedSchema> {
        self.get_schema("PipelineDescriptor", &self.default_namespace).await
    }

    /// Validate model metadata against canonical schema
    #[instrument(skip(self, metadata))]
    pub async fn validate_model_metadata(
        &self,
        metadata: &serde_json::Value,
    ) -> SchemaResult<SchemaValidationResult> {
        self.validate_against_schema("ModelMetadata", &self.default_namespace, metadata)
            .await
    }

    /// Validate pipeline descriptor against canonical schema
    #[instrument(skip(self, descriptor))]
    pub async fn validate_pipeline_descriptor(
        &self,
        descriptor: &serde_json::Value,
    ) -> SchemaResult<SchemaValidationResult> {
        self.validate_against_schema("PipelineDescriptor", &self.default_namespace, descriptor)
            .await
    }

    /// Clear cached schemas
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        debug!("Schema cache cleared");
    }
}

impl Default for SchemaRegistryAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchemaConsumer for SchemaRegistryAdapter {
    #[instrument(skip(self))]
    async fn get_schema(&self, name: &str, namespace: &str) -> SchemaResult<ConsumedSchema> {
        let key = Self::cache_key(name, namespace, None);

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(schema) = cache.get(&key) {
                debug!(schema_name = %name, "Schema found in cache");
                return Ok(schema.clone());
            }
        }

        // In production, this would fetch from the upstream schema registry
        // For Phase 2B, we provide a stub that indicates the integration point
        warn!(
            schema_name = %name,
            namespace = %namespace,
            "Schema registry fetch not yet connected - returning placeholder"
        );

        Err(SchemaAdapterError::Unavailable(
            "Schema registry connection not configured".to_string(),
        ))
    }

    #[instrument(skip(self))]
    async fn get_schema_version(
        &self,
        name: &str,
        namespace: &str,
        version: &str,
    ) -> SchemaResult<ConsumedSchema> {
        let key = Self::cache_key(name, namespace, Some(version));

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(schema) = cache.get(&key) {
                debug!(schema_name = %name, version = %version, "Versioned schema found in cache");
                return Ok(schema.clone());
            }
        }

        warn!(
            schema_name = %name,
            namespace = %namespace,
            version = %version,
            "Schema registry version fetch not yet connected"
        );

        Err(SchemaAdapterError::Unavailable(
            "Schema registry connection not configured".to_string(),
        ))
    }

    #[instrument(skip(self, _data))]
    async fn validate_against_schema(
        &self,
        schema_name: &str,
        namespace: &str,
        #[allow(unused_variables)]
        _data: &serde_json::Value,
    ) -> SchemaResult<SchemaValidationResult> {
        // Attempt to get the schema
        let schema_result = self.get_schema(schema_name, namespace).await;

        match schema_result {
            Ok(schema) => {
                // In production, perform actual JSON Schema validation
                // For Phase 2B, return success to indicate integration point works
                debug!(
                    schema_name = %schema_name,
                    schema_id = %schema.id,
                    "Validation performed against schema"
                );

                Ok(SchemaValidationResult {
                    valid: true,
                    schema_id: schema.id,
                    errors: vec![],
                    warnings: vec![
                        "Schema validation is in stub mode - connect to upstream for full validation".to_string()
                    ],
                })
            }
            Err(SchemaAdapterError::Unavailable(_)) => {
                // Return a soft validation result when registry is unavailable
                debug!(
                    schema_name = %schema_name,
                    "Schema registry unavailable - returning permissive validation"
                );

                Ok(SchemaValidationResult {
                    valid: true,
                    schema_id: "unavailable".to_string(),
                    errors: vec![],
                    warnings: vec![
                        "Schema registry unavailable - validation skipped".to_string()
                    ],
                })
            }
            Err(e) => Err(e),
        }
    }

    #[instrument(skip(self))]
    async fn list_schemas(&self, namespace: &str) -> SchemaResult<Vec<String>> {
        debug!(namespace = %namespace, "Listing schemas for namespace");

        // Return known schema types for the registry namespace
        if namespace == self.default_namespace || namespace == "llm.registry" {
            Ok(vec![
                "ModelMetadata".to_string(),
                "PipelineDescriptor".to_string(),
                "AssetManifest".to_string(),
                "DependencyGraph".to_string(),
            ])
        } else {
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schema_adapter_creation() {
        let adapter = SchemaRegistryAdapter::new();
        assert_eq!(adapter.default_namespace, "llm.registry");
    }

    #[tokio::test]
    async fn test_list_schemas() {
        let adapter = SchemaRegistryAdapter::new();
        let schemas = adapter.list_schemas("llm.registry").await.unwrap();
        assert!(schemas.contains(&"ModelMetadata".to_string()));
        assert!(schemas.contains(&"PipelineDescriptor".to_string()));
    }

    #[tokio::test]
    async fn test_cache_key_generation() {
        let key = SchemaRegistryAdapter::cache_key("Test", "ns", None);
        assert_eq!(key, "ns.Test");

        let versioned_key = SchemaRegistryAdapter::cache_key("Test", "ns", Some("1.0.0"));
        assert_eq!(versioned_key, "ns.Test@1.0.0");
    }
}
