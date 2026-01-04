/*!
# BM25 Lexical Search

Term-based ranking using BM25 algorithm.
*/

use rust_stemmers::{Algorithm, Stemmer};
use std::collections::HashMap;

pub struct BM25Search {
    corpus: Vec<String>,
    stemmer: Stemmer,
    k1: f64,
    b: f64,
}

impl BM25Search {
    pub fn new(corpus: Vec<String>) -> Self {
        Self {
            corpus,
            stemmer: Stemmer::create(Algorithm::English),
            k1: 1.2,
            b: 0.75,
        }
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f64)> {
        if self.corpus.is_empty() {
            return Vec::new();
        }

        let query_terms = self.tokenize_and_stem(query);
        let avg_doc_len = self.average_document_length();

        let mut scores: Vec<(usize, f64)> = self
            .corpus
            .iter()
            .enumerate()
            .map(|(idx, doc)| {
                let score = self.bm25_score(&query_terms, doc, avg_doc_len);
                (idx, score)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scores
            .into_iter()
            .take(top_k)
            .map(|(idx, score)| (self.corpus[idx].clone(), score))
            .collect()
    }

    fn bm25_score(&self, query_terms: &[String], document: &str, avg_doc_len: f64) -> f64 {
        let doc_terms = self.tokenize_and_stem(document);
        let doc_len = doc_terms.len() as f64;

        let term_freqs = self.term_frequencies(&doc_terms);

        query_terms
            .iter()
            .map(|term| {
                let tf = *term_freqs.get(term).unwrap_or(&0) as f64;
                let idf = self.inverse_document_frequency(term);

                let numerator = tf * (self.k1 + 1.0);
                let denominator = tf + self.k1 * (1.0 - self.b + self.b * (doc_len / avg_doc_len));

                idf * (numerator / denominator)
            })
            .sum()
    }

    fn inverse_document_frequency(&self, term: &str) -> f64 {
        let n = self.corpus.len() as f64;
        let df = self
            .corpus
            .iter()
            .filter(|doc| self.tokenize_and_stem(doc).contains(&term.to_string()))
            .count() as f64;

        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    fn tokenize_and_stem(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split_whitespace()
            .filter(|word| word.len() > 2)
            .map(|word| {
                let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
                self.stemmer.stem(cleaned).to_string()
            })
            .collect()
    }

    fn term_frequencies(&self, terms: &[String]) -> HashMap<String, usize> {
        let mut freqs = HashMap::new();
        for term in terms {
            *freqs.entry(term.clone()).or_insert(0) += 1;
        }
        freqs
    }

    fn average_document_length(&self) -> f64 {
        if self.corpus.is_empty() {
            return 0.0;
        }

        let total: usize = self
            .corpus
            .iter()
            .map(|doc| self.tokenize_and_stem(doc).len())
            .sum();

        total as f64 / self.corpus.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_search() {
        let corpus = vec![
            "The quick brown fox jumps over the lazy dog".to_string(),
            "A brown dog sleeps in the sun".to_string(),
            "The fox is quick and clever".to_string(),
        ];

        let bm25 = BM25Search::new(corpus);
        let results = bm25.search("brown fox", 2);

        assert_eq!(results.len(), 2);
        assert!(results[0].1 > 0.0);
    }
}
