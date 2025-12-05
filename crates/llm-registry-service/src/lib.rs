//! Service layer for LLM Registry
//!
//! This crate provides the service layer that sits between the API and database layers.
//! It implements business logic, orchestration, validation, and event emission.
//!
//! # Architecture
//!
//! The service layer is organized into the following components:
//!
//! - **RegistrationService**: Asset registration with validation and dependency resolution
//! - **SearchService**: Search and query operations
//! - **ValidationService**: Schema and policy validation
//! - **IntegrityService**: Checksum computation and verification
//! - **VersioningService**: Version management and conflict detection
//!
//! # Example
//!
//! ```rust,no_run
//! use llm_registry_service::{
//!     RegistrationService, DefaultRegistrationService,
//!     ValidationService, DefaultValidationService,
//!     IntegrityService, DefaultIntegrityService,
//!     VersioningService, DefaultVersioningService,
//!     SearchService, DefaultSearchService,
//! };
//! use std::sync::Arc;
//!
//! # async fn example(
//! #     repository: Arc<dyn llm_registry_db::AssetRepository>,
//! #     event_store: Arc<dyn llm_registry_db::EventStore>,
//! # ) {
//! // Create service instances
//! let validation_service = Arc::new(DefaultValidationService::new(
//!     repository.clone(),
//!     event_store.clone(),
//! ));
//!
//! let integrity_service = Arc::new(DefaultIntegrityService::new(
//!     repository.clone(),
//!     event_store.clone(),
//! ));
//!
//! let versioning_service = Arc::new(DefaultVersioningService::new(
//!     repository.clone(),
//!     event_store.clone(),
//! ));
//!
//! let registration_service = Arc::new(DefaultRegistrationService::new(
//!     repository.clone(),
//!     event_store.clone(),
//!     validation_service.clone(),
//!     integrity_service.clone(),
//!     versioning_service.clone(),
//! ));
//!
//! let search_service = Arc::new(DefaultSearchService::new(repository.clone()));
//! # }
//! ```

pub mod dto;
pub mod error;
pub mod integrity;
pub mod registration;
pub mod search;
pub mod validation;
pub mod versioning;

// LLM-Dev-Ops upstream integration adapters (Phase 2B)
pub mod adapters;

// Re-export main types for convenience
pub use dto::*;
pub use error::{ServiceError, ServiceResult};

// Re-export service traits and implementations
pub use integrity::{DefaultIntegrityService, IntegrityService};
pub use registration::{DefaultRegistrationService, RegistrationService};
pub use search::{DefaultSearchService, SearchService};
pub use validation::{DefaultValidationService, ValidationService};
pub use versioning::{DefaultVersioningService, VersioningService};

// Re-export upstream adapters for convenience
pub use adapters::{
    ConfigManagerAdapter, ObservatoryAdapter, SchemaRegistryAdapter,
};

use llm_registry_db::{AssetRepository, EventStore};
use std::sync::Arc;

/// Service registry that holds all service instances
///
/// This provides a convenient way to manage all services together
/// and ensures consistent dependency injection.
#[derive(Clone)]
pub struct ServiceRegistry {
    /// Registration service
    pub registration: Arc<dyn RegistrationService>,
    /// Search service
    pub search: Arc<dyn SearchService>,
    /// Validation service
    pub validation: Arc<dyn ValidationService>,
    /// Integrity service
    pub integrity: Arc<dyn IntegrityService>,
    /// Versioning service
    pub versioning: Arc<dyn VersioningService>,
}

impl ServiceRegistry {
    /// Create a new service registry with default implementations
    ///
    /// # Arguments
    ///
    /// * `repository` - Asset repository implementation
    /// * `event_store` - Event store implementation
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use llm_registry_service::ServiceRegistry;
    /// use std::sync::Arc;
    ///
    /// # async fn example(
    /// #     repository: Arc<dyn llm_registry_db::AssetRepository>,
    /// #     event_store: Arc<dyn llm_registry_db::EventStore>,
    /// # ) {
    /// let services = ServiceRegistry::new(repository, event_store);
    /// # }
    /// ```
    pub fn new(
        repository: Arc<dyn AssetRepository>,
        event_store: Arc<dyn EventStore>,
    ) -> Self {
        // Create shared service instances
        let validation = Arc::new(DefaultValidationService::new(
            repository.clone(),
            event_store.clone(),
        ));

        let integrity = Arc::new(DefaultIntegrityService::new(
            repository.clone(),
            event_store.clone(),
        ));

        let versioning = Arc::new(DefaultVersioningService::new(
            repository.clone(),
            event_store.clone(),
        ));

        let search = Arc::new(DefaultSearchService::new(repository.clone()));

        let registration = Arc::new(DefaultRegistrationService::new(
            repository.clone(),
            event_store.clone(),
            validation.clone(),
            integrity.clone(),
            versioning.clone(),
        ));

        Self {
            registration,
            search,
            validation,
            integrity,
            versioning,
        }
    }

