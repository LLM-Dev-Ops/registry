//! Thin adapter modules for LLM-Dev-Ops upstream integrations
//!
//! This module provides runtime consumption adapters for:
//! - Schema Registry: Canonical schema definitions for model metadata and pipeline descriptors
//! - Config Manager: Configuration-driven registry policies, TTLs, and validation constraints
//! - Observatory: Telemetry signals, governance events, and registry health traces
//!
//! These adapters are additive and do not modify existing registry logic.

pub mod schema_registry;
pub mod config_manager;
pub mod observatory;

// Re-export adapter types for convenience
pub use schema_registry::SchemaRegistryAdapter;
pub use config_manager::ConfigManagerAdapter;
pub use observatory::ObservatoryAdapter;
