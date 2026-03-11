/// Anno NER backend: GLiNER zero-shot NER via external `engram-ner` binary.
///
/// The `engram-ner` binary wraps gline-rs (ONNX GLiNER inference) and communicates
/// via JSON Lines over stdin/stdout. This avoids an ort version conflict between
/// engram-core's embedder (ort rc.12) and gline-rs (ort rc.9).
///
/// The model is user-installed at `~/.engram/models/ner/<model-name>/`.
///
/// This module is compiled only with `--features anno`.

#[cfg(feature = "anno")]
use crate::traits::Extractor;
#[cfg(feature = "anno")]
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// Configuration for the GLiNER NER backend.
#[derive(Debug, Clone)]
pub struct AnnoConfig {
    /// Path to the model directory (containing model.onnx and tokenizer.json).
    pub model_dir: std::path::PathBuf,
    /// Entity types to extract (for zero-shot models).
    pub entity_types: Vec<String>,
    /// Minimum confidence threshold for extracted entities.
    pub min_confidence: f32,
}

impl Default for AnnoConfig {
    fn default() -> Self {
        Self {
            model_dir: std::path::PathBuf::new(),
            entity_types: vec![
                "PERSON".into(),
                "ORG".into(),
                "LOC".into(),
                "DATE".into(),
                "EVENT".into(),
            ],
            min_confidence: 0.5,
        }
    }
}

/// GLiNER NER backend via external `engram-ner` process.
///
/// Feature-gated: only compiled with `--features anno`.
#[cfg(feature = "anno")]
pub struct AnnoBackend {
    config: AnnoConfig,
    /// Path to the engram-ner binary.
    ner_binary: std::path::PathBuf,
}

#[cfg(feature = "anno")]
impl AnnoBackend {
    /// Create a new GLiNER NER backend.
    ///
    /// Locates the `engram-ner` binary next to the current executable,
    /// then verifies the model directory exists.
    pub fn new(config: AnnoConfig) -> Result<Self, crate::IngestError> {
        let ner_binary = find_ner_binary().ok_or_else(|| {
            crate::IngestError::Config(
                "engram-ner binary not found. It should be in the same directory as engram."
                    .into(),
            )
        })?;

        if !config.model_dir.exists() {
            return Err(crate::IngestError::Config(format!(
                "NER model directory not found: {}",
                config.model_dir.display()
            )));
        }

        let model_path = config.model_dir.join("model.onnx");
        if !model_path.exists() {
            return Err(crate::IngestError::Config(format!(
                "model.onnx not found in {}. Run: engram model install ner <model-id>",
                config.model_dir.display()
            )));
        }

        Ok(Self { config, ner_binary })
    }

    /// Check if the backend is ready for inference.
    pub fn is_ready(&self) -> bool {
        self.config.model_dir.join("model.onnx").exists()
            && self.config.model_dir.join("tokenizer.json").exists()
    }

    /// Run NER on a batch of texts via the engram-ner subprocess.
    fn run_ner(&self, texts: &[&str]) -> Result<Vec<Vec<NerEntity>>, String> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let request = serde_json::json!({
            "model_dir": self.config.model_dir.to_string_lossy(),
            "texts": texts,
            "labels": self.config.entity_types,
            "threshold": self.config.min_confidence,
        });

        let mut child = Command::new(&self.ner_binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn engram-ner: {e}"))?;

        // Write request and close stdin to signal EOF
        {
            let stdin = child.stdin.as_mut().ok_or("failed to open stdin")?;
            serde_json::to_writer(&mut *stdin, &request)
                .map_err(|e| format!("failed to write request: {e}"))?;
            writeln!(stdin).map_err(|e| format!("failed to write newline: {e}"))?;
        }
        // Drop stdin to close it
        drop(child.stdin.take());

        let output = child
            .wait_with_output()
            .map_err(|e| format!("engram-ner process error: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("engram-ner exited with {}: {stderr}", output.status));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("");

        let resp: serde_json::Value = serde_json::from_str(first_line)
            .map_err(|e| format!("invalid response from engram-ner: {e}"))?;

        if resp.get("ok") == Some(&serde_json::Value::Bool(true)) {
            let results: Vec<Vec<NerEntity>> =
                serde_json::from_value(resp["results"].clone())
                    .map_err(|e| format!("failed to parse results: {e}"))?;
            Ok(results)
        } else {
            let error = resp["error"].as_str().unwrap_or("unknown error");
            Err(error.to_string())
        }
    }
}

/// Entity returned by the engram-ner process.
#[cfg(feature = "anno")]
#[derive(serde::Deserialize)]
struct NerEntity {
    text: String,
    label: String,
    score: f32,
    start: usize,
    end: usize,
}

#[cfg(feature = "anno")]
impl Extractor for AnnoBackend {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let _ = lang; // GLiNER handles multilingual natively

        match self.run_ner(&[text]) {
            Ok(mut results) => {
                if let Some(entities) = results.pop() {
                    entities
                        .into_iter()
                        .map(|e| ExtractedEntity {
                            text: e.text,
                            entity_type: e.label,
                            span: (e.start, e.end),
                            confidence: e.score,
                            method: ExtractionMethod::StatisticalModel,
                            language: String::new(),
                            resolved_to: None,
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "engram-ner inference failed");
                Vec::new()
            }
        }
    }

    fn name(&self) -> &str {
        "gliner-onnx"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::StatisticalModel
    }

    fn supported_languages(&self) -> Vec<String> {
        // Depends on the installed model — GLiNER Multi supports 100+ languages
        vec![]
    }
}

/// Find the `engram-ner` binary next to the current executable.
fn find_ner_binary() -> Option<std::path::PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    #[cfg(target_os = "windows")]
    let name = "engram-ner.exe";
    #[cfg(not(target_os = "windows"))]
    let name = "engram-ner";

    let path = exe_dir.join(name);
    if path.exists() {
        Some(path)
    } else {
        // Also check PATH
        which_in_path(name)
    }
}

/// Check if a binary exists in PATH.
fn which_in_path(name: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|p| p.exists())
    })
}

/// Check if an NER model is installed at the standard location.
pub fn find_ner_model(model_name: &str) -> Option<AnnoConfig> {
    let home = dirs_next()?;
    let model_dir = home.join(".engram").join("models").join("ner").join(model_name);

    if model_dir.join("model.onnx").exists() && model_dir.join("tokenizer.json").exists() {
        Some(AnnoConfig {
            model_dir,
            ..Default::default()
        })
    } else {
        None
    }
}

/// Check if the engram-ner binary is available.
pub fn is_ner_binary_available() -> bool {
    find_ner_binary().is_some()
}

/// Get the user's home directory.
fn dirs_next() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// List installed NER models.
pub fn list_installed_models() -> Vec<String> {
    let home = match dirs_next() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let models_dir = home.join(".engram").join("models").join("ner");
    if !models_dir.exists() {
        return Vec::new();
    }

    std::fs::read_dir(&models_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.path().join("model.onnx").exists() {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect()
}
