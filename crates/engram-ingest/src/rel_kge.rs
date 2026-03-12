/// KGE (Knowledge Graph Embedding) relation prediction via RotatE.
///
/// Pure Rust implementation — no ONNX dependency.
/// RotatE models entities and relations as complex vectors where
/// relations are rotations in complex space: `h * r = t`.
///
/// Scoring: `||h ⊙ r - t||` where `⊙` is element-wise complex multiply
/// and `r` has unit modulus (rotation).
///
/// Training: margin-based loss with negative sampling.
/// Persistence: `.brain.kge` binary sidecar.

use std::collections::HashMap;
use std::io::{Read as IoRead, Write as IoWrite};
use std::path::{Path, PathBuf};

use crate::rel_traits::{CandidateRelation, RelationExtractionInput, RelationExtractor};
use crate::types::ExtractionMethod;

/// KGE model configuration.
#[derive(Debug, Clone)]
pub struct KgeConfig {
    /// Embedding dimension (complex = 2 * dim floats per entity).
    pub dim: usize,
    /// Learning rate for SGD.
    pub learning_rate: f32,
    /// Number of negative samples per positive triple.
    pub neg_samples: usize,
    /// Margin for margin-based loss.
    pub gamma: f32,
    /// Maximum confidence for KGE predictions (cap).
    pub max_prediction_confidence: f32,
}

impl Default for KgeConfig {
    fn default() -> Self {
        Self {
            dim: 128,
            learning_rate: 0.01,
            neg_samples: 5,
            gamma: 6.0,
            max_prediction_confidence: 0.6,
        }
    }
}

/// KGE training statistics.
#[derive(Debug, Clone)]
pub struct KgeTrainStats {
    pub epochs_completed: u64,
    pub final_loss: f32,
    pub entity_count: u32,
    pub relation_type_count: u32,
}

/// A RotatE knowledge graph embedding model.
pub struct KgeModel {
    config: KgeConfig,
    /// Entity embeddings: entity_label -> Vec<f32> (length = 2 * dim, interleaved real/imag).
    entity_embeddings: HashMap<String, Vec<f32>>,
    /// Relation embeddings: rel_type -> Vec<f32> (length = dim, phase angles).
    relation_embeddings: HashMap<String, Vec<f32>>,
    /// Persistence path.
    path: PathBuf,
    /// Whether the model has been trained.
    trained: bool,
}

impl KgeModel {
    /// Create a new untrained model.
    pub fn new(brain_path: &Path, config: KgeConfig) -> Self {
        let path = brain_path.with_extension("kge");
        Self {
            config,
            entity_embeddings: HashMap::new(),
            relation_embeddings: HashMap::new(),
            path,
            trained: false,
        }
    }

    /// Whether the model has been trained.
    pub fn is_trained(&self) -> bool {
        self.trained
    }

    /// Number of entity embeddings.
    pub fn entity_count(&self) -> usize {
        self.entity_embeddings.len()
    }

    /// Number of relation type embeddings.
    pub fn relation_type_count(&self) -> usize {
        self.relation_embeddings.len()
    }

