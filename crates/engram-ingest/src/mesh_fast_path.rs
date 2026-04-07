/// Mesh fast path: specialized pipeline config for mesh-received data.
///
/// Data from trusted peers skips NER (entities are pre-extracted),
/// resolves locally via gazetteer only, and applies a peer trust multiplier
/// to confidence scores.

use crate::types::{PipelineConfig, StageConfig};

/// Mesh fast path configuration.
#[derive(Debug, Clone)]
pub struct MeshFastPathConfig {
    /// Trust multiplier for peer-provided data [0.0, 1.0].
    /// Applied as: confidence = peer_confidence * peer_trust_multiplier.
    pub peer_trust_multiplier: f32,
    /// Whether to skip NER entirely (peer already extracted entities).
    pub skip_ner: bool,
    /// Whether to skip dedup (peer already deduped).
    pub skip_dedup: bool,
    /// Whether to run conflict detection against local graph.
    pub check_conflicts: bool,
}

impl Default for MeshFastPathConfig {
    fn default() -> Self {
        Self {
            peer_trust_multiplier: 0.70,
            skip_ner: true,
            skip_dedup: true,
            check_conflicts: true,
        }
    }
}

/// Build a PipelineConfig for mesh fast path ingestion.
pub fn mesh_pipeline_config(mesh_config: &MeshFastPathConfig) -> PipelineConfig {
    PipelineConfig {
        name: "mesh-fast-path".into(),
        stages: StageConfig {
            parse: false,        // mesh data is pre-parsed
            language_detect: false, // language already known
            ner: !mesh_config.skip_ner,
            entity_resolve: true, // always resolve locally
            dedup: !mesh_config.skip_dedup,
            conflict_check: mesh_config.check_conflicts,
            confidence_calc: false, // we apply peer trust multiplier instead
            relation_extract: false, // mesh sync uses fast path, skip RE
            fact_extract: false, // mesh sync doesn't need LLM facts
            translate: false, // mesh data is already in target language
        },
        ..Default::default()
    }
}

/// Apply peer trust multiplier to a confidence value.
pub fn apply_peer_trust(confidence: f32, multiplier: f32) -> f32 {
    (confidence * multiplier).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mesh_config() {
        let config = MeshFastPathConfig::default();
        assert_eq!(config.peer_trust_multiplier, 0.70);
        assert!(config.skip_ner);
        assert!(config.skip_dedup);
        assert!(config.check_conflicts);
    }

    #[test]
    fn mesh_pipeline_skips_ner() {
        let config = MeshFastPathConfig::default();
        let pipeline_config = mesh_pipeline_config(&config);
        assert!(!pipeline_config.stages.ner);
        assert!(!pipeline_config.stages.parse);
        assert!(!pipeline_config.stages.language_detect);
        assert!(pipeline_config.stages.entity_resolve);
        assert!(!pipeline_config.stages.dedup);
        assert!(pipeline_config.stages.conflict_check);
    }

    #[test]
    fn peer_trust_multiplier_applied() {
        assert!((apply_peer_trust(0.9, 0.7) - 0.63).abs() < 0.001);
        assert!((apply_peer_trust(1.0, 0.5) - 0.50).abs() < 0.001);
        assert_eq!(apply_peer_trust(0.8, 1.5), 1.0); // clamped
        assert_eq!(apply_peer_trust(0.5, 0.0), 0.0);
    }

    #[test]
    fn mesh_config_with_ner_enabled() {
        let config = MeshFastPathConfig {
            skip_ner: false,
            ..Default::default()
        };
        let pipeline_config = mesh_pipeline_config(&config);
        assert!(pipeline_config.stages.ner);
    }
}
