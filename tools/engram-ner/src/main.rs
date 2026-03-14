/// engram-ner: GLiNER zero-shot NER via gline-rs (ONNX).
///
/// Long-running subprocess: loads model once at startup from CLI arg,
/// then reads JSON extraction requests from stdin, writes JSON responses to stdout.
///
/// Usage: engram-ner <model_dir>
///
/// On startup, prints a ready signal: {"ok": true, "status": "ready"}
///
/// Protocol (JSON Lines on stdin/stdout):
///   Request:  {"texts": ["..."], "labels": ["person","org"], "threshold": 0.5}
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
    Ready {
        ok: bool,
        status: String,
    },
    Err {
        ok: bool,
        error: String,
    },
}

fn respond_err(error: String) -> Response {
    Response::Err { ok: false, error }
}

fn process_request(model: &GLiNER<SpanMode>, req: &Request) -> Response {
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
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: engram-ner <model_dir>");
        eprintln!("  model_dir: path containing model.onnx + tokenizer.json");
        std::process::exit(1);
    }

    let model_dir = Path::new(&args[1]);
    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    if !model_path.exists() {
        eprintln!("model.onnx not found in {}", model_dir.display());
        std::process::exit(1);
    }
    if !tokenizer_path.exists() {
        eprintln!("tokenizer.json not found in {}", model_dir.display());
        std::process::exit(1);
    }

    // Load model once at startup
    let model = match GLiNER::<SpanMode>::new(
        Parameters::default(),
        RuntimeParameters::default(),
        tokenizer_path.to_string_lossy().as_ref(),
        model_path.to_string_lossy().as_ref(),
    ) {
        Ok(m) => m,
        Err(e) => {
            let mut stdout = io::stdout();
            let resp = respond_err(format!("failed to load model: {e}"));
            let _ = serde_json::to_writer(&mut stdout, &resp);
            let _ = writeln!(stdout);
            let _ = stdout.flush();
            std::process::exit(1);
        }
    };

    // Signal ready
    let mut stdout = io::stdout();
    let ready = Response::Ready {
        ok: true,
        status: "ready".to_string(),
    };
    let _ = serde_json::to_writer(&mut stdout, &ready);
    let _ = writeln!(stdout);
    let _ = stdout.flush();

    // Process requests from stdin
    let stdin = io::stdin();
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

        let resp = process_request(&model, &req);
        let _ = serde_json::to_writer(&mut stdout, &resp);
        let _ = writeln!(stdout);
        let _ = stdout.flush();
    }
}
