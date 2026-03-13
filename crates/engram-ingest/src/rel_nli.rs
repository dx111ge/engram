/// NLI-based relation extraction via external `engram-rel` subprocess.
///
/// Uses Natural Language Inference to classify relations between entity pairs.
/// The inference runs in a separate `engram-rel` binary (ONNX Runtime + tokenizers)
/// to avoid MSVC CRT linker conflicts between esaxx-rs and ort_sys.
///
/// Based on EMNLP 2021 "Label Verbalization and Entailment for Effective Zero
/// and Few-Shot Relation Extraction" (Sainz et al.) — 63% F1 zero-shot on TACRED.
///
/// Feature-gated: only compiled with `--features nli-rel`.

#[cfg(feature = "nli-rel")]
use crate::rel_traits::{CandidateRelation, RelationExtractionInput, RelationExtractor};
#[cfg(feature = "nli-rel")]
use crate::types::ExtractionMethod;

/// Default relation templates (21 types).
/// Sourced from TACRED/FewRel/Wikidata research.
pub const DEFAULT_TEMPLATES: &[(&str, &str)] = &[
    ("works_at", "{head} works at {tail}"),
    ("born_in", "{head} was born in {tail}"),
    ("lives_in", "{head} lives in {tail}"),
    ("educated_at", "{head} was educated at {tail}"),
    ("spouse", "{head} is married to {tail}"),
    ("parent_of", "{head} is the parent of {tail}"),
    ("child_of", "{head} is the child of {tail}"),
    ("citizen_of", "{head} is a citizen of {tail}"),
    ("member_of", "{head} is a member of {tail}"),
    ("holds_position", "{head} holds the position of {tail}"),
    ("founded_by", "{head} was founded by {tail}"),
    ("headquartered_in", "{head}'s headquarters are in {tail}"),
    ("subsidiary_of", "{head} is a subsidiary of {tail}"),
    ("acquired_by", "{head} was acquired by {tail}"),
    ("located_in", "{head} is located in {tail}"),
    ("instance_of", "{head} is a {tail}"),
    ("part_of", "{head} is part of {tail}"),
    ("capital_of", "{head} is the capital of {tail}"),
    ("cause_of", "{head} causes {tail}"),
    ("author_of", "{head} was written by {tail}"),
    ("produces", "{head} produces {tail}"),
];

/// Build the default relation template map.
pub fn default_templates() -> std::collections::HashMap<String, String> {
    DEFAULT_TEMPLATES
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Configuration for NLI-based relation extraction.
#[derive(Debug, Clone)]
pub struct NliRelConfig {
    /// Model directory (containing model.onnx + tokenizer.json).
    pub model_dir: std::path::PathBuf,
    /// Relation templates: rel_type -> hypothesis template with {head}/{tail} placeholders.
    pub relation_templates: std::collections::HashMap<String, String>,
    /// Minimum entailment confidence to emit a relation.
    pub min_confidence: f32,
}

impl Default for NliRelConfig {
    fn default() -> Self {
        Self {
            model_dir: std::path::PathBuf::new(),
            relation_templates: default_templates(),
            min_confidence: 0.5,
        }
    }
}

/// NLI-based relation extraction via external `engram-rel` process.
#[cfg(feature = "nli-rel")]
pub struct NliRelBackend {
    config: NliRelConfig,
    rel_binary: std::path::PathBuf,
}

#[cfg(feature = "nli-rel")]
impl NliRelBackend {
    /// Create a new NLI relation extraction backend.
    pub fn new(config: NliRelConfig) -> Result<Self, crate::IngestError> {
        let rel_binary = find_rel_binary().ok_or_else(|| {
            crate::IngestError::Config(
                "engram-rel binary not found. Build with: cargo build -p engram-rel (in tools/engram-rel/)"
                    .into(),
            )
        })?;

        if !config.model_dir.exists() {
            return Err(crate::IngestError::Config(format!(
                "NLI model directory not found: {}. Download with: engram model install rel <model-id>",
                config.model_dir.display()
            )));
        }

        tracing::info!(
            model = %config.model_dir.display(),
            templates = config.relation_templates.len(),
            binary = %rel_binary.display(),
            "NLI relation extraction backend ready (subprocess)"
        );

        Ok(Self {
            config,
            rel_binary,
        })
    }

    /// Number of relation templates configured.
    pub fn template_count(&self) -> usize {
        self.config.relation_templates.len()
    }

    fn run_extraction(
        &self,
        input: &RelationExtractionInput,
    ) -> Result<Vec<CandidateRelation>, crate::IngestError> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let entities: Vec<serde_json::Value> = input
            .entities
            .iter()
            .map(|e| {
                serde_json::json!({
                    "text": e.text,
                    "label": e.entity_type,
                    "start": e.span.0,
                    "end": e.span.1,
                })
            })
            .collect();

        let request = serde_json::json!({
            "model_dir": self.config.model_dir.to_string_lossy(),
            "text": input.text,
            "entities": entities,
            "relation_templates": self.config.relation_templates,
            "threshold": self.config.min_confidence,
        });

        let mut child = Command::new(&self.rel_binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| crate::IngestError::Ner(format!("failed to spawn engram-rel: {e}")))?;

        {
            let stdin = child.stdin.as_mut().unwrap();
            serde_json::to_writer(&mut *stdin, &request)
                .map_err(|e| crate::IngestError::Ner(format!("write to engram-rel: {e}")))?;
            writeln!(stdin)
                .map_err(|e| crate::IngestError::Ner(format!("write to engram-rel: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| crate::IngestError::Ner(format!("engram-rel failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::IngestError::Ner(format!(
                "engram-rel exited with {}: {}",
                output.status, stderr
            )));
        }

        // Parse JSON Lines response
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut relations = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(line) {
                if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                    if let Some(rels) = resp.get("relations").and_then(|v| v.as_array()) {
                        for rel in rels {
                            let head_text =
                                rel.get("head").and_then(|v| v.as_str()).unwrap_or("");
                            let tail_text =
                                rel.get("tail").and_then(|v| v.as_str()).unwrap_or("");
                            let label =
                                rel.get("label").and_then(|v| v.as_str()).unwrap_or("");
                            let score = rel
                                .get("score")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;

                            // Find matching entity indices
                            let head_idx = input
                                .entities
                                .iter()
                                .position(|e| e.text == head_text);
                            let tail_idx = input
                                .entities
                                .iter()
                                .position(|e| e.text == tail_text);

                            if let (Some(h), Some(t)) = (head_idx, tail_idx) {
                                relations.push(CandidateRelation {
                                    head_idx: h,
                                    tail_idx: t,
                                    rel_type: label.to_string(),
                                    confidence: score,
                                    method: ExtractionMethod::StatisticalModel,
                                });
                            }
                        }
                    }
                } else if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
                    tracing::warn!("engram-rel error: {err}");
                }
            }
        }

        Ok(relations)
    }
}

