/// engram-rel: NLI-based zero-shot relation extraction via ONNX.
///
/// Long-running subprocess: loads the model once at startup, then processes
/// requests via JSON Lines on stdin/stdout.
///
/// Usage: engram-rel <model_dir>
///   model_dir: path containing model.onnx + tokenizer.json
///
/// Protocol (JSON Lines):
///   Ready:    {"ok": true, "status": "ready"}
///   Request:  {"text": "...", "entities": [...],
///              "relation_templates": {"works_at": "{head} works at {tail}", ...},
///              "threshold": 0.9}
///   Response: {"ok": true, "relations": [{"head": "...", "tail": "...", "label": "...", "score": 0.9}]}
///   Error:    {"ok": false, "error": "message"}
///
/// NLI approach (EMNLP 2021, Sainz et al.):
///   For each (head, tail) entity pair and each relation template:
///   - Premise: the sentence containing both entities
///   - Hypothesis: template with {head}/{tail} filled in (e.g., "John works at Google")
///   - Run NLI model -> softmax([entailment, neutral, contradiction])
///   - If entailment score > threshold -> emit relation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Request {
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
    label: String,
    start: usize,
    end: usize,
}

fn default_threshold() -> f32 {
    0.9
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

/// Normalize entity type labels from GLiNER to canonical forms.
/// GLiNER emits various forms: "person", "PER", "PERSON", "organization", "ORG", etc.
fn normalize_type(label: &str) -> &'static str {
    match label.to_ascii_lowercase().as_str() {
        "person" | "per" => "person",
        "organization" | "org" | "company" | "corporation" => "organization",
        "location" | "loc" | "gpe" | "place" | "city" | "country" => "location",
        "date" | "time" | "datetime" => "date",
        "event" => "event",
        "product" => "product",
        _ => "other",
    }
}

/// Check whether a relation template is semantically valid for a given
/// (head_type, tail_type) entity pair. Returns `true` if the pair is compatible
/// with the relation, `false` if it would produce nonsensical triples.
fn type_compatible(rel_type: &str, head_type: &str, tail_type: &str) -> bool {
    let h = normalize_type(head_type);
    let t = normalize_type(tail_type);

    match rel_type {
        // person -> organization
        "works_at" | "member_of" | "holds_position" => h == "person" && t == "organization",

        // person -> location
        "born_in" | "lives_in" | "citizen_of" => h == "person" && t == "location",

        // person -> person
        "spouse" | "parent_of" | "child_of" => h == "person" && t == "person",

        // organization -> location
        "headquartered_in" => h == "organization" && t == "location",

        // organization -> organization
        "subsidiary_of" | "acquired_by" => h == "organization" && t == "organization",

        // founded_by: person -> organization OR organization -> person
        "founded_by" => {
            (h == "person" && t == "organization")
                || (h == "organization" && t == "person")
        }

        // located_in: org -> location OR location -> location
        "located_in" => {
            (h == "organization" && t == "location")
                || (h == "location" && t == "location")
        }

        // part_of: org -> org OR location -> location
        "part_of" => {
            (h == "organization" && t == "organization")
                || (h == "location" && t == "location")
        }

        // location -> location
        "capital_of" => h == "location" && t == "location",

        // any -> any (no type restriction)
        // instance_of, cause_of, author_of, produces, educated_at, and any
        // user-defined templates we don't recognize
        _ => true,
    }
}

