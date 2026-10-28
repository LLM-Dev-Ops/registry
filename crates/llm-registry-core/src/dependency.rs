//! Asset dependency management and graph structures
//!
//! This module provides types for representing and managing dependencies between assets,
//! including circular dependency detection and dependency graph analysis.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::error::{RegistryError, Result};
use crate::types::AssetId;

/// A reference to an asset as a dependency
///
/// This can reference an asset either by its unique ID or by name and version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssetReference {
    /// Reference by unique asset ID
    ById {
        /// The unique asset identifier
        id: AssetId,
    },
    /// Reference by name and version
    ByNameVersion {
        /// Asset name
        name: String,
        /// Semantic version or version constraint
        version: String,
    },
}

impl AssetReference {
    /// Create a reference by ID
    pub fn by_id(id: AssetId) -> Self {
        AssetReference::ById { id }
    }

    /// Create a reference by name and version
    pub fn by_name_version(name: impl Into<String>, version: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let version = version.into();

        if name.is_empty() {
            return Err(RegistryError::ValidationError(
                "Asset name cannot be empty".to_string(),
            ));
        }

        if version.is_empty() {
            return Err(RegistryError::ValidationError(
                "Asset version cannot be empty".to_string(),
            ));
        }

        Ok(AssetReference::ByNameVersion { name, version })
    }

    /// Get the asset ID if this is an ID reference
    pub fn as_id(&self) -> Option<&AssetId> {
        match self {
            AssetReference::ById { id } => Some(id),
            _ => None,
        }
    }

    /// Get the name and version if this is a name/version reference
    pub fn as_name_version(&self) -> Option<(&str, &str)> {
        match self {
            AssetReference::ByNameVersion { name, version } => {
                Some((name.as_str(), version.as_str()))
            }
            _ => None,
        }
    }

    /// Validate the reference
    pub fn validate(&self) -> Result<()> {
        match self {
            AssetReference::ById { .. } => Ok(()),
            AssetReference::ByNameVersion { name, version } => {
                if name.is_empty() {
                    return Err(RegistryError::ValidationError(
                        "Asset name cannot be empty".to_string(),
                    ));
                }
                if version.is_empty() {
                    return Err(RegistryError::ValidationError(
                        "Asset version cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for AssetReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetReference::ById { id } => write!(f, "id:{}", id),
            AssetReference::ByNameVersion { name, version } => write!(f, "{}@{}", name, version),
        }
    }
}

impl From<AssetId> for AssetReference {
    fn from(id: AssetId) -> Self {
        AssetReference::by_id(id)
    }
}

/// Dependency graph for tracking asset relationships
///
/// Manages the dependency relationships between assets and provides
/// circular dependency detection and topological sorting capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// Map from asset ID to its list of dependencies
    dependencies: HashMap<AssetId, Vec<AssetReference>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
        }
    }

    /// Add dependencies for an asset
    ///
    /// # Arguments
    /// * `asset_id` - The asset that has dependencies
    /// * `deps` - The list of dependencies for this asset
    pub fn add_dependencies(&mut self, asset_id: AssetId, deps: Vec<AssetReference>) -> Result<()> {
        // Validate all references
        for dep in &deps {
            dep.validate()?;
        }

        self.dependencies.insert(asset_id, deps);
        Ok(())
    }

    /// Add a single dependency for an asset
    ///
    /// # Arguments
    /// * `asset_id` - The asset that has a dependency
    /// * `dependency` - The dependency to add
    pub fn add_dependency(&mut self, asset_id: AssetId, dependency: AssetReference) -> Result<()> {
        dependency.validate()?;

        self.dependencies
            .entry(asset_id)
            .or_default()
            .push(dependency);
        Ok(())
    }

    /// Get the dependencies for an asset
    ///
    /// Returns None if the asset has no recorded dependencies.
    pub fn get_dependencies(&self, asset_id: &AssetId) -> Option<&Vec<AssetReference>> {
        self.dependencies.get(asset_id)
    }

    /// Remove all dependencies for an asset
    pub fn remove_asset(&mut self, asset_id: &AssetId) {
        self.dependencies.remove(asset_id);
    }

    /// Check if the graph contains an asset
    pub fn contains_asset(&self, asset_id: &AssetId) -> bool {
        self.dependencies.contains_key(asset_id)
    }

    /// Get the total number of assets in the graph
    pub fn asset_count(&self) -> usize {
        self.dependencies.len()
    }

    /// Detect circular dependencies in the graph
    ///
    /// This performs a depth-first search to detect cycles. If a cycle is found,
    /// returns an error with the cycle path.
    pub fn detect_circular_dependencies(&self) -> Result<()> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for asset_id in &self.sorted_asset_ids() {
            if !visited.contains(asset_id) {
                self.dfs_cycle_detect(asset_id, &mut visited, &mut rec_stack, &mut path)?;
            }
        }

        Ok(())
    }