    /// Create a service registry with custom implementations
    ///
    /// This allows for dependency injection of custom service implementations
    /// for testing or specialized behavior.
    pub fn with_services(
        registration: Arc<dyn RegistrationService>,
        search: Arc<dyn SearchService>,
        validation: Arc<dyn ValidationService>,
        integrity: Arc<dyn IntegrityService>,
        versioning: Arc<dyn VersioningService>,
    ) -> Self {
        Self {
            registration,
            search,
            validation,
            integrity,
            versioning,
        }
    }

    /// Get the registration service
    pub fn registration(&self) -> &Arc<dyn RegistrationService> {
        &self.registration
    }

    /// Get the search service
    pub fn search(&self) -> &Arc<dyn SearchService> {
        &self.search
    }

    /// Get the validation service
    pub fn validation(&self) -> &Arc<dyn ValidationService> {
        &self.validation
    }

    /// Get the integrity service
    pub fn integrity(&self) -> &Arc<dyn IntegrityService> {
        &self.integrity
    }

    /// Get the versioning service
    pub fn versioning(&self) -> &Arc<dyn VersioningService> {
        &self.versioning
    }
}

/// Builder for ServiceRegistry with custom configuration
pub struct ServiceRegistryBuilder {
    repository: Option<Arc<dyn AssetRepository>>,
    event_store: Option<Arc<dyn EventStore>>,
    validation: Option<Arc<dyn ValidationService>>,
    integrity: Option<Arc<dyn IntegrityService>>,
    versioning: Option<Arc<dyn VersioningService>>,
    search: Option<Arc<dyn SearchService>>,
    registration: Option<Arc<dyn RegistrationService>>,
}

impl ServiceRegistryBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            repository: None,
            event_store: None,
            validation: None,
            integrity: None,
            versioning: None,
            search: None,
            registration: None,
        }
    }

    /// Set the repository
    pub fn repository(mut self, repository: Arc<dyn AssetRepository>) -> Self {
        self.repository = Some(repository);
        self
    }

    /// Set the event store
    pub fn event_store(mut self, event_store: Arc<dyn EventStore>) -> Self {
        self.event_store = Some(event_store);
        self
    }

    /// Set a custom validation service
    pub fn validation_service(mut self, service: Arc<dyn ValidationService>) -> Self {
        self.validation = Some(service);
        self
    }

    /// Set a custom integrity service
    pub fn integrity_service(mut self, service: Arc<dyn IntegrityService>) -> Self {
        self.integrity = Some(service);
        self
    }

    /// Set a custom versioning service
    pub fn versioning_service(mut self, service: Arc<dyn VersioningService>) -> Self {
        self.versioning = Some(service);
        self
    }

    /// Set a custom search service
    pub fn search_service(mut self, service: Arc<dyn SearchService>) -> Self {
        self.search = Some(service);
        self
    }

    /// Set a custom registration service
    pub fn registration_service(mut self, service: Arc<dyn RegistrationService>) -> Self {
        self.registration = Some(service);
        self
    }

    /// Build the service registry
    ///
    /// This will create default implementations for any services not explicitly set.
    ///
    /// # Errors
    ///
    /// Returns an error if repository or event_store are not set and custom services
    /// requiring them are not provided.
    pub fn build(self) -> Result<ServiceRegistry, String> {
        let repository = self.repository.ok_or("Repository is required")?;
        let event_store = self.event_store.ok_or("Event store is required")?;

        // Create or use provided services
        let validation = self.validation.unwrap_or_else(|| {
            Arc::new(DefaultValidationService::new(
                repository.clone(),
                event_store.clone(),
            ))
        });

        let integrity = self.integrity.unwrap_or_else(|| {
            Arc::new(DefaultIntegrityService::new(
                repository.clone(),
                event_store.clone(),
            ))
        });

        let versioning = self.versioning.unwrap_or_else(|| {
            Arc::new(DefaultVersioningService::new(
                repository.clone(),
                event_store.clone(),
            ))
        });

        let search = self
            .search
            .unwrap_or_else(|| Arc::new(DefaultSearchService::new(repository.clone())));

        let registration = self.registration.unwrap_or_else(|| {
            Arc::new(DefaultRegistrationService::new(
                repository.clone(),
                event_store.clone(),
                validation.clone(),
                integrity.clone(),
                versioning.clone(),
            ))
        });

        Ok(ServiceRegistry {
            registration,
            search,
            validation,
            integrity,
            versioning,
        })
    }
}

impl Default for ServiceRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_registry_builder() {
        // This test just verifies the builder compiles
        // Actual functionality would require mock implementations
        let _builder = ServiceRegistryBuilder::new();
    }
}
