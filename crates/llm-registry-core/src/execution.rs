//! Agentics Execution System types
//!
//! This module defines the foundational types for integrating the LLM Registry
//! as a Foundational Execution Unit (FEU) within the Agentics execution system.
//!
//! Every externally-invoked operation must produce a hierarchical span tree:
//!
//! ```text
//! Core (caller)
//!   └─ Repo span (this repo: "llm-registry")
//!       └─ Agent span (one per service invocation)
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ulid::Ulid;

// ============================================================================
// Identifiers
// ============================================================================

/// Unique identifier for a span within an execution graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpanId(Ulid);

impl SpanId {
    /// Generate a new unique SpanId.
    pub fn new() -> Self {
        Self(Ulid::new())
    }

    /// Parse a SpanId from its string representation.
    pub fn from_string(s: &str) -> Result<Self, String> {
        Ulid::from_string(s)
            .map(Self)
            .map_err(|e| format!("Invalid SpanId: {}", e))
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Execution-wide identifier assigned by the orchestrating Core.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutionId(String);

impl ExecutionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Execution context
// ============================================================================

/// The execution context that arrives with every external request.
///
/// This is provided by the calling Core and is mandatory for all `/v1/*`
/// operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Execution-wide identifier assigned by the orchestrator.
    pub execution_id: ExecutionId,
    /// The parent span ID from the calling entity (the Core's span).
    pub parent_span_id: SpanId,
}

// ============================================================================
// Span types
// ============================================================================

/// Discriminator for span hierarchy level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanType {
    /// Repository-level span (top of the FEU hierarchy).
    Repo,
    /// Agent-level span (one per service invocation).
    Agent,
}

/// Terminal status of a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Failed,
}

/// An artifact produced by an agent and attached to its span.
///
/// Artifacts MUST only be attached to agent-level spans, never to repo spans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanArtifact {
    /// Artifact name (e.g., "registered_asset", "validation_report").
    pub name: String,
    /// MIME type hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// The artifact payload (must be JSON-serializable).
    pub data: serde_json::Value,
}

/// A single execution span (repo-level or agent-level).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSpan {
    pub span_id: SpanId,
    pub parent_span_id: SpanId,
    pub span_type: SpanType,
    /// For repo spans: "llm-registry". For agent spans: the service name.
    pub name: String,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    pub status: SpanStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<SpanArtifact>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, serde_json::Value>,
}

/// The finalized execution result included in every response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub execution_id: ExecutionId,
    pub spans: Vec<ExecutionSpan>,
}

// ============================================================================
// Span collector
// ============================================================================

/// Append-only, thread-safe span collector scoped to a single request.
///
/// Created by the execution middleware and shared with handlers via Axum
/// request extensions. The collector automatically creates the repo-level
/// span on construction.
#[derive(Debug, Clone)]
pub struct SpanCollector {
    inner: Arc<Mutex<SpanCollectorInner>>,
}

#[derive(Debug)]
struct SpanCollectorInner {
    execution_id: ExecutionId,
    repo_span_id: SpanId,
    spans: Vec<ExecutionSpan>,
}

impl SpanCollector {
    /// Create a new collector. Automatically creates the repo-level span.
    pub fn new(ctx: &ExecutionContext) -> Self {
        let repo_span_id = SpanId::new();
        let repo_span = ExecutionSpan {
            span_id: repo_span_id,
            parent_span_id: ctx.parent_span_id,
            span_type: SpanType::Repo,
            name: "llm-registry".to_string(),
            started_at: Utc::now(),
            ended_at: None,
            status: SpanStatus::Ok,
            artifacts: vec![],
            attributes: HashMap::new(),
        };
        Self {
            inner: Arc::new(Mutex::new(SpanCollectorInner {
                execution_id: ctx.execution_id.clone(),
                repo_span_id,
                spans: vec![repo_span],
            })),
        }
    }

    /// Returns the repo-level span ID (used as parent for agent spans).
    pub fn repo_span_id(&self) -> SpanId {
        self.inner.lock().unwrap().repo_span_id
    }

    /// Begin a new agent-level span. Returns its SpanId.
    pub fn begin_agent_span(&self, agent_name: &str) -> SpanId {
        let mut inner = self.inner.lock().unwrap();
        let span_id = SpanId::new();
        inner.spans.push(ExecutionSpan {
            span_id,
            parent_span_id: inner.repo_span_id,
            span_type: SpanType::Agent,
            name: agent_name.to_string(),
            started_at: Utc::now(),
            ended_at: None,
            status: SpanStatus::Ok,
            artifacts: vec![],
            attributes: HashMap::new(),
        });
        span_id
    }