    /// Recursive helper for cycle detection using DFS
    fn dfs_cycle_detect(
        &self,
        asset_id: &AssetId,
        visited: &mut HashSet<AssetId>,
        rec_stack: &mut HashSet<AssetId>,
        path: &mut Vec<AssetId>,
    ) -> Result<()> {
        visited.insert(*asset_id);
        rec_stack.insert(*asset_id);
        path.push(*asset_id);

        for dep_id in &self.sorted_dependency_ids(asset_id) {
            // Only follow ID-based dependencies for cycle detection
            if !visited.contains(dep_id) {
                self.dfs_cycle_detect(dep_id, visited, rec_stack, path)?;
            } else if rec_stack.contains(dep_id) {
                // Cycle detected - build the cycle path
                let cycle_start = path.iter().position(|id| id == dep_id).unwrap();
                let mut cycle_path: Vec<String> = path[cycle_start..]
                    .iter()
                    .map(|id| id.to_string())
                    .collect();
                cycle_path.push(dep_id.to_string());

                return Err(RegistryError::CircularDependency(format!(
                    "Cycle detected: {}",
                    cycle_path.join(" -> ")
                )));
            }
        }

        rec_stack.remove(asset_id);
        path.pop();
        Ok(())
    }

    /// Get all direct and transitive dependencies for an asset
    ///
    /// Returns a set of all asset IDs that the given asset depends on,
    /// directly or indirectly (only for ID-based references).
    pub fn get_all_dependencies(&self, asset_id: &AssetId) -> HashSet<AssetId> {
        let mut all_deps = HashSet::new();
        let mut to_visit = vec![*asset_id];
        let mut visited = HashSet::new();

        while let Some(current) = to_visit.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            if let Some(deps) = self.dependencies.get(&current) {
                for dep in deps {
                    if let Some(dep_id) = dep.as_id() {
                        all_deps.insert(*dep_id);
                        to_visit.push(*dep_id);
                    }
                }
            }
        }

