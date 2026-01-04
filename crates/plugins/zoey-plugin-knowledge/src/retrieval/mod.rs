/*!
# Advanced Retrieval Module

Hybrid search combining semantic, lexical (BM25), and graph-based retrieval.
*/

pub mod bm25;
pub mod graph_search;
pub mod query_expansion;
pub mod reranker;
pub mod semantic;

pub use bm25::BM25Search;
pub use graph_search::GraphSearch;
pub use query_expansion::QueryExpansion;
pub use reranker::ReRanker;
pub use semantic::SemanticSearch;

use crate::graph::KnowledgeGraph;
use serde::{Deserialize, Serialize};

/// A search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Text snippet
    pub text: String,

    /// Overall relevance score
    pub score: f64,

    /// Score breakdown
    pub scores: ScoreBreakdown,

    /// Source document ID (if applicable)
    pub document_id: Option<uuid::Uuid>,

    /// Entity ID (if from knowledge graph)
    pub entity_id: Option<uuid::Uuid>,
}

/// Score breakdown from different retrieval methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub semantic_score: f64,
    pub lexical_score: f64,
    pub graph_score: f64,
}

/// Hybrid retriever combining multiple search strategies
pub struct HybridRetriever {
    semantic: SemanticSearch,
    bm25: BM25Search,
    graph: GraphSearch,
    config: crate::KnowledgeConfig,
}

impl HybridRetriever {
    /// Create a new hybrid retriever
    pub fn new(knowledge_graph: KnowledgeGraph, corpus: Vec<String>) -> Self {
        Self {
            semantic: SemanticSearch::new(),
            bm25: BM25Search::new(corpus.clone()),
            graph: GraphSearch::new(knowledge_graph),
            config: crate::KnowledgeConfig::default(),
        }
    }

    /// Search with hybrid approach
    pub async fn search(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        // 1. Query expansion
        let expanded_queries = QueryExpansion::expand(query);

        let mut all_results = Vec::new();

        for expanded_query in &expanded_queries {
            // 2. Semantic search
            let semantic_results = self.semantic.search(expanded_query).await;

            // 3. BM25 lexical search
            let bm25_results = self.bm25.search(expanded_query, top_k * 2);

            // 4. Graph-based search
            let graph_results = self.graph.search(expanded_query);

            // 5. Combine results
            all_results.extend(self.combine_results(semantic_results, bm25_results, graph_results));
        }

        // 6. Re-rank
        let reranked = ReRanker::rerank(&all_results, query);

        // 7. Return top-k
        Ok(reranked.into_iter().take(top_k).collect())
    }

    fn combine_results(
        &self,
        semantic: Vec<(String, f64)>,
        bm25: Vec<(String, f64)>,
        graph: Vec<(String, f64)>,
    ) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Combine all results
        let max_results = semantic.len().max(bm25.len()).max(graph.len());

        for i in 0..max_results {
            // Add semantic result
            if i < semantic.len() {
                let (text, score) = &semantic[i];
                if seen.insert(text.clone()) {
                    results.push(SearchResult {
                        text: text.clone(),
                        score: self.calculate_combined_score(*score, 0.0, 0.0),
                        scores: ScoreBreakdown {
                            semantic_score: *score,
                            lexical_score: 0.0,
                            graph_score: 0.0,
                        },
                        document_id: None,
                        entity_id: None,
                    });
                }
            }

            // Add BM25 result
            if i < bm25.len() {
                let (text, score) = &bm25[i];
                if let Some(existing) = results.iter_mut().find(|r| &r.text == text) {
                    // Update existing result
                    existing.scores.lexical_score = *score;
                    existing.score = self.calculate_combined_score(
                        existing.scores.semantic_score,
                        *score,
                        existing.scores.graph_score,
                    );
                } else if seen.insert(text.clone()) {
                    results.push(SearchResult {
                        text: text.clone(),
                        score: self.calculate_combined_score(0.0, *score, 0.0),
                        scores: ScoreBreakdown {
                            semantic_score: 0.0,
                            lexical_score: *score,
                            graph_score: 0.0,
                        },
                        document_id: None,
                        entity_id: None,
                    });
                }
            }

            // Add graph result
            if i < graph.len() {
                let (text, score) = &graph[i];
                if let Some(existing) = results.iter_mut().find(|r| &r.text == text) {
                    existing.scores.graph_score = *score;
                    existing.score = self.calculate_combined_score(
                        existing.scores.semantic_score,
                        existing.scores.lexical_score,
                        *score,
                    );
                } else if seen.insert(text.clone()) {
                    results.push(SearchResult {
                        text: text.clone(),
                        score: self.calculate_combined_score(0.0, 0.0, *score),
                        scores: ScoreBreakdown {
                            semantic_score: 0.0,
                            lexical_score: 0.0,
                            graph_score: *score,
                        },
                        document_id: None,
                        entity_id: None,
                    });
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    fn calculate_combined_score(&self, semantic: f64, lexical: f64, graph: f64) -> f64 {
        (semantic * self.config.semantic_weight)
            + (lexical * self.config.lexical_weight)
            + (graph * self.config.graph_weight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::KnowledgeGraph;

    #[test]
    fn test_hybrid_retriever() {
        let graph = KnowledgeGraph::new("test");
        let corpus = vec!["Test document 1".to_string(), "Test document 2".to_string()];

        let _retriever = HybridRetriever::new(graph, corpus);
    }

    #[test]
    fn test_score_combination() {
        let graph = KnowledgeGraph::new("test");
        let retriever = HybridRetriever::new(graph, vec![]);

        let score = retriever.calculate_combined_score(0.8, 0.6, 0.4);
        assert!(score > 0.0 && score <= 1.0);
    }
}