    /// Train on all edges in the graph.
    pub fn train_full(
        &mut self,
        graph: &engram_core::graph::Graph,
        epochs: u32,
    ) -> Result<KgeTrainStats, crate::IngestError> {
        let start = std::time::Instant::now();

        // Collect all triples from graph
        let nodes = graph
            .all_nodes()
            .map_err(|e| crate::IngestError::Graph(e.to_string()))?;

        let mut triples: Vec<(String, String, String)> = Vec::new();
        let mut entity_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut rel_type_set: std::collections::HashSet<String> = std::collections::HashSet::new();

        for node in &nodes {
            entity_set.insert(node.label.clone());
            let edges = match graph.edges_from(&node.label) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for edge in edges {
                entity_set.insert(edge.to.clone());
                rel_type_set.insert(edge.relationship.clone());
                triples.push((node.label.clone(), edge.relationship.clone(), edge.to.clone()));
            }
        }

        if triples.is_empty() {
            return Ok(KgeTrainStats {
                epochs_completed: 0,
                final_loss: 0.0,
                entity_count: 0,
                relation_type_count: 0,
            });
        }

        // Initialize embeddings for new entities/relations
        let dim = self.config.dim;
        for entity in &entity_set {
            self.entity_embeddings
                .entry(entity.clone())
                .or_insert_with(|| Self::random_entity_embedding(dim));
        }
        for rel in &rel_type_set {
            self.relation_embeddings
                .entry(rel.clone())
                .or_insert_with(|| Self::random_relation_embedding(dim));
        }

        let entity_list: Vec<String> = entity_set.into_iter().collect();
        let mut final_loss = 0.0f32;

        // Simple PRNG for negative sampling (deterministic, no external dep)
        let mut rng_state: u64 = 42 ^ (triples.len() as u64);

        for epoch in 0..epochs {
            let mut epoch_loss = 0.0f32;

            for (head, rel, tail) in &triples {
                // Positive score
                let pos_score = self.score_triple(head, rel, tail);

                // Negative samples: corrupt head or tail
                for _ in 0..self.config.neg_samples {
                    rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let corrupt_head = (rng_state >> 33) as usize % 2 == 0;
                    let random_idx =
                        (rng_state.wrapping_mul(2862933555777941757).wrapping_add(3) >> 33)
                            as usize
                            % entity_list.len();
                    let random_entity = &entity_list[random_idx];

                    let neg_score = if corrupt_head {
                        self.score_triple(random_entity, rel, tail)
                    } else {
                        self.score_triple(head, rel, random_entity)
                    };

                    // Margin-based loss: max(0, gamma + pos_score - neg_score)
                    let loss = (self.config.gamma + pos_score - neg_score).max(0.0);
                    epoch_loss += loss;

                    if loss > 0.0 {
                        // Gradient update
                        let lr = self.config.learning_rate;

                        if corrupt_head {
                            self.update_embeddings(head, rel, tail, lr);
                            self.update_embeddings_neg(random_entity, rel, tail, lr);
                        } else {
                            self.update_embeddings(head, rel, tail, lr);
                            self.update_embeddings_neg(head, rel, random_entity, lr);
                        }
                    }
                }
            }

            epoch_loss /= (triples.len() * self.config.neg_samples) as f32;
            final_loss = epoch_loss;

            if epoch % 10 == 0 {
                tracing::debug!(
                    epoch = epoch,
                    loss = format!("{:.4}", epoch_loss),
                    "KGE training progress"
                );
            }
        }

        self.trained = true;

        // Normalize relation embeddings to unit modulus
        for emb in self.relation_embeddings.values_mut() {
            for val in emb.iter_mut() {
                // Wrap phase to [-pi, pi]
                *val = val.rem_euclid(2.0 * std::f32::consts::PI) - std::f32::consts::PI;
            }
        }

        tracing::info!(
            epochs = epochs,
            loss = format!("{:.4}", final_loss),
            entities = self.entity_embeddings.len(),
            rel_types = self.relation_embeddings.len(),
            duration_ms = start.elapsed().as_millis(),
            "KGE training complete"
        );

        Ok(KgeTrainStats {
            epochs_completed: epochs as u64,
            final_loss,
            entity_count: self.entity_embeddings.len() as u32,
            relation_type_count: self.relation_embeddings.len() as u32,
        })
    }

    /// Predict relations between two entities.
    /// Returns scored (rel_type, confidence) pairs.
    pub fn predict(
        &self,
        head: &str,
        tail: &str,
        min_confidence: f32,
    ) -> Vec<(String, f32)> {
        if !self.trained {
            return Vec::new();
        }

        let head_emb = match self.entity_embeddings.get(head) {
            Some(e) => e,
            None => return Vec::new(),
        };
        let tail_emb = match self.entity_embeddings.get(tail) {
            Some(e) => e,
            None => return Vec::new(),
        };

        let mut predictions: Vec<(String, f32)> = Vec::new();

        for (rel_type, rel_emb) in &self.relation_embeddings {
            let score = self.compute_score(head_emb, rel_emb, tail_emb);
            // Convert distance to confidence via sigmoid: conf = 1 / (1 + exp(score - gamma))
            let confidence = 1.0 / (1.0 + (score - self.config.gamma).exp());
            let confidence = confidence.min(self.config.max_prediction_confidence);

            if confidence >= min_confidence {
                predictions.push((rel_type.clone(), confidence));
            }
        }

        predictions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        predictions
    }

