/// GLiNER ONNX NER backend via external `engram-ner` subprocess.
///
/// Spawns a long-running `engram-ner` sidecar process that loads the ONNX model
/// once at startup, then processes extraction requests via JSON Lines on stdin/stdout.
///
/// Default model: `knowledgator/gliner-x-small` (173 MB quantized, 20 languages).
/// Alternative: `onnx-community/gliner_multi-v2.1` (349 MB INT8, 6 languages).
///
/// This module is compiled only with `--features gliner`.

#[cfg(feature = "gliner")]
use crate::traits::Extractor;
#[cfg(feature = "gliner")]
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// Configuration for the GLiNER ONNX NER backend.
#[derive(Debug, Clone)]
pub struct GlinerConfig {
    /// Path to model directory containing `model.onnx` and `tokenizer.json`.
    pub model_dir: std::path::PathBuf,
    /// Entity types to extract (zero-shot labels).
    pub entity_types: Vec<String>,
    /// Minimum confidence threshold for extracted entities.
    pub min_confidence: f32,
}

impl Default for GlinerConfig {
    fn default() -> Self {
        Self {
            model_dir: std::path::PathBuf::new(),
            entity_types: vec![
                "person".into(),
                "organization".into(),
                "location".into(),
                "date".into(),
                "event".into(),
                "product".into(),
            ],
            min_confidence: 0.3,
        }
    }
}

/// Long-running GLiNER ONNX NER backend via `engram-ner` subprocess.
///
/// The subprocess loads the model once at startup and stays alive for the
/// lifetime of this backend. Requests are sent as JSON lines to stdin,
/// responses read from stdout.
///
/// Feature-gated: only compiled with `--features gliner`.
#[cfg(feature = "gliner")]
pub struct GlinerBackend {
    config: GlinerConfig,
    inner: std::sync::Mutex<GlinerInner>,
}

#[cfg(feature = "gliner")]
struct GlinerInner {
    stdin: std::io::BufWriter<std::process::ChildStdin>,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    _child: std::process::Child,
}

#[cfg(feature = "gliner")]
impl GlinerBackend {
    /// Create a new GLiNER ONNX backend.
    ///
    /// Spawns the `engram-ner` subprocess, loads the model, and waits for
    /// the ready signal before returning.
    pub fn new(config: GlinerConfig) -> Result<Self, crate::IngestError> {
        use std::io::BufRead;
        use std::process::{Command, Stdio};

        let binary = find_ner_binary().ok_or_else(|| {
            crate::IngestError::Config(
                "engram-ner binary not found. Build with: cd tools/engram-ner && cargo build --release"
                    .into(),
            )
        })?;

        if !config.model_dir.exists() {
            return Err(crate::IngestError::Config(format!(
                "NER model directory not found: {}. Download via POST /config/ner-download or the setup wizard.",
                config.model_dir.display()
            )));
        }

        let model_onnx = config.model_dir.join("model.onnx");
        if !model_onnx.exists() {
            return Err(crate::IngestError::Config(format!(
                "model.onnx not found in {}",
                config.model_dir.display()
            )));
        }

        tracing::info!(
            model_dir = %config.model_dir.display(),
            binary = %binary.display(),
            "spawning engram-ner subprocess"
        );

        let mut child = Command::new(&binary)
            .arg(config.model_dir.to_string_lossy().as_ref())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| crate::IngestError::Ner(format!("failed to spawn engram-ner: {e}")))?;

        let child_stdin = child.stdin.take().ok_or_else(|| {
            crate::IngestError::Ner("failed to capture engram-ner stdin".into())
        })?;
        let child_stdout = child.stdout.take().ok_or_else(|| {
            crate::IngestError::Ner("failed to capture engram-ner stdout".into())
        })?;

        let stdin = std::io::BufWriter::new(child_stdin);
        let mut stdout = std::io::BufReader::new(child_stdout);

        // Wait for ready signal (first line of output)
        let mut ready_line = String::new();
        stdout.read_line(&mut ready_line)
            .map_err(|e| crate::IngestError::Ner(format!("engram-ner startup read failed: {e}")))?;

        let ready: serde_json::Value = serde_json::from_str(ready_line.trim())
            .map_err(|e| crate::IngestError::Ner(format!("engram-ner startup parse failed: {e} (got: {ready_line})")))?;

        if ready.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = ready.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(crate::IngestError::Ner(format!("engram-ner model load failed: {err}")));
        }

        tracing::info!(
            model_dir = %config.model_dir.display(),
            "GLiNER ONNX NER backend ready (subprocess)"
        );

        Ok(Self {
            config,
            inner: std::sync::Mutex::new(GlinerInner {
                stdin,
                stdout,
                _child: child,
            }),
        })
    }
}

