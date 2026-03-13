/// engram-rel: NLI-based zero-shot relation extraction via ONNX.
///
/// Reads JSON requests from stdin (one per line), writes JSON responses to stdout.
/// Designed to be spawned by engram's ingest pipeline as a child process.
///
/// Protocol (JSON Lines):
///   Request:  {"model_dir": "...", "text": "...", "entities": [...],
///              "relation_templates": {"works_at": "{head} works at {tail}", ...},
///              "threshold": 0.5}
///   Response: {"ok": true, "relations": [{"head": "...", "tail": "...", "label": "...", "score": 0.9}]}
///   Error:    {"ok": false, "error": "message"}
///
/// NLI approach (EMNLP 2021, Sainz et al.):
///   For each (head, tail) entity pair and each relation template:
///   - Premise: the sentence containing both entities
///   - Hypothesis: template with {head}/{tail} filled in (e.g., "John works at Google")
///   - Run NLI model → softmax([entailment, neutral, contradiction])
///   - If entailment score > threshold → emit relation

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::collections::HashMap;

#[derive(Deserialize)]
struct Request {
    /// Path to model directory containing model.onnx and tokenizer.json
    model_dir: String,
    /// Input text
    text: String,
    /// Entities already extracted by NER
    entities: Vec<EntitySpan>,
    /// Relation templates: { rel_type: "{head} works at {tail}" }
    relation_templates: HashMap<String, String>,
    /// Minimum entailment confidence threshold
    #[serde(default = "default_threshold")]
    threshold: f32,
    /// Maximum sequence length for tokenization
    #[serde(default = "default_max_seq_length")]
    max_seq_length: usize,
}

#[derive(Deserialize)]
struct EntitySpan {
    text: String,
    #[allow(dead_code)]
    label: String,
    start: usize,
    end: usize,
}

fn default_threshold() -> f32 {
    0.5
}

fn default_max_seq_length() -> usize {
    512
}

#[derive(Serialize)]
struct Relation {
    head: String,
    tail: String,
    label: String,
    score: f32,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Response {
    Ok {
        ok: bool,
        relations: Vec<Relation>,
    },
    Err {
        ok: bool,
        error: String,
    },
}

fn respond_err(error: String) -> Response {
    Response::Err { ok: false, error }
}

/// Softmax over logits, returns the entailment probability (index 0).
fn softmax_entailment(logits: &[f32]) -> f32 {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps[0] / sum
}

/// Find the sentence in `text` that contains both entity spans.
/// Falls back to the full text if no single sentence contains both.
fn find_premise(text: &str, span_a: (usize, usize), span_b: (usize, usize)) -> &str {
    let min_start = span_a.0.min(span_b.0);
    let max_end = span_a.1.max(span_b.1);

    let start = text[..min_start]
        .rfind(|c: char| c == '.' || c == '!' || c == '?')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = text[max_end..]
        .find(|c: char| c == '.' || c == '!' || c == '?')
        .map(|i| max_end + i + 1)
        .unwrap_or(text.len());

    text[start..end].trim()
}

/// Run NLI entailment for a (premise, hypothesis) pair.
fn nli_entailment(
    session: &mut ort::session::Session,
    tokenizer: &tokenizers::Tokenizer,
    premise: &str,
    hypothesis: &str,
    max_seq_length: usize,
) -> Result<f32, String> {
    let encoding = tokenizer
        .encode((premise, hypothesis), true)
        .map_err(|e| format!("tokenization failed: {e}"))?;

    let ids = encoding.get_ids();
    let mask = encoding.get_attention_mask();
    let seq_len = ids.len().min(max_seq_length);

    let input_ids: Vec<i64> = ids[..seq_len].iter().map(|&id| id as i64).collect();
    let attention_mask: Vec<i64> = mask[..seq_len].iter().map(|&m| m as i64).collect();

    let input_ids_tensor = ort::value::Tensor::from_array(
        ndarray::Array2::from_shape_vec((1, seq_len), input_ids)
            .map_err(|e| format!("input_ids shape: {e}"))?,
    )
    .map_err(|e| format!("input_ids tensor: {e}"))?;

    let attention_mask_tensor = ort::value::Tensor::from_array(
        ndarray::Array2::from_shape_vec((1, seq_len), attention_mask)
            .map_err(|e| format!("attention_mask shape: {e}"))?,
    )
    .map_err(|e| format!("attention_mask tensor: {e}"))?;

    let inputs = ort::inputs![
        "input_ids" => input_ids_tensor,
        "attention_mask" => attention_mask_tensor,
    ];

    let output_names: Vec<String> = session
        .outputs()
        .iter()
        .map(|o| o.name().to_string())
        .collect();

    let outputs = session
        .run(inputs)
        .map_err(|e| format!("ort inference: {e}"))?;

    let output_name = output_names
        .first()
        .ok_or("model has no outputs")?;

    let logits_value = outputs
        .get(
            if outputs.get("logits").is_some() {
                "logits"
            } else {
                output_name.as_str()
            },
        )
        .ok_or("no output tensor found")?;

    let (_shape, data) = logits_value
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("extract logits: {e}"))?;

