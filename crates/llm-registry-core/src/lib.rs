//! Core domain models and types for LLM Registry
//!
//! This crate contains the core data structures, enums, and domain logic
//! that represent assets, metadata, dependencies, and related concepts in
//! the LLM Registry system.

pub mod asset;
pub mod checksum;
pub mod dependency;
pub mod error;
pub mod event;
pub mod execution;
pub mod provenance;
pub mod storage;
pub mod types;

// Re-exports for convenience
pub use asset::{Asset, AssetMetadata, AssetType};
pub use checksum::{Checksum, HashAlgorithm};
pub use dependency::{AssetReference, DependencyGraph};
pub use error::{RegistryError, Result};
pub use event::{EventType, RegistryEvent};
pub use execution::{
    ExecutionContext, ExecutionId, ExecutionResult, ExecutionSpan, SpanArtifact, SpanCollector,
    SpanId, SpanStatus, SpanType,
};
pub use provenance::Provenance;
pub use storage::{StorageBackend, StorageLocation};
pub use types::{AssetId, AssetStatus, Tags, Annotations};