#[cfg(feature = "gliner")]
impl Extractor for GlinerBackend {
    fn extract(&self, text: &str, _lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        use std::io::{BufRead, Write};

        if text.trim().is_empty() {
            return Vec::new();
        }

        let request = serde_json::json!({
            "texts": [text],
            "labels": self.config.entity_types,
            "threshold": self.config.min_confidence,
        });

        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("engram-ner mutex poisoned: {e}");
                return Vec::new();
            }
        };

        // Write request
        if let Err(e) = serde_json::to_writer(&mut inner.stdin, &request) {
            tracing::warn!("engram-ner write failed: {e}");
            return Vec::new();
        }
        if let Err(e) = writeln!(inner.stdin) {
            tracing::warn!("engram-ner write newline failed: {e}");
            return Vec::new();
        }
        if let Err(e) = inner.stdin.flush() {
            tracing::warn!("engram-ner flush failed: {e}");
            return Vec::new();
        }

        // Read response
        let mut response_line = String::new();
        if let Err(e) = inner.stdout.read_line(&mut response_line) {
            tracing::warn!("engram-ner read failed: {e}");
            return Vec::new();
        }

        let resp: serde_json::Value = match serde_json::from_str(response_line.trim()) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("engram-ner parse failed: {e} (got: {})", response_line.trim());
                return Vec::new();
            }
        };

        if resp.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = resp.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
            tracing::warn!("engram-ner extraction error: {err}");
            return Vec::new();
        }

        // Parse results — we sent 1 text, expect results[0]
        let results = resp.get("results")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_array());

        match results {
            Some(entities) => {
                entities.iter().filter_map(|e| {
                    let text = e.get("text")?.as_str()?;
                    let label = e.get("label")?.as_str()?;
                    let score = e.get("score")?.as_f64()? as f32;
                    let start = e.get("start")?.as_u64()? as usize;
                    let end = e.get("end")?.as_u64()? as usize;

                    Some(ExtractedEntity {
                        text: text.to_string(),
                        entity_type: label.to_string(),
                        span: (start, end),
                        confidence: score,
                        method: ExtractionMethod::StatisticalModel,
                        language: String::new(),
                        resolved_to: None,
                    })
                }).collect()
            }
            None => Vec::new(),
        }
    }

    fn name(&self) -> &str {
        "gliner-onnx"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::StatisticalModel
    }

    fn supported_languages(&self) -> Vec<String> {
        // knowledgator/gliner-x-small supports 20 languages
        vec![]
    }
}

// ── Model discovery ──

/// Check if a NER model is available as a local ONNX model.
///
/// Looks in `~/.engram/models/ner/{model_name}/` for `model.onnx` + `tokenizer.json`.
/// Also tries the sanitized name (org/model -> org_model) since the download endpoint
/// uses that format for directory names.
pub fn find_ner_model(model_name: &str) -> Option<GlinerConfig> {
    let home = crate::engram_home()?;
    let models_dir = home.join("models").join("ner");

    // Try exact name first, then sanitized (org/model -> org_model)
    let candidates = [
        model_name.to_string(),
        model_name.replace('/', "_"),
    ];

    for candidate in &candidates {
        let model_dir = models_dir.join(candidate);
        if model_dir.join("model.onnx").exists() && model_dir.join("tokenizer.json").exists() {
            return Some(GlinerConfig {
                model_dir,
                ..Default::default()
            });
        }
    }

    None
}

/// List installed NER models (local ONNX models with model.onnx + tokenizer.json).
pub fn list_installed_models() -> Vec<String> {
    let home = match crate::engram_home() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let models_dir = home.join("models").join("ner");
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

/// Default model ID for the setup wizard.
pub const DEFAULT_MODEL_ID: &str = "knowledgator/gliner-x-small";

/// Default ONNX variant to download.
pub const DEFAULT_MODEL_VARIANT: &str = "quantized";

/// Known GLiNER ONNX models available for download.
pub const KNOWN_MODELS: &[(&str, &str, &str, u32)] = &[
    // (model_id, variant, description, size_mb)
    ("knowledgator/gliner-x-small", "quantized", "20 languages, 173 MB (recommended)", 173),
    ("onnx-community/gliner_multi-v2.1", "int8", "6 languages, 349 MB", 349),
    ("knowledgator/gliner-x-base", "quantized", "20 languages, 303 MB", 303),
    ("knowledgator/gliner-x-large", "quantized", "20 languages, 610 MB (best accuracy)", 610),
];

// ── Binary discovery ──

/// Find the `engram-ner` binary: next to exe, ~/.engram/bin/, or on PATH.
#[cfg(feature = "gliner")]
fn find_ner_binary() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let name = "engram-ner.exe";
    #[cfg(not(target_os = "windows"))]
    let name = "engram-ner";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gliner_config_defaults() {
        let config = GlinerConfig::default();
        assert_eq!(config.entity_types.len(), 6);
        assert!(config.entity_types.contains(&"person".to_string()));
        assert!(config.entity_types.contains(&"organization".to_string()));
        assert!(config.entity_types.contains(&"location".to_string()));
        assert!((config.min_confidence - 0.3).abs() < f32::EPSILON);
        assert!(config.model_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_find_ner_model_nonexistent() {
        let result = find_ner_model("nonexistent_model_xyz_123");
        assert!(result.is_none());
    }

    #[test]
    fn test_default_model_id() {
        assert_eq!(DEFAULT_MODEL_ID, "knowledgator/gliner-x-small");
    }

    #[test]
    fn test_known_models_not_empty() {
        assert!(!KNOWN_MODELS.is_empty());
        // First should be default
        assert_eq!(KNOWN_MODELS[0].0, DEFAULT_MODEL_ID);
    }
}