    /// Close an agent span with the given status.
    pub fn end_agent_span(&self, span_id: SpanId, status: SpanStatus) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(span) = inner.spans.iter_mut().find(|s| s.span_id == span_id) {
            span.ended_at = Some(Utc::now());
            span.status = status;
        }
    }

    /// Attach an artifact to an agent span.
    ///
    /// Returns an error if the target span is a repo span (artifacts MUST
    /// only be attached at the agent level).
    pub fn attach_artifact(&self, span_id: SpanId, artifact: SpanArtifact) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        let span = inner
            .spans
            .iter_mut()
            .find(|s| s.span_id == span_id)
            .ok_or_else(|| format!("Span not found: {}", span_id))?;
        if span.span_type == SpanType::Repo {
            return Err("Cannot attach artifacts to repo-level spans".to_string());
        }
        span.artifacts.push(artifact);
        Ok(())
    }

    /// Returns `true` if at least one agent-level span has been recorded.
    pub fn has_agent_spans(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.spans.iter().any(|s| s.span_type == SpanType::Agent)
    }

    /// Finalize the collector: close the repo span, propagate failure status,
    /// and return the complete execution result.
    ///
    /// If any agent span has status `Failed`, the repo span is also marked
    /// `Failed`.
    pub fn finalize(&self) -> ExecutionResult {
        let mut inner = self.inner.lock().unwrap();
        let any_failed = inner
            .spans
            .iter()
            .any(|s| s.status == SpanStatus::Failed);
        // Close repo span
        if let Some(repo) = inner.spans.first_mut() {
            repo.ended_at = Some(Utc::now());
            if any_failed {
                repo.status = SpanStatus::Failed;
            }
        }
        ExecutionResult {
            execution_id: inner.execution_id.clone(),
            spans: inner.spans.clone(),
        }
    }

    /// Finalize with an explicit failure status on the repo span.
    pub fn finalize_failed(&self, reason: &str) -> ExecutionResult {
        let mut inner = self.inner.lock().unwrap();
        if let Some(repo) = inner.spans.first_mut() {
            repo.ended_at = Some(Utc::now());
            repo.status = SpanStatus::Failed;
            repo.attributes.insert(
                "failure_reason".to_string(),
                serde_json::Value::String(reason.to_string()),
            );
        }
        ExecutionResult {
            execution_id: inner.execution_id.clone(),
            spans: inner.spans.clone(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> ExecutionContext {
        ExecutionContext {
            execution_id: ExecutionId::new("test-exec-001"),
            parent_span_id: SpanId::new(),
        }
    }

    #[test]
    fn test_span_id_roundtrip() {
        let id = SpanId::new();
        let s = id.to_string();
        let parsed = SpanId::from_string(&s).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_span_id_invalid() {
        assert!(SpanId::from_string("not-a-ulid").is_err());
    }

    #[test]
    fn test_collector_creates_repo_span() {
        let ctx = test_context();
        let collector = SpanCollector::new(&ctx);
        let result = collector.finalize();

        assert_eq!(result.execution_id, ctx.execution_id);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span_type, SpanType::Repo);
        assert_eq!(result.spans[0].name, "llm-registry");
        assert_eq!(result.spans[0].parent_span_id, ctx.parent_span_id);
        assert!(result.spans[0].ended_at.is_some());
    }

    #[test]
    fn test_collector_agent_span_lifecycle() {
        let ctx = test_context();
        let collector = SpanCollector::new(&ctx);

        assert!(!collector.has_agent_spans());

        let agent_id = collector.begin_agent_span("RegistrationService");
        assert!(collector.has_agent_spans());

        collector.end_agent_span(agent_id, SpanStatus::Ok);
        let result = collector.finalize();

        assert_eq!(result.spans.len(), 2);
        let agent = &result.spans[1];
        assert_eq!(agent.span_type, SpanType::Agent);
        assert_eq!(agent.name, "RegistrationService");
        assert_eq!(agent.parent_span_id, collector.repo_span_id());
        assert_eq!(agent.status, SpanStatus::Ok);
        assert!(agent.ended_at.is_some());
        // Repo span should be Ok since agent is Ok
        assert_eq!(result.spans[0].status, SpanStatus::Ok);
    }

    #[test]
    fn test_collector_failure_propagates_to_repo() {
        let ctx = test_context();
        let collector = SpanCollector::new(&ctx);

        let agent_id = collector.begin_agent_span("ValidationService");
        collector.end_agent_span(agent_id, SpanStatus::Failed);

        let result = collector.finalize();
        assert_eq!(result.spans[0].status, SpanStatus::Failed);
    }

    #[test]
    fn test_attach_artifact_to_agent_span() {
        let ctx = test_context();
        let collector = SpanCollector::new(&ctx);
        let agent_id = collector.begin_agent_span("SearchService");

        let artifact = SpanArtifact {
            name: "search_results".to_string(),
            content_type: Some("application/json".to_string()),
            data: serde_json::json!({"count": 5}),
        };
        assert!(collector.attach_artifact(agent_id, artifact).is_ok());

        collector.end_agent_span(agent_id, SpanStatus::Ok);
        let result = collector.finalize();
        assert_eq!(result.spans[1].artifacts.len(), 1);
        assert_eq!(result.spans[1].artifacts[0].name, "search_results");
    }

    #[test]
    fn test_attach_artifact_to_repo_span_rejected() {
        let ctx = test_context();
        let collector = SpanCollector::new(&ctx);
        let repo_id = collector.repo_span_id();

        let artifact = SpanArtifact {
            name: "bad".to_string(),
            content_type: None,
            data: serde_json::json!(null),
        };
        assert!(collector.attach_artifact(repo_id, artifact).is_err());
    }

    #[test]
    fn test_execution_result_serialization() {
        let ctx = test_context();
        let collector = SpanCollector::new(&ctx);
        let agent_id = collector.begin_agent_span("IntegrityService");
        collector.end_agent_span(agent_id, SpanStatus::Ok);
        let result = collector.finalize();

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.spans.len(), 2);
        assert_eq!(parsed.execution_id, result.execution_id);
    }

    #[test]
    fn test_multiple_agent_spans() {
        let ctx = test_context();
        let collector = SpanCollector::new(&ctx);

        let a1 = collector.begin_agent_span("ValidationService");
        collector.end_agent_span(a1, SpanStatus::Ok);

        let a2 = collector.begin_agent_span("RegistrationService");
        collector.end_agent_span(a2, SpanStatus::Ok);

        let result = collector.finalize();
        assert_eq!(result.spans.len(), 3); // 1 repo + 2 agents
        assert_eq!(result.spans[1].name, "ValidationService");
        assert_eq!(result.spans[2].name, "RegistrationService");
    }
}