    /// Score a single triple (lower = better fit).
    fn score_triple(&self, head: &str, rel: &str, tail: &str) -> f32 {
        let head_emb = match self.entity_embeddings.get(head) {
            Some(e) => e,
            None => return self.config.gamma * 2.0,
        };
        let rel_emb = match self.relation_embeddings.get(rel) {
            Some(e) => e,
            None => return self.config.gamma * 2.0,
        };
        let tail_emb = match self.entity_embeddings.get(tail) {
            Some(e) => e,
            None => return self.config.gamma * 2.0,
        };

        self.compute_score(head_emb, rel_emb, tail_emb)
    }

    /// Compute RotatE distance: ||h ⊙ r - t||
    fn compute_score(&self, head: &[f32], rel_phase: &[f32], tail: &[f32]) -> f32 {
        let dim = self.config.dim;
        let mut dist = 0.0f32;

        for i in 0..dim {
            let h_re = head[2 * i];
            let h_im = head[2 * i + 1];
            let r_re = rel_phase[i].cos();
            let r_im = rel_phase[i].sin();
            let t_re = tail[2 * i];
            let t_im = tail[2 * i + 1];

            // Complex multiply: (h_re + h_im*i) * (r_re + r_im*i)
            let hr_re = h_re * r_re - h_im * r_im;
            let hr_im = h_re * r_im + h_im * r_re;

            // Distance from (h*r) to t
            let d_re = hr_re - t_re;
            let d_im = hr_im - t_im;
            dist += (d_re * d_re + d_im * d_im).sqrt();
        }

        dist
    }

    /// Update embeddings for a positive triple (move closer).
    fn update_embeddings(&mut self, head: &str, rel: &str, tail: &str, lr: f32) {
        let dim = self.config.dim;
        let head_emb = match self.entity_embeddings.get(head) {
            Some(e) => e.clone(),
            None => return,
        };
        let rel_emb = match self.relation_embeddings.get(rel) {
            Some(e) => e.clone(),
            None => return,
        };
        let tail_emb = match self.entity_embeddings.get(tail) {
            Some(e) => e.clone(),
            None => return,
        };

        let mut h_grad = vec![0.0f32; 2 * dim];
        let mut t_grad = vec![0.0f32; 2 * dim];
        let mut r_grad = vec![0.0f32; dim];

        for i in 0..dim {
            let h_re = head_emb[2 * i];
            let h_im = head_emb[2 * i + 1];
            let r_re = rel_emb[i].cos();
            let r_im = rel_emb[i].sin();
            let t_re = tail_emb[2 * i];
            let t_im = tail_emb[2 * i + 1];

            let hr_re = h_re * r_re - h_im * r_im;
            let hr_im = h_re * r_im + h_im * r_re;

            let d_re = hr_re - t_re;
            let d_im = hr_im - t_im;
            let norm = (d_re * d_re + d_im * d_im).sqrt().max(1e-8);

            let g_re = d_re / norm;
            let g_im = d_im / norm;

            // Head gradients
            h_grad[2 * i] = g_re * r_re + g_im * r_im;
            h_grad[2 * i + 1] = -g_re * r_im + g_im * r_re;

            // Tail gradients (negative)
            t_grad[2 * i] = -g_re;
            t_grad[2 * i + 1] = -g_im;

            // Relation phase gradient
            r_grad[i] = g_re * (-h_re * r_im - h_im * r_re) + g_im * (h_re * r_re - h_im * r_im);
        }

        // Apply gradients
        if let Some(h) = self.entity_embeddings.get_mut(head) {
            for i in 0..2 * dim {
                h[i] -= lr * h_grad[i];
            }
        }
        if let Some(t) = self.entity_embeddings.get_mut(tail) {
            for i in 0..2 * dim {
                t[i] -= lr * t_grad[i];
            }
        }
        if let Some(r) = self.relation_embeddings.get_mut(rel) {
            for i in 0..dim {
                r[i] -= lr * r_grad[i];
            }
        }
    }

