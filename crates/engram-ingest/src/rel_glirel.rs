/// GLiREL relation extraction via external `engram-rel` subprocess.
///
/// Mirrors the `anno_backend.rs` pattern: communicates with an external binary
/// via JSON Lines over stdin/stdout. The model weights are user-downloaded
/// (CC BY-NC-SA 4.0), never bundled.
///
/// Feature-gated: only compiled with `--features glirel`.

#[cfg(feature = "glirel")]
use crate::rel_traits::{CandidateRelation, RelationExtractionInput, RelationExtractor};
#[cfg(feature = "glirel")]
use crate::types::ExtractionMethod;

/// Configuration for the GLiREL backend.
#[derive(Debug, Clone)]
pub struct GlirelConfig {
    /// Path to the model directory (containing model.onnx and tokenizer.json).
    pub model_dir: std::path::PathBuf,
    /// Candidate relation labels for zero-shot classification.
    pub candidate_labels: Vec<String>,
    /// Minimum confidence threshold.
    pub min_confidence: f32,
}

impl Default for GlirelConfig {
    fn default() -> Self {
        Self {
            model_dir: std::path::PathBuf::new(),
            candidate_labels: vec![
                "works_at".into(),
                "located_in".into(),
                "founded_by".into(),
                "part_of".into(),
                "ceo_of".into(),
                "member_of".into(),
                "born_in".into(),
                "acquired_by".into(),
            ],
            min_confidence: 0.3,
        }
    }
}

/// GLiREL backend via external `engram-rel` process.
#[cfg(feature = "glirel")]
pub struct GlirelBackend {
    config: GlirelConfig,
    rel_binary: std::path::PathBuf,
}

#[cfg(feature = "glirel")]
impl GlirelBackend {
    pub fn new(config: GlirelConfig) -> Result<Self, crate::IngestError> {
        let rel_binary = find_rel_binary().ok_or_else(|| {
            crate::IngestError::Config(
                "engram-rel binary not found. It should be in the same directory as engram."
                    .into(),
            )
        })?;

        if !config.model_dir.exists() {
            return Err(crate::IngestError::Config(format!(
                "GLiREL model directory not found: {}",
                config.model_dir.display()
            )));
        }

        Ok(Self {
            config,
            rel_binary,
        })
    }

    fn run_extraction(
        &self,
        input: &RelationExtractionInput,
    ) -> Result<Vec<CandidateRelation>, crate::IngestError> {
        use std::io::{BufRead, Write};
        use std::process::{Command, Stdio};

        // Build entity spans for the request
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
            "candidate_labels": self.config.candidate_labels,
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
            serde_json::to_writer(stdin, &request)
                .map_err(|e| crate::IngestError::Ner(format!("write to engram-rel: {e}")))?;
            writeln!(stdin).map_err(|e| crate::IngestError::Ner(format!("write to engram-rel: {e}")))?;
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

        // Parse JSON lines response
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
                            let head_text = rel.get("head").and_then(|v| v.as_str()).unwrap_or("");
                            let tail_text = rel.get("tail").and_then(|v| v.as_str()).unwrap_or("");
                            let label = rel.get("label").and_then(|v| v.as_str()).unwrap_or("");
                            let score =
                                rel.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

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
                }
            }
        }

        Ok(relations)
    }
}

#[cfg(feature = "glirel")]
impl RelationExtractor for GlirelBackend {
    fn extract_relations(&self, input: &RelationExtractionInput) -> Vec<CandidateRelation> {
        match self.run_extraction(input) {
            Ok(rels) => rels,
            Err(e) => {
                tracing::warn!("GLiREL extraction failed: {e}");
                Vec::new()
            }
        }
    }

    fn name(&self) -> &str {
        "glirel"
    }
}

/// Find the `engram-rel` binary next to the current executable.
#[cfg(feature = "glirel")]
fn find_rel_binary() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;

    #[cfg(target_os = "windows")]
    let name = "engram-rel.exe";
    #[cfg(not(target_os = "windows"))]
    let name = "engram-rel";

    let path = dir.join(name);
    if path.exists() {
        return Some(path);
    }

    // Check PATH
    if let Ok(output) = std::process::Command::new("which")
        .arg(name)
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(std::path::PathBuf::from(path));
            }
        }
    }

    None
}

/// Get the user's home directory.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// Find a GLiREL model by ID in the standard model directory.
pub fn find_rel_model(model_id: &str) -> Option<GlirelConfig> {
    let home = home_dir()?;
    let model_dir = home.join(".engram").join("models").join("rel").join(model_id);
    if model_dir.exists() && model_dir.join("model.onnx").exists() {
        Some(GlirelConfig {
            model_dir,
            ..Default::default()
        })
    } else {
        None
    }
}
