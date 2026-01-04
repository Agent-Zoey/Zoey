/*!
# Query Expansion

Expands queries with synonyms and related terms.
*/

pub struct QueryExpansion;

impl QueryExpansion {
    /// Expand query with synonyms and variations
    pub fn expand(query: &str) -> Vec<String> {
        let mut expanded = vec![query.to_string()];

        // Simple expansion: add variations
        // In production: use synonym databases, word embeddings

        // Add stemmed version
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.len() > 1 {
            // Try partial queries
            for i in 0..words.len() {
                let partial: String = words[..=i].join(" ");
                if partial != query {
                    expanded.push(partial);
                }
            }
        }

        // Remove duplicates
        expanded.sort();
        expanded.dedup();

        expanded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_expansion() {
        let expanded = QueryExpansion::expand("hypertension treatment");
        assert!(!expanded.is_empty());
        assert!(expanded.contains(&"hypertension treatment".to_string()));
    }
}