    /// Update embeddings for a negative triple (move apart).
    fn update_embeddings_neg(&mut self, head: &str, rel: &str, tail: &str, lr: f32) {
        // Same as positive but with opposite sign
        self.update_embeddings(head, rel, tail, -lr);
    }

    /// Random entity embedding (2 * dim floats).
    fn random_entity_embedding(dim: usize) -> Vec<f32> {
        let mut emb = vec![0.0f32; 2 * dim];
        // Simple deterministic initialization using index-based seed
        let mut state: u64 = 0xDEADBEEF;
        for val in &mut emb {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *val = ((state >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0;
            *val *= 0.1; // small initial values
        }
        // Add some variation
        for (i, val) in emb.iter_mut().enumerate() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(i as u64);
            *val += ((state >> 33) as f32 / u32::MAX as f32 - 0.5) * 0.05;
        }
        emb
    }

    /// Random relation embedding (dim phase angles).
    fn random_relation_embedding(dim: usize) -> Vec<f32> {
        let mut emb = vec![0.0f32; dim];
        let mut state: u64 = 0xCAFEBABE;
        for val in &mut emb {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            // Phase in [-pi, pi]
            *val = ((state >> 33) as f32 / u32::MAX as f32) * 2.0 * std::f32::consts::PI
                - std::f32::consts::PI;
        }
        emb
    }

    /// Save model to `.brain.kge` binary file.
    pub fn save(&self) -> Result<(), crate::IngestError> {
        let mut file = std::fs::File::create(&self.path).map_err(crate::IngestError::Io)?;

        // Header: magic + version + dim + entity_count + rel_count + trained flag
        file.write_all(b"EKGE").map_err(crate::IngestError::Io)?;
        file.write_all(&1u32.to_le_bytes())
            .map_err(crate::IngestError::Io)?; // version
        file.write_all(&(self.config.dim as u32).to_le_bytes())
            .map_err(crate::IngestError::Io)?;
        file.write_all(&(self.entity_embeddings.len() as u32).to_le_bytes())
            .map_err(crate::IngestError::Io)?;
        file.write_all(&(self.relation_embeddings.len() as u32).to_le_bytes())
            .map_err(crate::IngestError::Io)?;
        file.write_all(&[if self.trained { 1u8 } else { 0u8 }])
            .map_err(crate::IngestError::Io)?;

        // Entity embeddings
        for (name, emb) in &self.entity_embeddings {
            let name_bytes = name.as_bytes();
            file.write_all(&(name_bytes.len() as u32).to_le_bytes())
                .map_err(crate::IngestError::Io)?;
            file.write_all(name_bytes)
                .map_err(crate::IngestError::Io)?;
            for &val in emb {
                file.write_all(&val.to_le_bytes())
                    .map_err(crate::IngestError::Io)?;
            }
        }

        // Relation embeddings
        for (name, emb) in &self.relation_embeddings {
            let name_bytes = name.as_bytes();
            file.write_all(&(name_bytes.len() as u32).to_le_bytes())
                .map_err(crate::IngestError::Io)?;
            file.write_all(name_bytes)
                .map_err(crate::IngestError::Io)?;
            for &val in emb {
                file.write_all(&val.to_le_bytes())
                    .map_err(crate::IngestError::Io)?;
            }
        }

        file.flush().map_err(crate::IngestError::Io)?;
        tracing::debug!(
            entities = self.entity_embeddings.len(),
            rel_types = self.relation_embeddings.len(),
            path = %self.path.display(),
            "KGE model saved"
        );
        Ok(())
    }

    /// Load model from `.brain.kge` binary file.
    pub fn load(brain_path: &Path, config: KgeConfig) -> Result<Self, crate::IngestError> {
        let path = brain_path.with_extension("kge");
        let mut model = Self {
            config: config.clone(),
            entity_embeddings: HashMap::new(),
            relation_embeddings: HashMap::new(),
            path: path.clone(),
            trained: false,
        };

        if !path.exists() {
            return Ok(model);
        }

        let mut file = std::fs::File::open(&path).map_err(crate::IngestError::Io)?;

        // Read header
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)
            .map_err(crate::IngestError::Io)?;
        if &magic != b"EKGE" {
            return Err(crate::IngestError::Config(
                "invalid KGE file magic".into(),
            ));
        }

        let mut buf4 = [0u8; 4];
        file.read_exact(&mut buf4)
            .map_err(crate::IngestError::Io)?;
        let _version = u32::from_le_bytes(buf4);

        file.read_exact(&mut buf4)
            .map_err(crate::IngestError::Io)?;
        let dim = u32::from_le_bytes(buf4) as usize;
        model.config.dim = dim;

        file.read_exact(&mut buf4)
            .map_err(crate::IngestError::Io)?;
        let entity_count = u32::from_le_bytes(buf4) as usize;

        file.read_exact(&mut buf4)
            .map_err(crate::IngestError::Io)?;
        let rel_count = u32::from_le_bytes(buf4) as usize;

        let mut trained_flag = [0u8; 1];
        file.read_exact(&mut trained_flag)
            .map_err(crate::IngestError::Io)?;
        model.trained = trained_flag[0] == 1;

        // Read entity embeddings
        for _ in 0..entity_count {
            file.read_exact(&mut buf4)
                .map_err(crate::IngestError::Io)?;
            let name_len = u32::from_le_bytes(buf4) as usize;
            let mut name_buf = vec![0u8; name_len];
            file.read_exact(&mut name_buf)
                .map_err(crate::IngestError::Io)?;
            let name = String::from_utf8(name_buf)
                .map_err(|e| crate::IngestError::Config(e.to_string()))?;

            let emb_len = 2 * dim;
            let mut emb = vec![0.0f32; emb_len];
            for val in &mut emb {
                file.read_exact(&mut buf4)
                    .map_err(crate::IngestError::Io)?;
                *val = f32::from_le_bytes(buf4);
            }

            model.entity_embeddings.insert(name, emb);
        }

        // Read relation embeddings
        for _ in 0..rel_count {
            file.read_exact(&mut buf4)
                .map_err(crate::IngestError::Io)?;
            let name_len = u32::from_le_bytes(buf4) as usize;
            let mut name_buf = vec![0u8; name_len];
            file.read_exact(&mut name_buf)
                .map_err(crate::IngestError::Io)?;
            let name = String::from_utf8(name_buf)
                .map_err(|e| crate::IngestError::Config(e.to_string()))?;

            let mut emb = vec![0.0f32; dim];
            for val in &mut emb {
                file.read_exact(&mut buf4)
                    .map_err(crate::IngestError::Io)?;
                *val = f32::from_le_bytes(buf4);
            }

            model.relation_embeddings.insert(name, emb);
        }

        tracing::info!(
            entities = model.entity_embeddings.len(),
            rel_types = model.relation_embeddings.len(),
            trained = model.trained,
            path = %path.display(),
            "KGE model loaded"
        );

        Ok(model)
    }
}

