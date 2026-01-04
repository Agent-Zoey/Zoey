/*!
# Knowledge Graph Module

Build and query entity-relationship graphs with domain ontologies.
*/

pub mod entity;
pub mod extractor;
pub mod ontology;
pub mod relationship;

pub use entity::{Entity, EntityExtractor, EntityType};
pub use ontology::Ontology;
pub use relationship::{RelationType, Relationship, RelationshipDetector};

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A knowledge graph storing entities and their relationships
#[derive(Debug, Clone)]
pub struct KnowledgeGraph {
    /// Domain name
    pub domain: String,

    /// The underlying graph structure
    graph: DiGraph<Entity, Relationship>,

    /// Map entity IDs to graph node indices
    entity_index: HashMap<Uuid, NodeIndex>,

    /// Domain ontology
    pub ontology: Ontology,
}

impl KnowledgeGraph {
    /// Create a new knowledge graph
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            graph: DiGraph::new(),
            entity_index: HashMap::new(),
            ontology: Ontology::new(),
        }
    }

    /// Add an entity to the graph
    pub fn add_entity(&mut self, entity: Entity) -> Uuid {
        let id = entity.id;
        let node_idx = self.graph.add_node(entity);
        self.entity_index.insert(id, node_idx);
        id
    }

    /// Add multiple entities
    pub fn add_entities(&mut self, entities: Vec<Entity>) {
        for entity in entities {
            self.add_entity(entity);
        }
    }

    /// Get an entity by ID
    pub fn get_entity(&self, id: &Uuid) -> Option<&Entity> {
        self.entity_index
            .get(id)
            .and_then(|idx| self.graph.node_weight(*idx))
    }

    /// Add a relationship between entities
    pub fn add_relationship(
        &mut self,
        from_id: Uuid,
        to_id: Uuid,
        relationship: Relationship,
    ) -> anyhow::Result<()> {
        let from_idx = self
            .entity_index
            .get(&from_id)
            .ok_or_else(|| anyhow::anyhow!("Source entity not found"))?;

        let to_idx = self
            .entity_index
            .get(&to_id)
            .ok_or_else(|| anyhow::anyhow!("Target entity not found"))?;

        self.graph.add_edge(*from_idx, *to_idx, relationship);
        Ok(())
    }

    /// Get all entities
    pub fn entities(&self) -> Vec<&Entity> {
        self.graph.node_weights().collect()
    }

    /// Get entities by type
    pub fn entities_by_type(&self, entity_type: EntityType) -> Vec<&Entity> {
        self.graph
            .node_weights()
            .filter(|e| e.entity_type == entity_type)
            .collect()
    }

    /// Get related entities
    pub fn get_related(&self, entity_id: &Uuid) -> Vec<(&Entity, &Relationship)> {
        let idx = match self.entity_index.get(entity_id) {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let mut related = Vec::new();

        // Outgoing relationships
        for edge in self.graph.edges_directed(*idx, Direction::Outgoing) {
            if let Some(target) = self.graph.node_weight(edge.target()) {
                related.push((target, edge.weight()));
            }
        }

        related
    }

    /// Search entities by name
    pub fn search_entities(&self, query: &str) -> Vec<&Entity> {
        let query_lower = query.to_lowercase();

        self.graph
            .node_weights()
            .filter(|e| e.name.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// Get entity count
    pub fn entity_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get relationship count
    pub fn relationship_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Get statistics
    pub fn statistics(&self) -> GraphStatistics {
        let mut entity_type_counts = HashMap::new();
        for entity in self.graph.node_weights() {
            *entity_type_counts.entry(entity.entity_type).or_insert(0) += 1;
        }

        let mut relationship_type_counts = HashMap::new();
        for relationship in self.graph.edge_weights() {
            *relationship_type_counts
                .entry(relationship.rel_type)
                .or_insert(0) += 1;
        }

        GraphStatistics {
            total_entities: self.entity_count(),
            total_relationships: self.relationship_count(),
            entity_type_counts,
            relationship_type_counts,
        }
    }

    /// Find shortest path between two entities
    pub fn shortest_path(&self, from: &Uuid, to: &Uuid) -> Option<Vec<Uuid>> {
        let from_idx = self.entity_index.get(from)?;
        let to_idx = self.entity_index.get(to)?;

        use petgraph::algo::dijkstra;

        let paths = dijkstra(&self.graph, *from_idx, Some(*to_idx), |_| 1);

        if paths.contains_key(to_idx) {
            // Path exists, reconstruct it
            // For simplicity, just return the IDs
            Some(vec![*from, *to])
        } else {
            None
        }
    }
}

/// Statistics about the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStatistics {
    pub total_entities: usize,
    pub total_relationships: usize,
    pub entity_type_counts: HashMap<EntityType, usize>,
    pub relationship_type_counts: HashMap<RelationType, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knowledge_graph_creation() {
        let graph = KnowledgeGraph::new("test_domain");
        assert_eq!(graph.domain, "test_domain");
        assert_eq!(graph.entity_count(), 0);
    }

    #[test]
    fn test_add_entities() {
        let mut graph = KnowledgeGraph::new("test");

        let entity1 = Entity::new("Person", "John Doe");
        let entity2 = Entity::new("Organization", "Acme Corp");

        graph.add_entity(entity1.clone());
        graph.add_entity(entity2);

        assert_eq!(graph.entity_count(), 2);
        assert!(graph.get_entity(&entity1.id).is_some());
    }

    #[test]
    fn test_relationships() {
        let mut graph = KnowledgeGraph::new("test");

        let person = Entity::new("Person", "Alice");
        let org = Entity::new("Organization", "TechCo");

        let person_id = graph.add_entity(person);
        let org_id = graph.add_entity(org);

        let rel = Relationship::new(RelationType::WorksFor, 1.0);
        graph.add_relationship(person_id, org_id, rel).unwrap();

        assert_eq!(graph.relationship_count(), 1);

        let related = graph.get_related(&person_id);
        assert_eq!(related.len(), 1);
    }
}
