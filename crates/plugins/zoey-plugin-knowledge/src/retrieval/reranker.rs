/*!
# Re-ranker

Re-ranks search results for better relevance.
*/

use super::SearchResult;

pub struct ReRanker;

impl ReRanker {
    /// Re-rank search results
    pub fn rerank(results: &[SearchResult], query: &str) -> Vec<SearchResult> {
        let mut reranked = results.to_vec();

        // Apply query-specific boosting
        for result in &mut reranked {
            result.score *= Self::calculate_boost(query, &result.text);
        }

        // Sort by adjusted score
        reranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        reranked
    }

    fn calculate_boost(query: &str, text: &str) -> f64 {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        // Exact match boost
        if text_lower.contains(&query_lower) {
            1.2
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval::ScoreBreakdown;

    #[test]
    fn test_reranking() {
        let results = vec![
            SearchResult {
                text: "brown dog".to_string(),
                score: 0.5,
                scores: ScoreBreakdown {
                    semantic_score: 0.5,
                    lexical_score: 0.0,
                    graph_score: 0.0,
                },
                document_id: None,
                entity_id: None,
            },
            SearchResult {
                text: "quick brown fox".to_string(),
                score: 0.7,
                scores: ScoreBreakdown {
                    semantic_score: 0.7,
                    lexical_score: 0.0,
                    graph_score: 0.0,
                },
                document_id: None,
                entity_id: None,
            },
        ];

        let reranked = ReRanker::rerank(&results, "brown");
        assert_eq!(reranked.len(), 2);
    }
}