/// KGE as a `RelationExtractor` backend.
///
/// Only predicts for entities that have embeddings (i.e., resolved entities
/// that were present during training).
pub struct KgeRelationExtractor {
    model: std::sync::Arc<std::sync::RwLock<KgeModel>>,
}

impl KgeRelationExtractor {
    pub fn new(model: std::sync::Arc<std::sync::RwLock<KgeModel>>) -> Self {
        Self { model }
    }
}

impl RelationExtractor for KgeRelationExtractor {
    fn extract_relations(&self, input: &RelationExtractionInput) -> Vec<CandidateRelation> {
        let model = match self.model.read() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        if !model.is_trained() {
            return Vec::new();
        }

        let mut relations = Vec::new();

        for i in 0..input.entities.len() {
            for j in (i + 1)..input.entities.len() {
                // Try both directions
                let predictions_forward =
                    model.predict(&input.entities[i].text, &input.entities[j].text, 0.1);
                let predictions_reverse =
                    model.predict(&input.entities[j].text, &input.entities[i].text, 0.1);

                for (rel_type, confidence) in predictions_forward {
                    relations.push(CandidateRelation {
                        head_idx: i,
                        tail_idx: j,
                        rel_type,
                        confidence,
                        method: ExtractionMethod::LearnedPattern,
                    });
                }

                for (rel_type, confidence) in predictions_reverse {
                    relations.push(CandidateRelation {
                        head_idx: j,
                        tail_idx: i,
                        rel_type,
                        confidence,
                        method: ExtractionMethod::LearnedPattern,
                    });
                }
            }
        }

        relations
    }

