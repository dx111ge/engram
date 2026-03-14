/// Evaluation harness: GLiNER-Multitask relation extraction with German text.
///
/// Tests gline-rs RelationPipeline with onnx-community/gliner-multitask-large-v0.5
/// to determine if it handles German text for both NER and RE.
///
/// Usage: eval-re <model_dir>
///   model_dir: path containing model.onnx + tokenizer.json

use composable::Composable;
use gliner::model::input::relation::schema::RelationSchema;
use gliner::model::input::text::TextInput;
use gliner::model::params::Parameters;
use gliner::model::pipeline::relation::RelationPipeline;
use gliner::model::pipeline::span::SpanMode;
use gliner::model::pipeline::token::TokenPipeline;
use gliner::model::GLiNER;
use orp::model::Model;
use orp::params::RuntimeParameters;
use orp::pipeline::Pipeline;
use std::time::Instant;

struct TestCase {
    id: &'static str,
    lang: &'static str,
    text: &'static str,
    expected_relations: Vec<(&'static str, &'static str, &'static str)>,
}

fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            id: "S0",
            lang: "EN",
            text: "Bill Gates is an American businessman who co-founded Microsoft.",
            expected_relations: vec![("Bill Gates", "founded", "Microsoft")],
        },
        TestCase {
            id: "S1",
            lang: "DE",
            text: "Tim Cook ist der CEO von Apple. Apple hat seinen Hauptsitz in Cupertino.",
            expected_relations: vec![
                ("Tim Cook", "works_at", "Apple"),
                ("Apple", "headquartered_in", "Cupertino"),
            ],
        },
        TestCase {
            id: "S2",
            lang: "DE",
            text: "Max arbeitet bei Siemens in Muenchen.",
            expected_relations: vec![
                ("Max", "works_at", "Siemens"),
                ("Siemens", "located_in", "Muenchen"),
            ],
        },
        TestCase {
            id: "S3",
            lang: "DE",
            text: "Angela Merkel war Bundeskanzlerin von Deutschland.",
            expected_relations: vec![("Angela Merkel", "leads", "Deutschland")],
        },
        TestCase {
            id: "S4",
            lang: "DE",
            text: "Putin und Zelensky verhandeln ueber den Konflikt in der Ukraine. NATO unterstuetzt die Ukraine mit HIMARS.",
            expected_relations: vec![("NATO", "supports", "Ukraine")],
        },
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: eval-re <model_dir>");
        eprintln!("  model_dir: path containing model.onnx + tokenizer.json");
        std::process::exit(1);
    }

    let model_dir = std::path::Path::new(&args[1]);
    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    if !model_path.exists() {
        eprintln!("ERROR: model.onnx not found in {}", model_dir.display());
        std::process::exit(1);
    }
    if !tokenizer_path.exists() {
        eprintln!("ERROR: tokenizer.json not found in {}", model_dir.display());
        std::process::exit(1);
    }

    let entity_labels: &[&str] =
        &["person", "company", "organization", "city", "country", "product"];

    let mut schema = RelationSchema::new();
    schema.push_with_allowed_labels("works_at", &["person"], &["company", "organization"]);
    schema.push_with_allowed_labels("founded", &["person"], &["company", "organization"]);
    schema.push_with_allowed_labels(
        "headquartered_in",
        &["company", "organization"],
        &["city", "country"],
    );
    schema.push_with_allowed_labels(
        "located_in",
        &["company", "organization"],
        &["city", "country"],
    );
    schema.push_with_allowed_labels(
        "leads",
        &["person"],
        &["company", "organization", "country"],
    );
    schema.push_with_allowed_labels(
        "supports",
        &["organization", "country", "person"],
        &["organization", "country", "person"],
    );
    schema.push_with_allowed_labels("born_in", &["person"], &["city", "country"]);
    schema.push_with_allowed_labels("member_of", &["person"], &["organization"]);

    println!("=== GLiNER-Multitask RE Evaluation ===");
    println!("Model: {}", model_dir.display());
    println!("Entity labels: {:?}", entity_labels);
    println!(
        "Relations: works_at, founded, headquartered_in, located_in, leads, supports, born_in, member_of"
    );
    println!();

    // --- Load model (shared between NER and RE) ---
    println!("Loading ONNX model...");
    let load_start = Instant::now();
    let runtime_params = RuntimeParameters::default();
    let ort_model = match Model::new(model_path.to_string_lossy().as_ref(), runtime_params) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ERROR loading model: {e}");
            std::process::exit(1);
        }
    };
    let params = Parameters::default();
    println!("Model loaded in {:.1}s", load_start.elapsed().as_secs_f64());

    // Also load via GLiNER<SpanMode> for standalone NER
    let ner_model = match GLiNER::<SpanMode>::new(
        Parameters::default(),
        RuntimeParameters::default(),
        tokenizer_path.to_string_lossy().as_ref(),
        model_path.to_string_lossy().as_ref(),
    ) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ERROR loading NER model: {e}");
            std::process::exit(1);
        }
    };
    println!("NER model loaded.");
    println!();

    let tokenizer_str = tokenizer_path.to_string_lossy();

    for tc in test_cases() {
        println!("--- {} ({}) ---", tc.id, tc.lang);
        println!("Input: \"{}\"", tc.text);

        // --- NER via GLiNER<SpanMode> ---
        let ner_input = match TextInput::from_str(&[tc.text], entity_labels) {
            Ok(i) => i,
            Err(e) => {
                println!("  NER input error: {e}");
                continue;
            }
        };

        let ner_start = Instant::now();
        match ner_model.inference(ner_input) {
            Ok(output) => {
                let ner_ms = ner_start.elapsed().as_secs_f64() * 1000.0;
                println!("  NER ({:.0}ms):", ner_ms);
                if output.spans.is_empty() || output.spans[0].is_empty() {
                    println!("    (none)");
                } else {
                    for span in &output.spans[0] {
                        let (start, end) = span.offsets();
                        println!(
                            "    {:20} | {:15} | {:.1}% | [{}-{}]",
                            span.text(),
                            span.class(),
                            span.probability() * 100.0,
                            start,
                            end
                        );
                    }
                }
            }
            Err(e) => println!("  NER error: {e}"),
        }

        // --- RE via composed TokenPipeline + RelationPipeline ---
        let re_input = match TextInput::from_str(&[tc.text], entity_labels) {
            Ok(i) => i,
            Err(e) => {
                println!("  RE input error: {e}");
                continue;
            }
        };

        let token_pipeline = match TokenPipeline::new(&*tokenizer_str) {
            Ok(p) => p,
            Err(e) => {
                println!("  TokenPipeline error: {e}");
                continue;
            }
        };
        let rel_pipeline = match RelationPipeline::default(&*tokenizer_str, &schema) {
            Ok(p) => p,
            Err(e) => {
                println!("  RelationPipeline error: {e}");
                continue;
            }
        };

        let re_start = Instant::now();

        // Step 1: NER pass (token pipeline)
        let ner_composable = token_pipeline.to_composable(&ort_model, &params);
        let span_output = match ner_composable.apply(re_input) {
            Ok(o) => o,
            Err(e) => {
                println!("  RE NER pass error: {e}");
                continue;
            }
        };

        // Step 2: RE pass (relation pipeline)
        let rel_composable = rel_pipeline.to_composable(&ort_model, &params);
        match rel_composable.apply(span_output) {
            Ok(output) => {
                let re_ms = re_start.elapsed().as_secs_f64() * 1000.0;
                println!("  RE ({:.0}ms):", re_ms);
                if output.relations.is_empty() || output.relations[0].is_empty() {
                    println!("    (none)");
                } else {
                    for rel in &output.relations[0] {
                        println!(
                            "    {:20} --[{:15}]--> {:20} | {:.1}%",
                            rel.subject(),
                            rel.class(),
                            rel.object(),
                            rel.probability() * 100.0
                        );
                    }
                }

                println!("  Expected:");
                for (s, r, o) in &tc.expected_relations {
                    println!("    {:20} --[{:15}]--> {:20}", s, r, o);
                }
            }
            Err(e) => println!("  RE error: {e}"),
        }
        println!();
    }

    println!("=== Evaluation complete ===");
}