#[cfg(feature = "nli-rel")]
impl RelationExtractor for NliRelBackend {
    fn extract_relations(&self, input: &RelationExtractionInput) -> Vec<CandidateRelation> {
        if input.entities.len() < 2 || self.config.relation_templates.is_empty() {
            return Vec::new();
        }

        match self.run_extraction(input) {
            Ok(rels) => {
                if !rels.is_empty() {
                    tracing::debug!(
                        count = rels.len(),
                        "NLI relation extraction complete"
                    );
                }
                rels
            }
            Err(e) => {
                tracing::warn!("NLI relation extraction failed: {e}");
                Vec::new()
            }
        }
    }

    fn name(&self) -> &str {
        "nli-rel"
    }
}

// ── Binary discovery ──

/// Find the `engram-rel` binary: next to exe, ~/.engram/bin/, or on PATH.
#[cfg(feature = "nli-rel")]
fn find_rel_binary() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let name = "engram-rel.exe";
    #[cfg(not(target_os = "windows"))]
    let name = "engram-rel";

    // 1. Next to the current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
    }

    // 2. Check ~/.engram/bin/
    if let Some(home) = crate::engram_home() {
        let path = home.join("bin").join(name);
        if path.exists() {
            return Some(path);
        }
    }

    // 3. Check PATH via `where` (Windows) or `which` (Unix)
    #[cfg(target_os = "windows")]
    let which_cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let which_cmd = "which";

    if let Ok(output) = std::process::Command::new(which_cmd).arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                return Some(std::path::PathBuf::from(path));
            }
        }
    }

    None
}

// ── Model discovery ──

/// Find an NLI relation model by name in the standard model directory.
/// Expects `~/.engram/models/rel/<name>/model.onnx` + `tokenizer.json`.
pub fn find_nli_model(model_name: &str) -> Option<NliRelConfig> {
    let home = crate::engram_home()?;
    let model_dir = home.join("models").join("rel").join(model_name);

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    if model_path.exists() && tokenizer_path.exists() {
        Some(NliRelConfig {
            model_dir,
            ..Default::default()
        })
    } else {
        None
    }
}

/// List installed NLI relation models.
pub fn list_installed_nli_models() -> Vec<String> {
    let home = match crate::engram_home() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let models_dir = home.join("models").join("rel");
    if !models_dir.exists() {
        return Vec::new();
    }

    std::fs::read_dir(&models_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.join("model.onnx").exists() && path.join("tokenizer.json").exists() {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_templates_valid() {
        let templates = default_templates();
        assert_eq!(templates.len(), 21);
        for (rel_type, template) in &templates {
            assert!(!rel_type.is_empty(), "empty relation type");
            assert!(template.contains("{head}"), "{rel_type} missing {{head}}");
            assert!(template.contains("{tail}"), "{rel_type} missing {{tail}}");
        }
    }

    #[test]
    fn test_nli_config_defaults() {
        let config = NliRelConfig::default();
        assert_eq!(config.relation_templates.len(), 21);
        assert!((config.min_confidence - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_template_expansion() {
        let template = "{head} works at {tail}";
        let hypothesis = template
            .replace("{head}", "John")
            .replace("{tail}", "Google");
        assert_eq!(hypothesis, "John works at Google");
    }

    #[test]
    fn test_default_templates_all_have_placeholders() {
        for (rel_type, template) in DEFAULT_TEMPLATES {
            assert!(
                template.contains("{head}") && template.contains("{tail}"),
                "template '{}' missing placeholders: {}",
                rel_type,
                template
            );
        }
    }

    #[test]
    fn test_find_nli_model_nonexistent() {
        let result = find_nli_model("nonexistent_model_xyz_123");
        assert!(result.is_none());
    }

    #[test]
    fn test_list_installed_nli_models_runs() {
        // Should not panic, may return empty
        let models = list_installed_nli_models();
        assert!(models.len() < 1000); // sanity check
    }

    #[test]
    fn test_nli_config_custom_templates() {
        let mut config = NliRelConfig::default();
        config.relation_templates.insert("custom_rel".into(), "{head} is related to {tail}".into());
        assert_eq!(config.relation_templates.len(), 22); // 21 defaults + 1 custom
    }

    #[test]
    fn test_nli_config_min_confidence_range() {
        let config = NliRelConfig::default();
        assert!(config.min_confidence > 0.0 && config.min_confidence <= 1.0);
    }
}