fn process_request(
    session: &mut ort::session::Session,
    tokenizer: &tokenizers::Tokenizer,
    req: &Request,
) -> Response {
    // Early exit: nothing to do if too few entities or no templates
    if req.entities.len() < 2 || req.relation_templates.is_empty() {
        return Response::Ok {
            ok: true,
            relations: Vec::new(),
        };
    }

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

            // Test each relation template (skip type-incompatible pairs)
            for (rel_type, template) in &req.relation_templates {
                if !type_compatible(rel_type, &head.label, &tail.label) {
                    continue;
                }

                let hypothesis = template
                    .replace("{head}", &head.text)
                    .replace("{tail}", &tail.text);

                match nli_entailment(
                    session,
                    tokenizer,
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

    // Deduplicate: keep only the top-1 relation per directed (head, tail) pair.
    // Since relations are sorted by score descending, the first occurrence wins.
    let mut seen: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    relations.retain(|r| seen.insert((r.head.clone(), r.tail.clone())));

    Response::Ok {
        ok: true,
        relations,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: engram-rel <model_dir>");
        eprintln!("  model_dir: path containing model.onnx + tokenizer.json");
        std::process::exit(1);
    }

    let model_dir = std::path::Path::new(&args[1]);
    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    let mut stdout = io::stdout();

    // Load tokenizer once at startup
    let tokenizer = match tokenizers::Tokenizer::from_file(&tokenizer_path) {
        Ok(t) => t,
        Err(e) => {
            let resp = respond_err(format!("failed to load tokenizer: {e}"));
            let _ = serde_json::to_writer(&mut stdout, &resp);
            let _ = writeln!(stdout);
            let _ = stdout.flush();
            std::process::exit(1);
        }
    };

    // Load ONNX session once at startup
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
        Err(e) => {
            let resp = respond_err(format!("failed to load ONNX model: {e}"));
            let _ = serde_json::to_writer(&mut stdout, &resp);
            let _ = writeln!(stdout);
            let _ = stdout.flush();
            std::process::exit(1);
        }
    };

    // Signal ready
    let ready = Response::Ready {
        ok: true,
        status: "ready".into(),
    };
    let _ = serde_json::to_writer(&mut stdout, &ready);
    let _ = writeln!(stdout);
    let _ = stdout.flush();

    // Process requests in a loop (model stays loaded)
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

        let resp = process_request(&mut session, &tokenizer, &req);
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
        let result = softmax_entailment(&[0.0, 0.0, 0.0]);
        assert!((result - 1.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_softmax_entailment_strong_entailment() {
        let result = softmax_entailment(&[10.0, 0.0, 0.0]);
        assert!(result > 0.99);
    }

    #[test]
    fn test_softmax_entailment_strong_contradiction() {
        let result = softmax_entailment(&[0.0, 0.0, 10.0]);
        assert!(result < 0.01);
    }

    #[test]
    fn test_softmax_numerical_stability() {
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
        let premise = find_premise(text, (16, 20), (30, 36));
        assert_eq!(premise, "John works at Google.");
    }

    #[test]
    fn test_find_premise_spans_full_text() {
        let text = "John and Google are related";
        let premise = find_premise(text, (0, 4), (9, 15));
        assert_eq!(premise, "John and Google are related");
    }

    // --- type_compatible tests ---

    #[test]
    fn test_type_compatible_person_org() {
        assert!(type_compatible("works_at", "person", "organization"));
        assert!(type_compatible("member_of", "person", "organization"));
        assert!(type_compatible("holds_position", "person", "organization"));
        assert!(!type_compatible("works_at", "organization", "person"));
        assert!(!type_compatible("works_at", "location", "organization"));
        assert!(!type_compatible("holds_position", "person", "location"));
    }

    #[test]
    fn test_type_compatible_person_location() {
        assert!(type_compatible("born_in", "person", "location"));
        assert!(type_compatible("lives_in", "person", "location"));
        assert!(type_compatible("citizen_of", "person", "location"));
        assert!(!type_compatible("born_in", "organization", "location"));
        assert!(!type_compatible("lives_in", "person", "person"));
    }

    #[test]
    fn test_type_compatible_person_person() {
        assert!(type_compatible("spouse", "person", "person"));
        assert!(type_compatible("parent_of", "person", "person"));
        assert!(type_compatible("child_of", "person", "person"));
        assert!(!type_compatible("spouse", "person", "organization"));
    }

    #[test]
    fn test_type_compatible_org_location() {
        assert!(type_compatible("headquartered_in", "organization", "location"));
        assert!(!type_compatible("headquartered_in", "person", "location"));
        assert!(!type_compatible("headquartered_in", "location", "location"));
    }

    #[test]
    fn test_type_compatible_org_org() {
        assert!(type_compatible("subsidiary_of", "organization", "organization"));
        assert!(type_compatible("acquired_by", "organization", "organization"));
        assert!(!type_compatible("subsidiary_of", "person", "organization"));
    }

    #[test]
    fn test_type_compatible_founded_by_bidirectional() {
        assert!(type_compatible("founded_by", "person", "organization"));
        assert!(type_compatible("founded_by", "organization", "person"));
        assert!(!type_compatible("founded_by", "location", "person"));
        assert!(!type_compatible("founded_by", "person", "person"));
    }

    #[test]
    fn test_type_compatible_located_in() {
        assert!(type_compatible("located_in", "organization", "location"));
        assert!(type_compatible("located_in", "location", "location"));
        assert!(!type_compatible("located_in", "person", "location"));
    }

    #[test]
    fn test_type_compatible_part_of() {
        assert!(type_compatible("part_of", "organization", "organization"));
        assert!(type_compatible("part_of", "location", "location"));
        assert!(!type_compatible("part_of", "person", "organization"));
    }

    #[test]
    fn test_type_compatible_capital_of() {
        assert!(type_compatible("capital_of", "location", "location"));
        assert!(!type_compatible("capital_of", "organization", "location"));
    }

    #[test]
    fn test_type_compatible_any_any() {
        assert!(type_compatible("instance_of", "person", "organization"));
        assert!(type_compatible("cause_of", "event", "event"));
        assert!(type_compatible("author_of", "person", "product"));
        assert!(type_compatible("produces", "organization", "product"));
        assert!(type_compatible("educated_at", "person", "organization"));
        assert!(type_compatible("custom_relation", "date", "event"));
    }

    #[test]
    fn test_type_compatible_gliner_label_variants() {
        assert!(type_compatible("works_at", "PER", "ORG"));
        assert!(type_compatible("works_at", "PERSON", "company"));
        assert!(type_compatible("born_in", "person", "GPE"));
        assert!(type_compatible("born_in", "per", "city"));
        assert!(type_compatible("headquartered_in", "corporation", "country"));
        assert!(!type_compatible("works_at", "LOC", "ORG"));
    }

    #[test]
    fn test_normalize_type() {
        assert_eq!(normalize_type("person"), "person");
        assert_eq!(normalize_type("PER"), "person");
        assert_eq!(normalize_type("PERSON"), "person");
        assert_eq!(normalize_type("organization"), "organization");
        assert_eq!(normalize_type("ORG"), "organization");
        assert_eq!(normalize_type("company"), "organization");
        assert_eq!(normalize_type("corporation"), "organization");
        assert_eq!(normalize_type("location"), "location");
        assert_eq!(normalize_type("LOC"), "location");
        assert_eq!(normalize_type("GPE"), "location");
        assert_eq!(normalize_type("place"), "location");
        assert_eq!(normalize_type("city"), "location");
        assert_eq!(normalize_type("country"), "location");
        assert_eq!(normalize_type("date"), "date");
        assert_eq!(normalize_type("time"), "date");
        assert_eq!(normalize_type("event"), "event");
        assert_eq!(normalize_type("product"), "product");
        assert_eq!(normalize_type("MISC"), "other");
        assert_eq!(normalize_type("unknown_thing"), "other");
    }
}
