/*!
# Graph-based Search

Search using knowledge graph relationships.
*/

use crate::graph::KnowledgeGraph;

pub struct GraphSearch {
    graph: KnowledgeGraph,
}

impl GraphSearch {
    pub fn new(graph: KnowledgeGraph) -> Self {
        Self { graph }
    }

    pub fn search(&self, query: &str) -> Vec<(String, f64)> {
        // Search entities
        let entities = self.graph.search_entities(query);

        entities
            .into_iter()
            .map(|entity| {
                // Score based on name match quality
                let score = self.calculate_match_score(query, &entity.name);

                // Include entity info and related entities
                let mut text = format!("{} (type: {:?})", entity.name, entity.entity_type);

                let related = self.graph.get_related(&entity.id);
                if !related.is_empty() {
                    text.push_str(" - Related: ");
                    let related_names: Vec<String> = related
                        .iter()
                        .take(3)
                        .map(|(e, _)| e.name.clone())
                        .collect();
                    text.push_str(&related_names.join(", "));
                }

                (text, score)
            })
            .collect()
    }

    fn calculate_match_score(&self, query: &str, name: &str) -> f64 {
        let query_lower = query.to_lowercase();
        let name_lower = name.to_lowercase();

        if name_lower == query_lower {
            1.0
        } else if name_lower.contains(&query_lower) {
            0.8
        } else {
            // Simple word overlap
            let query_words: Vec<&str> = query_lower.split_whitespace().collect();
            let name_words: Vec<&str> = name_lower.split_whitespace().collect();

            let overlap = query_words
                .iter()
                .filter(|qw| name_words.iter().any(|nw| nw == *qw))
                .count();

            if query_words.is_empty() {
                0.0
            } else {
                overlap as f64 / query_words.len() as f64 * 0.6
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Entity, KnowledgeGraph};

    #[test]
    fn test_graph_search() {
        let mut graph = KnowledgeGraph::new("test");

        let entity = Entity::new("Person", "John Doe");
        graph.add_entity(entity);

        let search = GraphSearch::new(graph);
        let results = search.search("John");

        assert!(!results.is_empty());
        assert!(results[0].1 > 0.0);
    }
}