        all_deps
    }

    /// Get all assets that depend on the given asset (reverse dependencies)
    ///
    /// Only considers ID-based references.
    pub fn get_dependents(&self, asset_id: &AssetId) -> HashSet<AssetId> {
        let mut dependents = HashSet::new();

        for (id, deps) in &self.dependencies {
            for dep in deps {
                if let Some(dep_id) = dep.as_id() {
                    if dep_id == asset_id {
                        dependents.insert(*id);
                        break;
                    }
                }
            }
        }

        dependents
    }

    /// Compute a topological sort of the dependency graph
    ///
    /// Returns assets in an order where dependencies come before dependents.
    /// Only considers ID-based references. Returns an error if a cycle is detected.
    pub fn topological_sort(&self) -> Result<Vec<AssetId>> {
        // First detect cycles
        self.detect_circular_dependencies()?;

        let mut sorted = Vec::new();
        let mut visited = HashSet::new();

        for asset_id in &self.sorted_asset_ids() {
            if !visited.contains(asset_id) {
                self.topological_visit(asset_id, &mut visited, &mut sorted);
            }
        }

        // Post-order DFS gives us dependencies before dependents (no need to reverse)
        Ok(sorted)
    }

    /// Recursive helper for topological sort
    fn topological_visit(
        &self,
        asset_id: &AssetId,
        visited: &mut HashSet<AssetId>,
        sorted: &mut Vec<AssetId>,
    ) {
        visited.insert(*asset_id);

        for dep_id in &self.sorted_dependency_ids(asset_id) {
            if !visited.contains(dep_id) {
                self.topological_visit(dep_id, visited, sorted);
            }
        }

        sorted.push(*asset_id);
    }

    /// Canonical traversal makes equivalent graph serializations produce the
    /// same dependency-first ordering and the same cycle diagnostic.
    fn sorted_asset_ids(&self) -> Vec<AssetId> {
        let mut ids: Vec<_> = self.dependencies.keys().copied().collect();
        ids.sort_unstable_by_key(|id| *id.as_ulid());
        ids
    }

    fn sorted_dependency_ids(&self, asset_id: &AssetId) -> Vec<AssetId> {
        let mut ids: Vec<_> = self
            .dependencies
            .get(asset_id)
            .into_iter()
            .flatten()
            .filter_map(|dep| dep.as_id().copied())
            .collect();
        ids.sort_unstable_by_key(|id| *id.as_ulid());
        ids.dedup();
        ids
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DependencyGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DependencyGraph({} assets)", self.asset_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordered_fixture_ids() -> [AssetId; 3] {
        [1, 2, 3].map(|n| AssetId::from_string(&format!("{n:026}")).unwrap())
    }

    #[test]
    fn test_topological_sort_canonical_dependency_order() {
        let [root, lower, higher] = ordered_fixture_ids();
        let mut graph = DependencyGraph::new();
        graph
            .add_dependencies(
                root,
                vec![AssetReference::by_id(higher), AssetReference::by_id(lower)],
            )
            .unwrap();
        assert_eq!(graph.topological_sort().unwrap(), vec![lower, higher, root]);
        graph
            .add_dependencies(
                root,
                vec![AssetReference::by_id(lower), AssetReference::by_id(higher)],
            )
            .unwrap();
        assert_eq!(graph.topological_sort().unwrap(), vec![lower, higher, root]);
    }

    #[test]
    fn test_cycle_path_canonical_dependency_order() {
        let [root, lower, higher] = ordered_fixture_ids();
        let mut graph = DependencyGraph::new();
        graph
            .add_dependencies(
                root,
                vec![AssetReference::by_id(higher), AssetReference::by_id(lower)],
            )
            .unwrap();
        graph
            .add_dependency(lower, AssetReference::by_id(root))
            .unwrap();
        graph
            .add_dependency(higher, AssetReference::by_id(root))
            .unwrap();
        // Fix the root to isolate edge-order determinism from HashMap seeding.
        let err = graph
            .dfs_cycle_detect(
                &root,
                &mut HashSet::new(),
                &mut HashSet::new(),
                &mut Vec::new(),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .ends_with(&format!("{root} -> {lower} -> {root}")));
    }

    #[test]
    fn test_disconnected_roots_are_canonical() {
        let [lower, middle, higher] = ordered_fixture_ids();
        for order in [[higher, lower, middle], [middle, higher, lower]] {
            let mut graph = DependencyGraph::new();
            for id in order {
                graph.add_dependencies(id, vec![]).unwrap();
            }
            assert_eq!(
                graph.topological_sort().unwrap(),
                vec![lower, middle, higher]
            );
        }
    }

    #[test]
    fn test_asset_reference_by_id() {
        let id = AssetId::new();
        let reference = AssetReference::by_id(id);
        assert_eq!(reference.as_id(), Some(&id));
        assert!(reference.as_name_version().is_none());
    }

    #[test]
    fn test_asset_reference_by_name_version() {
        let reference = AssetReference::by_name_version("gpt-2", "1.0.0").unwrap();
        assert_eq!(reference.as_name_version(), Some(("gpt-2", "1.0.0")));
        assert!(reference.as_id().is_none());
    }

    #[test]
    fn test_asset_reference_validation_empty_name() {
        assert!(AssetReference::by_name_version("", "1.0.0").is_err());
    }

    #[test]
    fn test_asset_reference_validation_empty_version() {
        assert!(AssetReference::by_name_version("gpt-2", "").is_err());
    }

    #[test]
    fn test_dependency_graph_new() {
        let graph = DependencyGraph::new();
        assert_eq!(graph.asset_count(), 0);
    }

    #[test]
    fn test_dependency_graph_add_dependencies() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();

        let deps = vec![AssetReference::by_id(asset2)];
        graph.add_dependencies(asset1, deps).unwrap();

        assert_eq!(graph.asset_count(), 1);
        assert!(graph.contains_asset(&asset1));
    }

    #[test]
    fn test_dependency_graph_add_dependency() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();

        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();

        let deps = graph.get_dependencies(&asset1).unwrap();
        assert_eq!(deps.len(), 1);
    }

    #[test]
    fn test_dependency_graph_get_dependencies() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();

        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();

        let deps = graph.get_dependencies(&asset1).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].as_id(), Some(&asset2));
    }

    #[test]
    fn test_dependency_graph_remove_asset() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();

        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();
        assert!(graph.contains_asset(&asset1));

        graph.remove_asset(&asset1);
        assert!(!graph.contains_asset(&asset1));
    }

    #[test]
    fn test_circular_dependency_detection_simple() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();

        // Create a simple cycle: asset1 -> asset2 -> asset1
        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();
        graph
            .add_dependency(asset2, AssetReference::by_id(asset1))
            .unwrap();

        assert!(graph.detect_circular_dependencies().is_err());
    }

    #[test]
    fn test_circular_dependency_detection_complex() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();
        let asset3 = AssetId::new();

        // Create a cycle: asset1 -> asset2 -> asset3 -> asset1
        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();
        graph
            .add_dependency(asset2, AssetReference::by_id(asset3))
            .unwrap();
        graph
            .add_dependency(asset3, AssetReference::by_id(asset1))
            .unwrap();

        assert!(graph.detect_circular_dependencies().is_err());
    }

    #[test]
    fn test_no_circular_dependency() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();
        let asset3 = AssetId::new();

        // Create a DAG: asset1 -> asset2, asset1 -> asset3
        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();
        graph
            .add_dependency(asset1, AssetReference::by_id(asset3))
            .unwrap();

        assert!(graph.detect_circular_dependencies().is_ok());
    }

    #[test]
    fn test_get_all_dependencies() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();
        let asset3 = AssetId::new();

        // asset1 -> asset2 -> asset3
        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();
        graph
            .add_dependency(asset2, AssetReference::by_id(asset3))
            .unwrap();

        let all_deps = graph.get_all_dependencies(&asset1);
        assert_eq!(all_deps.len(), 2);
        assert!(all_deps.contains(&asset2));
        assert!(all_deps.contains(&asset3));
    }

    #[test]
    fn test_get_dependents() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();
        let asset3 = AssetId::new();

        // asset1 -> asset3, asset2 -> asset3
        graph
            .add_dependency(asset1, AssetReference::by_id(asset3))
            .unwrap();
        graph
            .add_dependency(asset2, AssetReference::by_id(asset3))
            .unwrap();

        let dependents = graph.get_dependents(&asset3);
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&asset1));
        assert!(dependents.contains(&asset2));
    }

    #[test]
    fn test_topological_sort() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();
        let asset3 = AssetId::new();

        // asset1 -> asset2 -> asset3 (asset1 depends on asset2, asset2 depends on asset3)
        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();
        graph
            .add_dependency(asset2, AssetReference::by_id(asset3))
            .unwrap();
        graph.add_dependencies(asset3, vec![]).unwrap(); // asset3 has no dependencies

        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted.len(), 3);

        // In topological order: dependencies come before dependents
        // So asset3 (no deps) should come before asset2 (depends on asset3),
        // and asset2 should come before asset1 (depends on asset2)
        let pos1 = sorted.iter().position(|id| id == &asset1).unwrap();
        let pos2 = sorted.iter().position(|id| id == &asset2).unwrap();
        let pos3 = sorted.iter().position(|id| id == &asset3).unwrap();

        // Verify the order: asset3 < asset2 < asset1
        assert!(
            pos3 < pos2,
            "asset3 (pos {}) should come before asset2 (pos {})",
            pos3,
            pos2
        );
        assert!(
            pos2 < pos1,
            "asset2 (pos {}) should come before asset1 (pos {})",
            pos2,
            pos1
        );
    }

    #[test]
    fn test_topological_sort_with_cycle() {
        let mut graph = DependencyGraph::new();
        let asset1 = AssetId::new();
        let asset2 = AssetId::new();

        // Create a cycle
        graph
            .add_dependency(asset1, AssetReference::by_id(asset2))
            .unwrap();
        graph
            .add_dependency(asset2, AssetReference::by_id(asset1))
            .unwrap();

        assert!(graph.topological_sort().is_err());
    }
}