    fn name(&self) -> &str {
        "kge-rotate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_model_is_untrained() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let model = KgeModel::new(&path, KgeConfig::default());
        assert!(!model.is_trained());
        assert_eq!(model.entity_count(), 0);
        assert_eq!(model.relation_type_count(), 0);
    }

    #[test]
    fn untrained_model_predicts_nothing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let model = KgeModel::new(&path, KgeConfig::default());
        assert!(model.predict("Alice", "Bob", 0.0).is_empty());
    }

    #[test]
    fn train_and_predict() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        // Create a small graph
        let mut graph = engram_core::graph::Graph::create(&path).unwrap();
        let prov = engram_core::graph::Provenance::user("test");
        graph.store("Alice", &prov).unwrap();
        graph.store("Acme Corp", &prov).unwrap();
        graph.store("Bob", &prov).unwrap();
        graph.relate("Alice", "Acme Corp", "works_at", &prov).unwrap();
        graph.relate("Bob", "Acme Corp", "works_at", &prov).unwrap();

        let mut model = KgeModel::new(&path, KgeConfig {
            dim: 16,
            learning_rate: 0.05,
            neg_samples: 3,
            gamma: 4.0,
            max_prediction_confidence: 0.6,
        });

        let stats = model.train_full(&graph, 50).unwrap();
        assert!(model.is_trained());
        assert_eq!(stats.entity_count, 3);
        assert_eq!(stats.relation_type_count, 1);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let mut graph = engram_core::graph::Graph::create(&path).unwrap();
        let prov = engram_core::graph::Provenance::user("test");
        graph.store("Alice", &prov).unwrap();
        graph.store("Bob", &prov).unwrap();
        graph.relate("Alice", "Bob", "knows", &prov).unwrap();

        let config = KgeConfig {
            dim: 8,
            ..Default::default()
        };
        let mut model = KgeModel::new(&path, config.clone());
        model.train_full(&graph, 10).unwrap();
        model.save().unwrap();

        let loaded = KgeModel::load(&path, config).unwrap();
        assert!(loaded.is_trained());
        assert_eq!(loaded.entity_count(), model.entity_count());
        assert_eq!(loaded.relation_type_count(), model.relation_type_count());
    }

    #[test]
    fn kge_extractor_returns_empty_when_untrained() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let model = KgeModel::new(&path, KgeConfig::default());
        let model = std::sync::Arc::new(std::sync::RwLock::new(model));
        let extractor = KgeRelationExtractor::new(model);

        let input = RelationExtractionInput {
            text: "test".into(),
            entities: vec![
                crate::types::ExtractedEntity {
                    text: "Alice".into(),
                    entity_type: "PERSON".into(),
                    span: (0, 5),
                    confidence: 0.9,
                    method: ExtractionMethod::Gazetteer,
                    language: "en".into(),
                    resolved_to: Some(1),
                },
                crate::types::ExtractedEntity {
                    text: "Bob".into(),
                    entity_type: "PERSON".into(),
                    span: (10, 13),
                    confidence: 0.9,
                    method: ExtractionMethod::Gazetteer,
                    language: "en".into(),
                    resolved_to: Some(2),
                },
            ],
            language: "en".into(),
        };

        assert!(extractor.extract_relations(&input).is_empty());
    }
}
