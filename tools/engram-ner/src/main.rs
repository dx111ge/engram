/// engram-ner: GLiNER zero-shot NER via gline-rs (ONNX).
///
/// Reads JSON requests from stdin (one per line), writes JSON responses to stdout.
/// Designed to be spawned by engram's ingest pipeline as a child process.
///
/// Protocol (JSON Lines):
///   Request:  {"model_dir": "...", "texts": ["..."], "labels": ["person","org"], "threshold": 0.5}
///   Response: {"ok": true, "results": [[{"text":"...","label":"...","score":0.9,"start":0,"end":5}]]}
///   Error:    {"ok": false, "error": "message"}

use gliner::model::pipeline::span::SpanMode;
use gliner::model::{input::text::TextInput, params::Parameters, GLiNER};
use orp::params::RuntimeParameters;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::path::Path;

#[derive(Deserialize)]
struct Request {
    /// Path to model directory containing model.onnx and tokenizer.json
    model_dir: String,
    /// Texts to process (batch)
    texts: Vec<String>,
    /// Entity labels for zero-shot extraction
    labels: Vec<String>,
    /// Minimum confidence threshold
    #[serde(default = "default_threshold")]
    threshold: f32,
}

fn default_threshold() -> f32 {
    0.5
}

#[derive(Serialize)]
struct Entity {
    text: String,
    label: String,
    score: f32,
    start: usize,
    end: usize,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Response {
    Ok {
        ok: bool,
        results: Vec<Vec<Entity>>,
    },
    Err {
        ok: bool,
        error: String,
    },
}

fn respond_err(error: String) -> Response {
    Response::Err { ok: false, error }
}

fn process_request(req: &Request) -> Response {
    let model_dir = Path::new(&req.model_dir);
    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Load model (fresh per request for now — can cache later if needed)
    let model = match GLiNER::<SpanMode>::new(
        Parameters::default(),
        RuntimeParameters::default(),
        tokenizer_path.to_string_lossy().as_ref(),
        model_path.to_string_lossy().as_ref(),
    ) {
        Ok(m) => m,
        Err(e) => return respond_err(format!("failed to load model: {e}")),
    };

    let text_refs: Vec<&str> = req.texts.iter().map(|s| s.as_str()).collect();
    let label_refs: Vec<&str> = req.labels.iter().map(|s| s.as_str()).collect();

    let input = match TextInput::from_str(&text_refs, &label_refs) {
        Ok(i) => i,
        Err(e) => return respond_err(format!("failed to build input: {e}")),
    };

    match model.inference(input) {
        Ok(output) => {
            let results: Vec<Vec<Entity>> = output
                .spans
                .into_iter()
                .map(|spans| {
                    spans
                        .into_iter()
                        .filter(|s| s.probability() >= req.threshold)
                        .map(|s| {
                            let (start, end) = s.offsets();
                            Entity {
                                text: s.text().to_string(),
                                label: s.class().to_string(),
                                score: s.probability(),
                                start,
                                end,
                            }
                        })
                        .collect()
                })
                .collect();
            Response::Ok { ok: true, results }
        }
        Err(e) => respond_err(format!("inference error: {e}")),
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(e) => {
                let resp = respond_err(format!("stdin read error: {e}"));
                let _ = serde_json::to_writer(&mut stdout, &resp);
                let _ = writeln!(stdout);
                let _ = stdout.flush();
                continue;
            }
        };

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = respond_err(format!("invalid request JSON: {e}"));
                let _ = serde_json::to_writer(&mut stdout, &resp);
                let _ = writeln!(stdout);
                let _ = stdout.flush();
                continue;
            }
        };

        let resp = process_request(&req);
        let _ = serde_json::to_writer(&mut stdout, &resp);
        let _ = writeln!(stdout);
        let _ = stdout.flush();
    }
}
