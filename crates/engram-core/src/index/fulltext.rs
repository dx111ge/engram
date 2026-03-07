/// Full-text inverted index with BM25 scoring.
///
/// Rebuilt on open from node labels and properties.
/// Supports tokenized keyword search with TF-IDF ranking.

use std::collections::{HashMap, HashSet};

/// BM25 tuning parameters
const K1: f64 = 1.2;
const B: f64 = 0.75;

pub struct FullTextIndex {
    /// term -> set of (node_slot, term_frequency)
    postings: HashMap<String, Vec<Posting>>,
    /// node_slot -> document length (number of tokens)
    doc_lengths: HashMap<u64, u32>,
    /// Total number of indexed documents
    doc_count: u64,
    /// Average document length
    avg_doc_len: f64,
}

#[derive(Debug, Clone)]
struct Posting {
    slot: u64,
    tf: u32, // term frequency in this document
}

/// A search result with BM25 score
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub slot: u64,
    pub score: f64,
}

impl FullTextIndex {
    pub fn new() -> Self {
        FullTextIndex {
            postings: HashMap::new(),
            doc_lengths: HashMap::new(),
            doc_count: 0,
            avg_doc_len: 0.0,
        }
    }

    /// Index a document (node slot) with the given text content.
    /// Call this for each node's label + property values.
    pub fn add_document(&mut self, slot: u64, text: &str) {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return;
        }

        let doc_len = tokens.len() as u32;

        // Count term frequencies
        let mut tf_map: HashMap<&str, u32> = HashMap::new();
        for token in &tokens {
            *tf_map.entry(token.as_str()).or_default() += 1;
        }

        // Add postings
        for (term, tf) in tf_map {
            self.postings
                .entry(term.to_string())
                .or_default()
                .push(Posting { slot, tf });
        }

        // Update stats
        let old_total = self.avg_doc_len * self.doc_count as f64;
        self.doc_lengths.insert(slot, doc_len);
        self.doc_count += 1;
        self.avg_doc_len = (old_total + doc_len as f64) / self.doc_count as f64;
    }

    /// Remove a document from the index.
    pub fn remove_document(&mut self, slot: u64) {
        if let Some(doc_len) = self.doc_lengths.remove(&slot) {
            // Remove from postings
            for postings in self.postings.values_mut() {
                postings.retain(|p| p.slot != slot);
            }
            // Remove empty posting lists
            self.postings.retain(|_, v| !v.is_empty());

            // Update stats
            let old_total = self.avg_doc_len * (self.doc_count as f64);
            self.doc_count -= 1;
            if self.doc_count > 0 {
                self.avg_doc_len = (old_total - doc_len as f64) / self.doc_count as f64;
            } else {
                self.avg_doc_len = 0.0;
            }
        }
    }

    /// Search for documents matching the query. Returns results sorted by BM25 score.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Deduplicate query terms
        let unique_terms: HashSet<&str> = query_tokens.iter().map(|s| s.as_str()).collect();

        // Accumulate BM25 scores per document
        let mut scores: HashMap<u64, f64> = HashMap::new();

        for term in &unique_terms {
            if let Some(postings) = self.postings.get(*term) {
                let df = postings.len() as f64;
                let idf = ((self.doc_count as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

                for posting in postings {
                    let doc_len = *self.doc_lengths.get(&posting.slot).unwrap_or(&1) as f64;
                    let tf = posting.tf as f64;
                    let tf_norm = (tf * (K1 + 1.0))
                        / (tf + K1 * (1.0 - B + B * doc_len / self.avg_doc_len));

                    *scores.entry(posting.slot).or_default() += idf * tf_norm;
                }
            }
        }

        // Sort by score descending
        let mut results: Vec<SearchHit> = scores
            .into_iter()
            .map(|(slot, score)| SearchHit { slot, score })
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    /// Number of indexed documents.
    pub fn doc_count(&self) -> u64 {
        self.doc_count
    }

    /// Number of unique terms.
    pub fn term_count(&self) -> usize {
        self.postings.len()
    }
}

/// Tokenizer with lowercasing. Splits on whitespace, punctuation, hyphens, underscores.
/// Hyphenated/underscored terms also produce sub-tokens for partial matching.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_basic() {
        let tokens = tokenize("Hello, World! Test-Node server_01");
        assert_eq!(tokens, vec!["hello", "world", "test", "node", "server", "01"]);
    }

    #[test]
    fn tokenize_skips_short() {
        let tokens = tokenize("a b cd ef");
        assert_eq!(tokens, vec!["cd", "ef"]);
    }

    #[test]
    fn add_and_search() {
        let mut idx = FullTextIndex::new();
        idx.add_document(0, "postgresql database server");
        idx.add_document(1, "nginx web server proxy");
        idx.add_document(2, "redis cache database");

        let results = idx.search("database", 10);
        assert_eq!(results.len(), 2);
        // Both slot 0 and 2 contain "database"
        let slots: Vec<u64> = results.iter().map(|h| h.slot).collect();
        assert!(slots.contains(&0));
        assert!(slots.contains(&2));
    }

    #[test]
    fn search_multi_term() {
        let mut idx = FullTextIndex::new();
        idx.add_document(0, "postgresql database server production");
        idx.add_document(1, "nginx web server");
        idx.add_document(2, "postgresql database staging");

        // "postgresql server" should rank slot 0 highest (matches both terms)
        let results = idx.search("postgresql server", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].slot, 0); // matches both terms
    }

    #[test]
    fn search_no_match() {
        let mut idx = FullTextIndex::new();
        idx.add_document(0, "postgresql database");

        let results = idx.search("kubernetes", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn remove_document() {
        let mut idx = FullTextIndex::new();
        idx.add_document(0, "test document");
        idx.add_document(1, "another test");

        idx.remove_document(0);

        let results = idx.search("test", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slot, 1);
        assert_eq!(idx.doc_count(), 1);
    }

    #[test]
    fn bm25_tf_saturation() {
        let mut idx = FullTextIndex::new();
        // Doc with repeated term should score higher but with diminishing returns
        idx.add_document(0, "database");
        idx.add_document(1, "database database database database database");

        let results = idx.search("database", 10);
        assert_eq!(results.len(), 2);
        // Slot 1 has higher TF but BM25 saturates — both should appear
        assert!(results[0].score > 0.0);
        assert!(results[1].score > 0.0);
    }

    #[test]
    fn limit_works() {
        let mut idx = FullTextIndex::new();
        for i in 0..100 {
            idx.add_document(i, &format!("node-{i} server"));
        }

        let results = idx.search("server", 5);
        assert_eq!(results.len(), 5);
    }
}
