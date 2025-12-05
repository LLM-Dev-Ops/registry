//! Observatory Adapter
//!
//! Thin adapter for consuming and emitting telemetry signals from/to LLM-Observatory.
//! Provides governance events, registry health traces, and telemetry integration
//! without modifying existing registry indexing or metadata management logic.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

/// Errors from observatory operations
#[derive(Error, Debug)]
pub enum ObservatoryError {
    #[error("Failed to emit telemetry: {0}")]
    EmitFailed(String),
    #[error("Observatory unavailable: {0}")]
    Unavailable(String),
    #[error("Invalid span data: {0}")]
    InvalidSpan(String),
    #[error("Trace not found: {0}")]
    TraceNotFound(String),
}

/// Result type for observatory operations
pub type ObservatoryResult<T> = Result<T, ObservatoryError>;

/// Span status (mirrors upstream)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpanStatus {
    #[default]
    Unset,
    Ok,
    Error,
}

/// Governance event types for registry operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GovernanceEvent {
    /// Asset was registered
    AssetRegistered {
        asset_id: String,
        asset_name: String,
        asset_version: String,
        registered_by: String,
    },
    /// Asset was updated
    AssetUpdated {
        asset_id: String,
        changes: Vec<String>,
        updated_by: String,
    },
    /// Asset was deprecated
    AssetDeprecated {
        asset_id: String,
        reason: String,
        deprecated_by: String,
    },
    /// Asset was deleted
    AssetDeleted {
        asset_id: String,
        deleted_by: String,
    },
    /// Policy was validated
    PolicyValidated {
        asset_id: String,
        policy_name: String,
        passed: bool,
        violations: Vec<String>,
    },
    /// Integrity was verified
    IntegrityVerified {
        asset_id: String,
        algorithm: String,
        valid: bool,
    },
    /// Access was granted/denied
    AccessDecision {
        principal: String,
        resource: String,
        action: String,
        allowed: bool,
    },
}

/// Registry health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health state
    pub healthy: bool,
    /// Component health states
    pub components: HashMap<String, ComponentHealth>,
    /// Timestamp of health check
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Health status for individual component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Whether component is healthy
    pub healthy: bool,
    /// Latency in milliseconds
    pub latency_ms: u64,
    /// Error message if unhealthy
    pub error: Option<String>,
}

/// Telemetry span for registry operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySpan {
    /// Unique span ID
    pub span_id: String,
    /// Parent trace ID
    pub trace_id: String,
    /// Parent span ID (if nested)
    pub parent_span_id: Option<String>,
    /// Operation name
    pub name: String,
    /// Span status
    pub status: SpanStatus,
    /// Start timestamp
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// End timestamp
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Span attributes
    pub attributes: HashMap<String, serde_json::Value>,
    /// Span events
    pub events: Vec<SpanEvent>,
}

/// Event within a span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Event attributes
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Metrics for registry operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryMetrics {
    /// Total assets registered
    pub total_assets: u64,
    /// Assets registered in last hour
    pub assets_registered_hour: u64,
    /// Average registration latency (ms)
    pub avg_registration_latency_ms: u64,
    /// Search queries in last hour
    pub search_queries_hour: u64,
    /// Average search latency (ms)
    pub avg_search_latency_ms: u64,
    /// Validation pass rate (0.0 - 1.0)
    pub validation_pass_rate: f64,
    /// Cache hit rate (0.0 - 1.0)
    pub cache_hit_rate: f64,
}

/// Trait for observatory telemetry operations
#[async_trait]
pub trait TelemetryEmitter: Send + Sync {
    /// Start a new trace span
    async fn start_span(&self, name: &str, attributes: HashMap<String, serde_json::Value>) -> ObservatoryResult<RegistrySpan>;

    /// End a span with status
    async fn end_span(&self, span: &mut RegistrySpan, status: SpanStatus) -> ObservatoryResult<()>;

    /// Emit a governance event
    async fn emit_governance_event(&self, event: GovernanceEvent) -> ObservatoryResult<()>;

    /// Record a health check
    async fn record_health(&self, status: HealthStatus) -> ObservatoryResult<()>;

    /// Record metrics
    async fn record_metrics(&self, metrics: RegistryMetrics) -> ObservatoryResult<()>;
}

/// Observatory Adapter for telemetry and governance events
///
/// This adapter provides a thin integration layer for emitting
/// telemetry to LLM-Observatory without modifying existing
/// registry logic or public APIs.
pub struct ObservatoryAdapter {
    /// Service name for spans
    #[allow(dead_code)]
    service_name: String,
    /// Remote endpoint (if configured)
    endpoint: Option<String>,
    /// Buffer for batching events
    event_buffer: Arc<tokio::sync::RwLock<Vec<GovernanceEvent>>>,
    /// Buffer flush interval
    flush_interval: Duration,
    /// Whether telemetry is enabled
    enabled: bool,
}

