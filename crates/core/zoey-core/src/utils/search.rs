//! Search utilities - BM25 implementation

use std::collections::HashMap;

/// BM25 search algorithm implementation
/// Used as fallback when embeddings are not available
pub struct BM25 {
    documents: Vec<String>,
    idf: HashMap<String, f32>,
    doc_lengths: Vec<usize>,
    avg_doc_length: f32,
    k1: f32,
    b: f32,
}

impl BM25 {
    /// Create a new BM25 instance with default parameters
    pub fn new(documents: Vec<String>) -> Self {
        Self::with_params(documents, 1.5, 0.75)
    }

    /// Create a new BM25 instance with custom parameters
    pub fn with_params(documents: Vec<String>, k1: f32, b: f32) -> Self {
        let doc_lengths: Vec<usize> = documents
            .iter()
            .map(|doc| doc.split_whitespace().count())
            .collect();

        let avg_doc_length = if doc_lengths.is_empty() {
            0.0
        } else {
            doc_lengths.iter().sum::<usize>() as f32 / doc_lengths.len() as f32
        };

        let idf = Self::compute_idf(&documents);

        Self {
            documents,
            idf,
            doc_lengths,
            avg_doc_length,
            k1,
            b,
        }
    }

    /// Compute IDF (Inverse Document Frequency) for all terms
    fn compute_idf(documents: &[String]) -> HashMap<String, f32> {
        let n = documents.len() as f32;
        let mut term_doc_count: HashMap<String, usize> = HashMap::new();

        // Count documents containing each term
        for doc in documents {
            let mut seen_terms = std::collections::HashSet::new();
            for term in doc.split_whitespace() {
                let term = term.to_lowercase();
                if seen_terms.insert(term.clone()) {
                    *term_doc_count.entry(term).or_insert(0) += 1;
                }
            }
        }

        // Compute IDF for each term
        term_doc_count
            .into_iter()
            .map(|(term, count)| {
                let idf = ((n - count as f32 + 0.5) / (count as f32 + 0.5) + 1.0).ln();
                (term, idf)
            })
            .collect()
    }

    /// Search for documents matching the query
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(usize, f32)> {
        let query_terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

        let mut scores: Vec<(usize, f32)> = self
            .documents
            .iter()
            .enumerate()
            .map(|(idx, doc)| {
                let score = self.score_document(doc, &query_terms, idx);
                (idx, score)
            })
            .collect();

        // Sort by score (descending)
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top K results
        scores.into_iter().take(top_k).collect()
    }

    /// Score a single document against query terms
    fn score_document(&self, doc: &str, query_terms: &[String], doc_idx: usize) -> f32 {
        let doc_length = self.doc_lengths[doc_idx] as f32;

        // Count term frequencies in document
        let mut term_freqs: HashMap<String, usize> = HashMap::new();
        for term in doc.split_whitespace() {
            let term = term.to_lowercase();
            *term_freqs.entry(term).or_insert(0) += 1;
        }

        // Compute BM25 score
        let mut score = 0.0;
        for query_term in query_terms {
            let tf = *term_freqs.get(query_term).unwrap_or(&0) as f32;
            let idf = *self.idf.get(query_term).unwrap_or(&0.0);

            let numerator = tf * (self.k1 + 1.0);
            let denominator =
                tf + self.k1 * (1.0 - self.b + self.b * (doc_length / self.avg_doc_length));

            score += idf * (numerator / denominator);
        }

        score
    }

    /// Get the number of documents
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_creation() {
        let docs = vec![
            "the quick brown fox".to_string(),
            "the lazy dog".to_string(),
            "quick brown dog".to_string(),
        ];

        let bm25 = BM25::new(docs);
        assert_eq!(bm25.len(), 3);
    }

    #[test]
    fn test_bm25_search() {
        let docs = vec![
            "the quick brown fox jumps over the lazy dog".to_string(),
            "the quick brown dog runs fast".to_string(),
            "a lazy cat sleeps all day".to_string(),
        ];

        let bm25 = BM25::new(docs);
        let results = bm25.search("quick brown dog", 2);

        assert_eq!(results.len(), 2);
        // Should return indices 1 and 0 (in that order, as doc 1 matches better)
        assert_eq!(results[0].0, 1); // "the quick brown dog runs fast"
    }

    #[test]
    fn test_bm25_empty() {
        let docs: Vec<String> = vec![];
        let bm25 = BM25::new(docs);
        assert!(bm25.is_empty());
        assert_eq!(bm25.len(), 0);
    }
}