    if data.len() < 3 {
        return Err(format!("expected at least 3 logits, got {}", data.len()));
    }

    Ok(softmax_entailment(&data[..3]))
}

fn process_request(req: &Request) -> Response {
    // Early exit: nothing to do if too few entities or no templates
    if req.entities.len() < 2 || req.relation_templates.is_empty() {
        return Response::Ok {
            ok: true,
            relations: Vec::new(),
        };
    }

    let model_dir = std::path::Path::new(&req.model_dir);
    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    if !model_path.exists() {
        return respond_err(format!("model.onnx not found in {}", model_dir.display()));
    }
    if !tokenizer_path.exists() {
        return respond_err(format!(
            "tokenizer.json not found in {}",
            model_dir.display()
        ));
    }

    // Load tokenizer
    let tokenizer = match tokenizers::Tokenizer::from_file(&tokenizer_path) {
        Ok(t) => t,
        Err(e) => return respond_err(format!("failed to load tokenizer: {e}")),
    };

    // Load ONNX session
    let mut session = match (|| -> Result<ort::session::Session, String> {
        let mut builder = ort::session::Session::builder()
            .map_err(|e| format!("session builder: {e}"))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| format!("optimization: {e}"))?
            .with_intra_threads(4)
            .map_err(|e| format!("threads: {e}"))?;
        builder
            .commit_from_file(&model_path)
            .map_err(|e| format!("load model: {e}"))
    })() {
        Ok(s) => s,
        Err(e) => return respond_err(format!("failed to load ONNX model: {e}")),
    };

    let mut relations = Vec::new();

    // For each ordered entity pair (head, tail)
    for (i, head) in req.entities.iter().enumerate() {
        for (j, tail) in req.entities.iter().enumerate() {
            if i == j {
                continue;
            }

            let premise = find_premise(
                &req.text,
                (head.start, head.end),
                (tail.start, tail.end),
            );
            if premise.is_empty() {
                continue;
            }

            // Test each relation template
            for (rel_type, template) in &req.relation_templates {
                let hypothesis = template
                    .replace("{head}", &head.text)
                    .replace("{tail}", &tail.text);

                match nli_entailment(
                    &mut session,
                    &tokenizer,
                    premise,
                    &hypothesis,
                    req.max_seq_length,
                ) {
                    Ok(score) => {
                        if score >= req.threshold {
                            relations.push(Relation {
                                head: head.text.clone(),
                                tail: tail.text.clone(),
                                label: rel_type.clone(),
                                score,
                            });
                        }
                    }
                    Err(_) => {
                        // Skip failed NLI calls silently
                    }
                }
            }
        }
    }

    // Sort by score descending
    relations.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Response::Ok {
        ok: true,
        relations,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_softmax_entailment_uniform() {
        // Equal logits -> equal probabilities -> 1/3
        let result = softmax_entailment(&[0.0, 0.0, 0.0]);
        assert!((result - 1.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_softmax_entailment_strong_entailment() {
        // First logit much larger -> high entailment
        let result = softmax_entailment(&[10.0, 0.0, 0.0]);
        assert!(result > 0.99);
    }

    #[test]
    fn test_softmax_entailment_strong_contradiction() {
        // Last logit much larger -> low entailment
        let result = softmax_entailment(&[0.0, 0.0, 10.0]);
        assert!(result < 0.01);
    }

    #[test]
    fn test_softmax_numerical_stability() {
        // Large values should not overflow
        let result = softmax_entailment(&[1000.0, 999.0, 998.0]);
        assert!(result > 0.0 && result <= 1.0);
    }

    #[test]
    fn test_find_premise_single_sentence() {
        let text = "John works at Google.";
        let premise = find_premise(text, (0, 4), (14, 20));
        assert_eq!(premise, "John works at Google.");
    }

    #[test]
    fn test_find_premise_multi_sentence() {
        let text = "Alice is a CEO. John works at Google. They met in 2020.";
        // "John" starts at 16, "Google" ends at 36
        let premise = find_premise(text, (16, 20), (30, 36));
        assert_eq!(premise, "John works at Google.");
    }

    #[test]
    fn test_find_premise_spans_full_text() {
        let text = "John and Google are related";
        let premise = find_premise(text, (0, 4), (9, 15));
        assert_eq!(premise, "John and Google are related");
    }

    #[test]
    fn test_process_request_missing_model() {
        let req = Request {
            model_dir: "/nonexistent/path".into(),
            text: "John works at Google.".into(),
            entities: vec![
                EntitySpan { text: "John".into(), label: "person".into(), start: 0, end: 4 },
                EntitySpan { text: "Google".into(), label: "org".into(), start: 14, end: 20 },
            ],
            relation_templates: HashMap::from([("works_at".into(), "{head} works at {tail}".into())]),
            threshold: 0.5,
            max_seq_length: 512,
        };
        let resp = process_request(&req);
        match resp {
            Response::Err { ok, error } => {
                assert!(!ok);
                assert!(error.contains("not found"));
            }
            Response::Ok { .. } => panic!("expected error for missing model"),
        }
    }

    #[test]
    fn test_process_request_too_few_entities() {
        let req = Request {
            model_dir: "/nonexistent".into(),
            text: "test".into(),
            entities: vec![EntitySpan { text: "John".into(), label: "person".into(), start: 0, end: 4 }],
            relation_templates: HashMap::from([("works_at".into(), "{head} works at {tail}".into())]),
            threshold: 0.5,
            max_seq_length: 512,
        };
        let resp = process_request(&req);
        match resp {
            Response::Ok { ok, relations } => {
                assert!(ok);
                assert!(relations.is_empty());
            }
            Response::Err { .. } => panic!("expected Ok with empty relations for single entity"),
        }
    }

    #[test]
    fn test_process_request_empty_templates() {
        let req = Request {
            model_dir: "/nonexistent".into(),
            text: "test".into(),
            entities: vec![
                EntitySpan { text: "John".into(), label: "person".into(), start: 0, end: 4 },
                EntitySpan { text: "Google".into(), label: "org".into(), start: 14, end: 20 },
            ],
            relation_templates: HashMap::new(),
            threshold: 0.5,
            max_seq_length: 512,
        };
        let resp = process_request(&req);
        match resp {
            Response::Ok { ok, relations } => {
                assert!(ok);
                assert!(relations.is_empty());
            }
            Response::Err { .. } => panic!("expected Ok with empty relations for no templates"),
        }
    }
}