impl ObservatoryAdapter {
    /// Create a new observatory adapter
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
            endpoint: None,
            event_buffer: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            flush_interval: Duration::from_secs(10),
            enabled: true,
        }
    }

    /// Create adapter with remote endpoint
    pub fn with_endpoint(service_name: &str, endpoint: String) -> Self {
        let mut adapter = Self::new(service_name);
        adapter.endpoint = Some(endpoint);
        adapter
    }

    /// Set the flush interval
    pub fn with_flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    /// Enable or disable telemetry
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Check if telemetry is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Generate a new span ID
    fn generate_span_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{:016x}", timestamp)
    }

    /// Generate a new trace ID
    fn generate_trace_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{:032x}", timestamp)
    }

    /// Emit a trace for asset registration
    #[instrument(skip(self))]
    pub async fn trace_asset_registration(
        &self,
        asset_id: &str,
        asset_name: &str,
        asset_version: &str,
        registered_by: &str,
    ) -> ObservatoryResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = GovernanceEvent::AssetRegistered {
            asset_id: asset_id.to_string(),
            asset_name: asset_name.to_string(),
            asset_version: asset_version.to_string(),
            registered_by: registered_by.to_string(),
        };

        self.emit_governance_event(event).await
    }

    /// Emit a trace for asset update
    #[instrument(skip(self, changes))]
    pub async fn trace_asset_update(
        &self,
        asset_id: &str,
        changes: Vec<String>,
        updated_by: &str,
    ) -> ObservatoryResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = GovernanceEvent::AssetUpdated {
            asset_id: asset_id.to_string(),
            changes,
            updated_by: updated_by.to_string(),
        };

        self.emit_governance_event(event).await
    }

    /// Emit a trace for policy validation
    #[instrument(skip(self, violations))]
    pub async fn trace_policy_validation(
        &self,
        asset_id: &str,
        policy_name: &str,
        passed: bool,
        violations: Vec<String>,
    ) -> ObservatoryResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = GovernanceEvent::PolicyValidated {
            asset_id: asset_id.to_string(),
            policy_name: policy_name.to_string(),
            passed,
            violations,
        };

        self.emit_governance_event(event).await
    }

    /// Emit a trace for integrity verification
    #[instrument(skip(self))]
    pub async fn trace_integrity_verification(
        &self,
        asset_id: &str,
        algorithm: &str,
        valid: bool,
    ) -> ObservatoryResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let event = GovernanceEvent::IntegrityVerified {
            asset_id: asset_id.to_string(),
            algorithm: algorithm.to_string(),
            valid,
        };

        self.emit_governance_event(event).await
    }

    /// Get pending events count
    pub async fn pending_events(&self) -> usize {
        let buffer = self.event_buffer.read().await;
        buffer.len()
    }

    /// Flush pending events
    #[instrument(skip(self))]
    pub async fn flush(&self) -> ObservatoryResult<()> {
        let events: Vec<GovernanceEvent> = {
            let mut buffer = self.event_buffer.write().await;
            std::mem::take(&mut *buffer)
        };

        if events.is_empty() {
            return Ok(());
        }

        if self.endpoint.is_some() {
            // In production, batch send to observatory
            warn!(
                event_count = events.len(),
                "Observatory remote flush not yet connected - events logged locally"
            );
        }

        for event in &events {
            info!(event = ?event, "Governance event emitted");
        }

        debug!(event_count = events.len(), "Flushed governance events");

        Ok(())
    }

    /// Create a health status for registry components
    pub fn create_health_status(
        database_healthy: bool,
        database_latency_ms: u64,
        cache_healthy: bool,
        cache_latency_ms: u64,
        search_healthy: bool,
        search_latency_ms: u64,
    ) -> HealthStatus {
        let mut components = HashMap::new();

        components.insert(
            "database".to_string(),
            ComponentHealth {
                name: "database".to_string(),
                healthy: database_healthy,
                latency_ms: database_latency_ms,
                error: if database_healthy { None } else { Some("Database connection failed".to_string()) },
            },
        );

        components.insert(
            "cache".to_string(),
            ComponentHealth {
                name: "cache".to_string(),
                healthy: cache_healthy,
                latency_ms: cache_latency_ms,
                error: if cache_healthy { None } else { Some("Cache connection failed".to_string()) },
            },
        );

        components.insert(
            "search".to_string(),
            ComponentHealth {
                name: "search".to_string(),
                healthy: search_healthy,
                latency_ms: search_latency_ms,
                error: if search_healthy { None } else { Some("Search service unavailable".to_string()) },
            },
        );

        let healthy = database_healthy && cache_healthy && search_healthy;

        HealthStatus {
            healthy,
            components,
            timestamp: chrono::Utc::now(),
        }
    }
}

impl Default for ObservatoryAdapter {
    fn default() -> Self {
        Self::new("llm-registry")
    }
}

#[async_trait]
impl TelemetryEmitter for ObservatoryAdapter {
    #[instrument(skip(self, attributes))]
    async fn start_span(
        &self,
        name: &str,
        attributes: HashMap<String, serde_json::Value>,
    ) -> ObservatoryResult<RegistrySpan> {
        let span = RegistrySpan {
            span_id: Self::generate_span_id(),
            trace_id: Self::generate_trace_id(),
            parent_span_id: None,
            name: name.to_string(),
            status: SpanStatus::Unset,
            start_time: chrono::Utc::now(),
            end_time: None,
            duration_ms: None,
            attributes,
            events: vec![],
        };

        debug!(
            span_id = %span.span_id,
            trace_id = %span.trace_id,
            name = %name,
            "Started registry span"
        );

        Ok(span)
    }

    #[instrument(skip(self, span))]
    async fn end_span(&self, span: &mut RegistrySpan, status: SpanStatus) -> ObservatoryResult<()> {
        let end_time = chrono::Utc::now();
        let duration = end_time - span.start_time;

        span.end_time = Some(end_time);
        span.duration_ms = Some(duration.num_milliseconds() as u64);
        span.status = status;

        debug!(
            span_id = %span.span_id,
            duration_ms = span.duration_ms,
            status = ?status,
            "Ended registry span"
        );

        Ok(())
    }

    #[instrument(skip(self, event))]
    async fn emit_governance_event(&self, event: GovernanceEvent) -> ObservatoryResult<()> {
        if !self.enabled {
            return Ok(());
        }

        // Buffer the event
        {
            let mut buffer = self.event_buffer.write().await;
            buffer.push(event.clone());
        }

        debug!(event = ?event, "Buffered governance event");

        // Auto-flush if buffer is large
        if self.pending_events().await >= 100 {
            self.flush().await?;
        }

        Ok(())
    }

    #[instrument(skip(self, status))]
    async fn record_health(&self, status: HealthStatus) -> ObservatoryResult<()> {
        if !self.enabled {
            return Ok(());
        }

        info!(
            healthy = status.healthy,
            component_count = status.components.len(),
            "Recorded health status"
        );

        // In production, this would emit to observatory
        if self.endpoint.is_some() {
            warn!("Observatory health recording not yet connected");
        }

        Ok(())
    }

    #[instrument(skip(self, metrics))]
    async fn record_metrics(&self, metrics: RegistryMetrics) -> ObservatoryResult<()> {
        if !self.enabled {
            return Ok(());
        }

        info!(
            total_assets = metrics.total_assets,
            validation_pass_rate = metrics.validation_pass_rate,
            cache_hit_rate = metrics.cache_hit_rate,
            "Recorded registry metrics"
        );

        // In production, this would emit to observatory
        if self.endpoint.is_some() {
            warn!("Observatory metrics recording not yet connected");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_observatory_adapter_creation() {
        let adapter = ObservatoryAdapter::new("test-service");
        assert!(adapter.is_enabled());
        assert_eq!(adapter.pending_events().await, 0);
    }

    #[tokio::test]
    async fn test_start_and_end_span() {
        let adapter = ObservatoryAdapter::default();

        let mut span = adapter
            .start_span("test_operation", HashMap::new())
            .await
            .unwrap();

        assert_eq!(span.status, SpanStatus::Unset);
        assert!(span.end_time.is_none());

        adapter.end_span(&mut span, SpanStatus::Ok).await.unwrap();

        assert_eq!(span.status, SpanStatus::Ok);
        assert!(span.end_time.is_some());
        assert!(span.duration_ms.is_some());
    }

    #[tokio::test]
    async fn test_emit_governance_event() {
        let adapter = ObservatoryAdapter::default();

        let event = GovernanceEvent::AssetRegistered {
            asset_id: "test-123".to_string(),
            asset_name: "test-model".to_string(),
            asset_version: "1.0.0".to_string(),
            registered_by: "test-user".to_string(),
        };

        adapter.emit_governance_event(event).await.unwrap();
        assert_eq!(adapter.pending_events().await, 1);

        adapter.flush().await.unwrap();
        assert_eq!(adapter.pending_events().await, 0);
    }

    #[tokio::test]
    async fn test_trace_asset_registration() {
        let adapter = ObservatoryAdapter::default();

        adapter
            .trace_asset_registration("id-123", "my-model", "1.0.0", "user@example.com")
            .await
            .unwrap();

        assert_eq!(adapter.pending_events().await, 1);
    }

    #[tokio::test]
    async fn test_disabled_adapter() {
        let adapter = ObservatoryAdapter::default().with_enabled(false);

        adapter
            .trace_asset_registration("id-123", "my-model", "1.0.0", "user@example.com")
            .await
            .unwrap();

        // No events should be buffered when disabled
        assert_eq!(adapter.pending_events().await, 0);
    }

    #[tokio::test]
    async fn test_health_status_creation() {
        let status = ObservatoryAdapter::create_health_status(
            true, 5,   // database
            true, 2,   // cache
            false, 0,  // search (unhealthy)
        );

        assert!(!status.healthy); // Overall unhealthy due to search
        assert!(status.components.get("database").unwrap().healthy);
        assert!(status.components.get("cache").unwrap().healthy);
        assert!(!status.components.get("search").unwrap().healthy);
    }
}
