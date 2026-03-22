//! GLiNER2 in-process NER + RE backend via ONNX Runtime.
//!
//! Replaces both `engram-ner` (gline-rs sidecar) and `engram-rel` (NLI sidecar)
//! with a single unified model: `fastino/gliner2-multi-v1`.
//!
//! Architecture:
//!   4 ONNX models: encoder (mDeBERTa), span_rep, count_embed, count_pred
//!   Tokenizer: HuggingFace tokenizer (sentencepiece)
//!
//! NER: schema `( [P] entities ( [E] person [E] company ... ) ) [SEP_TEXT] <text>`
//! RE:  schema `( [P] rel_name ( [R] head [R] tail ) ) [SEP_TEXT] <text>`

#[cfg(feature = "gliner2")]
use ndarray::Array2;
#[cfg(feature = "gliner2")]
use ort::session::Session;
#[cfg(feature = "gliner2")]
use std::path::{Path, PathBuf};
#[cfg(feature = "gliner2")]
use std::sync::Mutex;

/// Configuration loaded from `gliner2_config.json`.
#[derive(Debug, Clone)]
pub struct Gliner2Config {
    pub model_dir: PathBuf,
    pub max_width: usize,
    pub hidden_size: usize,
    pub special_tokens: SpecialTokens,
    pub encoder_file: String,
    pub span_rep_file: String,
    pub count_embed_file: String,
    pub count_pred_file: String,
    pub classifier_file: String,
}

#[derive(Debug, Clone)]
pub struct SpecialTokens {
    pub p: u32,          // [P]
    pub l: u32,          // [L]
    pub e: u32,          // [E]
    pub r: u32,          // [R]
    pub sep_struct: u32, // [SEP_STRUCT] -- between schema blocks
    pub sep_text: u32,   // [SEP_TEXT] -- after last schema, before text
}

/// A detected entity span from GLiNER2.
#[derive(Debug, Clone)]
pub struct Gliner2Entity {
    pub text: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub score: f32,
}

/// A detected relation from GLiNER2.
#[derive(Debug, Clone)]
pub struct Gliner2Relation {
    pub head: String,
    pub tail: String,
    pub label: String,
    pub head_start: usize,
    pub head_end: usize,
    pub tail_start: usize,
    pub tail_end: usize,
    pub head_score: f32,
    pub tail_score: f32,
}

/// Intermediate: tokenized text with word offsets.
#[cfg(feature = "gliner2")]
struct TokenizedText {
    word_offsets: Vec<(usize, usize)>,
    first_token_positions: Vec<usize>,
    token_ids: Vec<i64>,
    _text_token_count: usize,
}

/// Intermediate: scored span above threshold.
#[cfg(feature = "gliner2")]
#[derive(Debug, Clone)]
struct ScoredSpan {
    text: String,
    start: usize,
    end: usize,
    score: f32,
}

#[cfg(feature = "gliner2")]
pub struct Gliner2Backend {
    config: Gliner2Config,
    encoder: Session,
    span_rep: Session,
    count_embed: Session,
    #[allow(dead_code)]
    count_pred: Session,
    #[allow(dead_code)]
    classifier: Session,
    tokenizer: tokenizers::Tokenizer,
    word_re: regex::Regex,
}

#[cfg(feature = "gliner2")]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(feature = "gliner2")]
impl Gliner2Backend {
    /// Load a GLiNER2 model from a directory containing ONNX files + config.
    pub fn load(model_dir: &Path, variant: &str) -> Result<Self, String> {
        let config_path = model_dir.join("gliner2_config.json");
        let config_str =
            std::fs::read_to_string(&config_path).map_err(|e| format!("read config: {e}"))?;
        let config_json: serde_json::Value =
            serde_json::from_str(&config_str).map_err(|e| format!("parse config: {e}"))?;

        let max_width = config_json["max_width"].as_u64().unwrap_or(8) as usize;
        let hidden_size = config_json["hidden_size"].as_u64().unwrap_or(768) as usize;

        let st = &config_json["special_tokens"];
        let special_tokens = SpecialTokens {
            p: st["[P]"].as_u64().unwrap_or(0) as u32,
            l: st["[L]"].as_u64().unwrap_or(0) as u32,
            e: st["[E]"].as_u64().unwrap_or(0) as u32,
            r: st.get("[R]").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            sep_struct: st.get("[SEP_STRUCT]").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            sep_text: st["[SEP_TEXT]"].as_u64().unwrap_or(0) as u32,
        };

        let files = &config_json["onnx_files"][variant];
        let encoder_file = files["encoder"].as_str().unwrap_or("encoder.onnx").to_string();
        let span_rep_file = files["span_rep"].as_str().unwrap_or("span_rep.onnx").to_string();
        let count_embed_file = files["count_embed"]
            .as_str()
            .unwrap_or("count_embed.onnx")
            .to_string();
        let count_pred_file = files
            .get("count_pred")
            .and_then(|v| v.as_str())
            .unwrap_or("count_pred.onnx")
            .to_string();
        let classifier_file = files
            .get("classifier")
            .and_then(|v| v.as_str())
            .unwrap_or("classifier.onnx")
            .to_string();

        let config = Gliner2Config {
            model_dir: model_dir.to_path_buf(),
            max_width,
            hidden_size,
            special_tokens,
            encoder_file,
            span_rep_file,
            count_embed_file,
            count_pred_file,
            classifier_file,
        };

        // Scale intra-op threads to available cores. Important for FP16 hybrid
        // models where Cast(FP16->FP32) adds per-op overhead that benefits from
        // parallelization.
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        // commit_from_file handles external data files (.onnx.data) automatically.
        // For FP16 hybrid models, ort auto-casts FP16 weights to FP32 via Cast nodes
        // on CPUs without native FP16 (pre-AVX-512_FP16). No precision loss, half the
        // disk/download size.
        let load_session = |filename: &str| -> Result<Session, String> {
            let path = model_dir.join(filename);
            Session::builder()
                .map_err(|e| format!("session builder: {e}"))?
                .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
                .map_err(|e| format!("opt level: {e}"))?
                .with_intra_threads(num_threads)
                .map_err(|e| format!("threads: {e}"))?
                .commit_from_file(&path)
                .map_err(|e| format!("load {filename}: {e}"))
        };

        let encoder = load_session(&config.encoder_file)?;
        let span_rep = load_session(&config.span_rep_file)?;
        let count_embed = load_session(&config.count_embed_file)?;
        let count_pred = load_session(&config.count_pred_file)?;
        let classifier = load_session(&config.classifier_file)?;

        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("load tokenizer: {e}"))?;

        let word_re = regex::Regex::new(
            r"(?xi)
            (?:https?://[^\s]+|www\.[^\s]+)  |
            [a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}  |
            @[a-z0-9_]+  |
            \w+(?:[-_]\w+)*  |
            \S
            ",
        )
        .expect("word pattern regex");

        Ok(Self {
            config,
            encoder,
            span_rep,
            count_embed,
            count_pred,
            classifier,
            tokenizer,
            word_re,
        })
    }

    // ---------------------------------------------------------------
    // Shared helpers
    // ---------------------------------------------------------------

    fn tokenize_word(&self, word: &str) -> Vec<u32> {
        let encoding = self.tokenizer.encode(word, false).unwrap();
        encoding.get_ids().to_vec()
    }

    /// Build schema prefix: `( [P] task_name ( [marker] label1 [marker] label2 ... ) ) [SEP_TEXT]`
    fn build_schema(
        &self,
        task_name: &str,
        labels: &[&str],
        marker_token_id: u32,
    ) -> (Vec<i64>, Vec<usize>) {
        let mut tokens: Vec<i64> = Vec::new();
        let mut label_positions: Vec<usize> = Vec::new();

        // ( [P] task_name (
        tokens.extend(self.tokenize_word("(").iter().map(|&id| id as i64));
        tokens.push(self.config.special_tokens.p as i64);
        tokens.extend(
            self.tokenize_word(task_name)
                .iter()
                .map(|&id| id as i64),
        );
        tokens.extend(self.tokenize_word("(").iter().map(|&id| id as i64));

        // [marker] label1 [marker] label2 ...
        for label in labels {
            label_positions.push(tokens.len());
            tokens.push(marker_token_id as i64);
            tokens.extend(self.tokenize_word(label).iter().map(|&id| id as i64));
        }

        // ) ) [SEP_TEXT]
        let close = self.tokenize_word(")");
        tokens.extend(close.iter().map(|&id| id as i64));
        tokens.extend(close.iter().map(|&id| id as i64));
        tokens.push(self.config.special_tokens.sep_text as i64);

        (tokens, label_positions)
    }

    /// Tokenize text into words, returning word offsets and token IDs.
    fn tokenize_text(&self, text: &str) -> TokenizedText {
        let text_lower = text.to_lowercase();
        let mut word_offsets: Vec<(usize, usize)> = Vec::new();
        let mut first_token_positions: Vec<usize> = Vec::new();
        let mut token_ids: Vec<i64> = Vec::new();
        let mut token_idx: usize = 0;

        for mat in self.word_re.find_iter(&text_lower) {
            word_offsets.push((mat.start(), mat.end()));
            first_token_positions.push(token_idx);

            let word_tokens = self.tokenize_word(mat.as_str());
            token_ids.extend(word_tokens.iter().map(|&id| id as i64));
            token_idx += word_tokens.len();
        }

        TokenizedText {
            word_offsets,
            first_token_positions,
            token_ids,
            _text_token_count: token_idx,
        }
    }

    /// Run encoder on input_ids, return flat hidden states.
    fn encode(&mut self, input_ids: &[i64]) -> Result<Vec<f32>, String> {
        let seq_len = input_ids.len();
        let ids_array = Array2::from_shape_vec((1, seq_len), input_ids.to_vec())
            .map_err(|e| format!("ids shape: {e}"))?;
        let mask_array = Array2::from_shape_vec((1, seq_len), vec![1i64; seq_len])
            .map_err(|e| format!("mask shape: {e}"))?;

        let ids_tensor =
            ort::value::Tensor::from_array(ids_array).map_err(|e| format!("ids tensor: {e}"))?;
        let mask_tensor = ort::value::Tensor::from_array(mask_array)
            .map_err(|e| format!("mask tensor: {e}"))?;

        let outputs = self
            .encoder
            .run(ort::inputs![
                "input_ids" => ids_tensor,
                "attention_mask" => mask_tensor,
            ])
            .map_err(|e| format!("encoder run: {e}"))?;

        let hidden = outputs.get("hidden_state").ok_or("no hidden_state output")?;
        let (_, data) = hidden
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract hidden: {e}"))?;

        Ok(data.to_vec())
    }

    /// Transform label embeddings via count_embed.
    fn transform_labels(&mut self, label_embs: Vec<f32>, num_labels: usize) -> Result<Vec<f32>, String> {
        let hidden_size = self.config.hidden_size;
        let label_embs_array = Array2::from_shape_vec((num_labels, hidden_size), label_embs)
            .map_err(|e| format!("label embs shape: {e}"))?;
        let label_tensor = ort::value::Tensor::from_array(label_embs_array)
            .map_err(|e| format!("label tensor: {e}"))?;

        let outputs = self
            .count_embed
            .run(ort::inputs!["label_embeddings" => label_tensor])
            .map_err(|e| format!("count_embed run: {e}"))?;

        let transformed = outputs.values().next().ok_or("no count_embed output")?;
        let (_, data) = transformed
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract transformed: {e}"))?;

        Ok(data.to_vec())
    }

    /// Compute span representations from word-level hidden states.
    /// Uses word indices (0..num_words) for span start/end, matching the
    /// Python model's word-level first-token pooling.
    fn compute_word_span_reps(
        &mut self,
        word_hidden: Vec<f32>,
        num_words: usize,
        _word_offsets: &[(usize, usize)],
    ) -> Result<(Vec<f32>, Vec<i64>, Vec<i64>), String> {
        let hidden_size = self.config.hidden_size;

        // Generate spans using WORD indices (not token indices)
        let mut span_starts: Vec<i64> = Vec::new();
        let mut span_ends: Vec<i64> = Vec::new();

        for i in 0..num_words {
            for j in 0..self.config.max_width.min(num_words - i) {
                span_starts.push(i as i64);
                span_ends.push((i + j) as i64);
            }
        }

        let num_spans = span_starts.len();
        if num_spans == 0 {
            return Ok((Vec::new(), Vec::new(), Vec::new()));
        }

        let word_hidden_array =
            ndarray::Array3::from_shape_vec((1, num_words, hidden_size), word_hidden)
                .map_err(|e| format!("word hidden shape: {e}"))?;
        let starts_array = Array2::from_shape_vec((1, num_spans), span_starts.clone())
            .map_err(|e| format!("starts shape: {e}"))?;
        let ends_array = Array2::from_shape_vec((1, num_spans), span_ends.clone())
            .map_err(|e| format!("ends shape: {e}"))?;

        let wh_tensor = ort::value::Tensor::from_array(word_hidden_array)
            .map_err(|e| format!("word hidden tensor: {e}"))?;
        let ss_tensor = ort::value::Tensor::from_array(starts_array)
            .map_err(|e| format!("starts tensor: {e}"))?;
        let se_tensor = ort::value::Tensor::from_array(ends_array)
            .map_err(|e| format!("ends tensor: {e}"))?;

        let outputs = self
            .span_rep
            .run(ort::inputs![
                "hidden_states" => wh_tensor,
                "span_start_idx" => ss_tensor,
                "span_end_idx" => se_tensor,
            ])
            .map_err(|e| format!("span_rep run: {e}"))?;

        let span_rep_val = outputs.values().next().ok_or("no span_rep output")?;
        let (_, span_rep_data) = span_rep_val
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract span_rep: {e}"))?;

        Ok((span_rep_data.to_vec(), span_starts, span_ends))
    }

    /// Score spans against label embeddings, collect those above threshold.
    /// Uses rayon for parallel dot product computation across spans.
    fn score_spans(
        &self,
        span_rep_flat: &[f32],
        transformed_flat: &[f32],
        label_idx: usize,
        word_offsets: &[(usize, usize)],
        text: &str,
        threshold: f32,
    ) -> Vec<ScoredSpan> {
        use rayon::prelude::*;

        let hidden_size = self.config.hidden_size;
        let num_words = word_offsets.len();
        let max_width = self.config.max_width;
        let label_vec = &transformed_flat[label_idx * hidden_size..(label_idx + 1) * hidden_size];

        // Build span index -> (word_start, word_end) mapping
        let mut span_words: Vec<(usize, usize)> = Vec::new();
        for i in 0..num_words {
            for j in 0..max_width.min(num_words - i) {
                span_words.push((i, i + j));
            }
        }

        let num_total_spans = span_rep_flat.len() / hidden_size;
        let num_spans = span_words.len().min(num_total_spans);

        // Parallel scoring via rayon
        let mut spans: Vec<ScoredSpan> = span_words[..num_spans]
            .par_iter()
            .enumerate()
            .filter_map(|(span_idx, &(word_start, word_end))| {
                let span_vec = &span_rep_flat[span_idx * hidden_size..(span_idx + 1) * hidden_size];
                let dot: f32 = span_vec.iter().zip(label_vec.iter()).map(|(a, b)| a * b).sum();
                let score = sigmoid(dot);

                if score >= threshold {
                    let start_char = word_offsets[word_start].0;
                    let end_char = word_offsets[word_end].1;
                    Some(ScoredSpan {
                        text: text[start_char..end_char].to_string(),
                        start: start_char,
                        end: end_char,
                        score,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        spans.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        spans
    }

    /// Core inference: encode text with schema, get span representations + label embeddings.
    fn run_inference(
        &mut self,
        schema_tokens: Vec<i64>,
        label_positions: &[usize],
        tokenized: &TokenizedText,
    ) -> Result<(Vec<f32>, Vec<f32>), String> {
        let hidden_size = self.config.hidden_size;
        let text_start_idx = schema_tokens.len();
        let num_labels = label_positions.len();

        // Concatenate schema + text tokens
        let mut all_tokens = schema_tokens;
        all_tokens.extend_from_slice(&tokenized.token_ids);

        // Run encoder
        let hidden_flat = self.encode(&all_tokens)?;

        // Extract label embeddings from marker positions
        let mut label_embs = vec![0.0f32; num_labels * hidden_size];
        for (i, &pos) in label_positions.iter().enumerate() {
            let src = pos * hidden_size;
            let dst = i * hidden_size;
            label_embs[dst..dst + hidden_size]
                .copy_from_slice(&hidden_flat[src..src + hidden_size]);
        }

        // Extract WORD-LEVEL hidden states (first-token pooling per word).
        // The Python model uses processor.extract_embeddings_from_batch() which
        // returns one embedding per word, not per subword token.
        let num_words = tokenized.word_offsets.len();
        let mut word_hidden = vec![0.0f32; num_words * hidden_size];
        for (w, &first_tok) in tokenized.first_token_positions.iter().enumerate() {
            let src = (text_start_idx + first_tok) * hidden_size;
            let dst = w * hidden_size;
            word_hidden[dst..dst + hidden_size]
                .copy_from_slice(&hidden_flat[src..src + hidden_size]);
        }

        // Transform label embeddings
        let transformed = self.transform_labels(label_embs, num_labels)?;

        // Compute span representations using word-level hidden states.
        // Span indices are word-level (0..num_words), not token-level.
        let (span_reps, _, _) = self.compute_word_span_reps(
            word_hidden,
            num_words,
            &tokenized.word_offsets,
        )?;

        Ok((span_reps, transformed))
    }

    // ---------------------------------------------------------------
    // NER
    // ---------------------------------------------------------------

    /// Extract named entities from text.
    pub fn extract_entities(
        &mut self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> Result<Vec<Gliner2Entity>, String> {
        if text.trim().is_empty() || labels.is_empty() {
            return Ok(Vec::new());
        }

        let tokenized = self.tokenize_text(text);
        if tokenized.word_offsets.is_empty() {
            return Ok(Vec::new());
        }

        // Build NER schema with [E] markers
        let (schema_tokens, label_positions) =
            self.build_schema("entities", labels, self.config.special_tokens.e);

        // Run inference
        let (span_reps, transformed) =
            self.run_inference(schema_tokens, &label_positions, &tokenized)?;

        // Score spans for each label
        let mut entities: Vec<Gliner2Entity> = Vec::new();
        for (label_idx, label) in labels.iter().enumerate() {
            let spans = self.score_spans(
                &span_reps,
                &transformed,
                label_idx,
                &tokenized.word_offsets,
                text,
                threshold,
            );
            for span in spans {
                entities.push(Gliner2Entity {
                    text: span.text,
                    label: label.to_string(),
                    start: span.start,
                    end: span.end,
                    score: span.score,
                });
            }
        }

        // Deduplicate: keep highest-scoring per overlapping span+label
        entities
            .sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let mut kept: Vec<Gliner2Entity> = Vec::new();
        for entity in entities {
            let overlaps = kept.iter().any(|k| {
                k.label == entity.label && entity.start < k.end && entity.end > k.start
            });
            if !overlaps {
                kept.push(entity);
            }
        }

        Ok(kept)
    }

    // ---------------------------------------------------------------
    // Relation Extraction
    // ---------------------------------------------------------------

    /// Build combined schema for multiple relation types in a single encoder pass.
    ///
    /// Layout: `( [P] rel1 ( [R] head [R] tail ) ) [SEP_STRUCT]
    ///          ( [P] rel2 ( [R] head [R] tail ) ) [SEP_STRUCT]
    ///          ...
    ///          ( [P] relN ( [R] head [R] tail ) ) [SEP_TEXT]`
    ///
    /// Returns (tokens, per-relation label positions as [(head_pos, tail_pos), ...])
    fn build_multi_relation_schema(
        &self,
        relation_types: &[&str],
    ) -> (Vec<i64>, Vec<(usize, usize)>) {
        let marker = self.config.special_tokens.r;
        let mut tokens: Vec<i64> = Vec::new();
        let mut rel_positions: Vec<(usize, usize)> = Vec::new();

        for (i, &rel_type) in relation_types.iter().enumerate() {
            let is_last = i == relation_types.len() - 1;

            // ( [P] rel_name (
            tokens.extend(self.tokenize_word("(").iter().map(|&id| id as i64));
            tokens.push(self.config.special_tokens.p as i64);
            tokens.extend(self.tokenize_word(rel_type).iter().map(|&id| id as i64));
            tokens.extend(self.tokenize_word("(").iter().map(|&id| id as i64));

            // [R] head [R] tail
            let head_pos = tokens.len();
            tokens.push(marker as i64);
            tokens.extend(self.tokenize_word("head").iter().map(|&id| id as i64));
            let tail_pos = tokens.len();
            tokens.push(marker as i64);
            tokens.extend(self.tokenize_word("tail").iter().map(|&id| id as i64));

            rel_positions.push((head_pos, tail_pos));

            // ) )
            let close = self.tokenize_word(")");
            tokens.extend(close.iter().map(|&id| id as i64));
            tokens.extend(close.iter().map(|&id| id as i64));

            // [SEP_STRUCT] between blocks, [SEP_TEXT] after last
            if is_last {
                tokens.push(self.config.special_tokens.sep_text as i64);
            } else {
                tokens.push(self.config.special_tokens.sep_struct as i64);
            }
        }

        (tokens, rel_positions)
    }

    /// Extract relations from text using a single combined encoder pass.
    ///
    /// All relation types are processed together so the model can disambiguate
    /// (e.g., "supports" wins over "born_in" for NATO/Ukraine).
    pub fn extract_relations(
        &mut self,
        text: &str,
        relation_types: &[&str],
        threshold: f32,
    ) -> Result<Vec<Gliner2Relation>, String> {
        if text.trim().is_empty() || relation_types.is_empty() {
            return Ok(Vec::new());
        }

        let tokenized = self.tokenize_text(text);
        if tokenized.word_offsets.is_empty() {
            return Ok(Vec::new());
        }

        // Build combined schema with all relation types
        let (schema_tokens, rel_positions) = self.build_multi_relation_schema(relation_types);
        let hidden_size = self.config.hidden_size;
        let text_start_idx = schema_tokens.len();

        // Concatenate schema + text tokens
        let mut all_tokens = schema_tokens;
        all_tokens.extend_from_slice(&tokenized.token_ids);

        // Single encoder pass for all relation types
        let hidden_flat = self.encode(&all_tokens)?;

        // Extract word-level hidden states (first-token pooling)
        let num_words = tokenized.word_offsets.len();
        let mut word_hidden = vec![0.0f32; num_words * hidden_size];
        for (w, &first_tok) in tokenized.first_token_positions.iter().enumerate() {
            let src = (text_start_idx + first_tok) * hidden_size;
            let dst = w * hidden_size;
            word_hidden[dst..dst + hidden_size]
                .copy_from_slice(&hidden_flat[src..src + hidden_size]);
        }

        // Compute span representations (shared across all relation types)
        let (span_reps, _, _) =
            self.compute_word_span_reps(word_hidden, num_words, &tokenized.word_offsets)?;

        let mut relations: Vec<Gliner2Relation> = Vec::new();

        // Score each relation type using its head/tail embeddings
        for (rel_idx, &rel_type) in relation_types.iter().enumerate() {
            let (head_pos, tail_pos) = rel_positions[rel_idx];

            // Extract head/tail embeddings from encoder hidden states
            let mut label_embs = vec![0.0f32; 2 * hidden_size];
            label_embs[..hidden_size]
                .copy_from_slice(&hidden_flat[head_pos * hidden_size..(head_pos + 1) * hidden_size]);
            label_embs[hidden_size..]
                .copy_from_slice(&hidden_flat[tail_pos * hidden_size..(tail_pos + 1) * hidden_size]);

            // Transform via count_embed
            let transformed = self.transform_labels(label_embs, 2)?;

            // Score spans for head and tail
            let head_spans = self.score_spans(
                &span_reps, &transformed, 0, &tokenized.word_offsets, text, threshold,
            );
            let tail_spans = self.score_spans(
                &span_reps, &transformed, 1, &tokenized.word_offsets, text, threshold,
            );

            // Emit relation for best non-overlapping head + tail pair
            if let (Some(head), Some(tail)) = (head_spans.first(), tail_spans.first()) {
                if head.start < tail.end && head.end > tail.start {
                    // Overlapping: try next-best tail
                    if let Some(alt_tail) = tail_spans
                        .iter()
                        .find(|t| !(head.start < t.end && head.end > t.start))
                    {
                        relations.push(Gliner2Relation {
                            head: head.text.clone(),
                            tail: alt_tail.text.clone(),
                            label: rel_type.to_string(),
                            head_start: head.start,
                            head_end: head.end,
                            tail_start: alt_tail.start,
                            tail_end: alt_tail.end,
                            head_score: head.score,
                            tail_score: alt_tail.score,
                        });
                    }
                } else {
                    relations.push(Gliner2Relation {
                        head: head.text.clone(),
                        tail: tail.text.clone(),
                        label: rel_type.to_string(),
                        head_start: head.start,
                        head_end: head.end,
                        tail_start: tail.start,
                        tail_end: tail.end,
                        head_score: head.score,
                        tail_score: tail.score,
                    });
                }
            }
        }

        Ok(relations)
    }

    /// Combined NER + RE extraction in one call.
    /// Runs NER first, then RE. Returns both results.
    pub fn extract_all(
        &mut self,
        text: &str,
        entity_labels: &[&str],
        relation_types: &[&str],
        ner_threshold: f32,
        re_threshold: f32,
    ) -> Result<(Vec<Gliner2Entity>, Vec<Gliner2Relation>), String> {
        let entities = self.extract_entities(text, entity_labels, ner_threshold)?;
        let relations = self.extract_relations(text, relation_types, re_threshold)?;
        Ok((entities, relations))
    }
}

// ---------------------------------------------------------------
// Pipeline trait implementations
// ---------------------------------------------------------------

/// Thread-safe wrapper for GLiNER2 that implements pipeline traits.
/// Holds the backend behind a Mutex so `&self` trait methods work.
#[cfg(feature = "gliner2")]
pub struct Gliner2PipelineBackend {
    inner: Mutex<Gliner2Backend>,
    entity_labels: Vec<String>,
    relation_types: Vec<String>,
    ner_threshold: f32,
    re_threshold: f32,
}

#[cfg(feature = "gliner2")]
impl Gliner2PipelineBackend {
    pub fn new(
        backend: Gliner2Backend,
        entity_labels: Vec<String>,
        relation_types: Vec<String>,
        ner_threshold: f32,
        re_threshold: f32,
    ) -> Self {
        Self {
            inner: Mutex::new(backend),
            entity_labels,
            relation_types,
            ner_threshold,
            re_threshold,
        }
    }
}

#[cfg(feature = "gliner2")]
impl crate::traits::Extractor for Gliner2PipelineBackend {
    fn extract(
        &self,
        text: &str,
        _lang: &crate::types::DetectedLanguage,
    ) -> Vec<crate::types::ExtractedEntity> {
        let labels: Vec<&str> = self.entity_labels.iter().map(|s| s.as_str()).collect();
        let mut backend = match self.inner.lock() {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("gliner2 lock poisoned: {e}");
                return Vec::new();
            }
        };
        match backend.extract_entities(text, &labels, self.ner_threshold) {
            Ok(entities) => entities
                .into_iter()
                .map(|e| crate::types::ExtractedEntity {
                    text: e.text,
                    entity_type: e.label,
                    span: (e.start, e.end),
                    confidence: e.score,
                    method: crate::types::ExtractionMethod::StatisticalModel,
                    language: String::new(),
                    resolved_to: None,
                })
                .collect(),
            Err(e) => {
                tracing::error!("gliner2 NER error: {e}");
                Vec::new()
            }
        }
    }

    fn name(&self) -> &str {
        "gliner2-onnx"
    }

    fn method(&self) -> crate::types::ExtractionMethod {
        crate::types::ExtractionMethod::StatisticalModel
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![] // multilingual -- supports all
    }
}

#[cfg(feature = "gliner2")]
impl crate::rel_traits::RelationExtractor for Gliner2PipelineBackend {
    fn extract_relations(
        &self,
        input: &crate::rel_traits::RelationExtractionInput,
    ) -> Vec<crate::rel_traits::CandidateRelation> {
        if self.relation_types.is_empty() || input.entities.len() < 2 {
            return Vec::new();
        }

        let rel_types: Vec<&str> = self.relation_types.iter().map(|s| s.as_str()).collect();
        let mut backend = match self.inner.lock() {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("gliner2 lock poisoned: {e}");
                return Vec::new();
            }
        };

        let relations = match backend.extract_relations(&input.text, &rel_types, self.re_threshold)
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("gliner2 RE error: {e}");
                return Vec::new();
            }
        };

        // Map head/tail text back to entity indices (exact match + span fallback).
        // When entities have span=(0,0) (seed flow), use case-insensitive substring matching.
        relations
            .into_iter()
            .filter_map(|rel| {
                let head_idx = input.entities.iter().position(|e| {
                    if e.text == rel.head {
                        return true;
                    }
                    if e.span != (0, 0) && e.span == (rel.head_start, rel.head_end) {
                        return true;
                    }
                    if e.span == (0, 0) {
                        let ext = rel.head.to_lowercase();
                        let ent = e.text.to_lowercase();
                        return ext.contains(&ent) || ent.contains(&ext);
                    }
                    false
                });
                let tail_idx = input.entities.iter().position(|e| {
                    if e.text == rel.tail {
                        return true;
                    }
                    if e.span != (0, 0) && e.span == (rel.tail_start, rel.tail_end) {
                        return true;
                    }
                    if e.span == (0, 0) {
                        let ext = rel.tail.to_lowercase();
                        let ent = e.text.to_lowercase();
                        return ext.contains(&ent) || ent.contains(&ext);
                    }
                    false
                });

                match (head_idx, tail_idx) {
                    (Some(h), Some(t)) => Some(crate::rel_traits::CandidateRelation {
                        head_idx: h,
                        tail_idx: t,
                        rel_type: rel.label,
                        confidence: (rel.head_score + rel.tail_score) / 2.0,
                        method: crate::types::ExtractionMethod::StatisticalModel,
                    }),
                    _ => {
                        tracing::debug!(
                            "gliner2 RE: could not map head='{}' or tail='{}' to entity index",
                            rel.head,
                            rel.tail
                        );
                        None
                    }
                }
            })
            .collect()
    }

    fn name(&self) -> &str {
        "gliner2-onnx"
    }
}

// ---------------------------------------------------------------
// Discovery helpers (no feature gate needed)
// ---------------------------------------------------------------

fn dirs_next_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\Users\\default"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}

/// Find the installed GLiNER2 model directory.
pub fn find_gliner2_model() -> Option<Gliner2Config> {
    let model_dir = dirs_next_home()
        .join(".engram")
        .join("models")
        .join("gliner2")
        .join("gliner2-multi-v1");

    if model_dir.join("gliner2_config.json").exists() {
        Some(Gliner2Config {
            model_dir,
            max_width: 8,
            hidden_size: 768,
            special_tokens: SpecialTokens {
                p: 0,
                l: 0,
                e: 0,
                r: 0,
                sep_struct: 0,
                sep_text: 0,
            },
            encoder_file: String::new(),
            span_rep_file: String::new(),
            count_embed_file: String::new(),
            count_pred_file: String::new(),
            classifier_file: String::new(),
        })
    } else {
        None
    }
}

/// List installed GLiNER2 models.
pub fn list_installed_gliner2_models() -> Vec<String> {
    let models_dir = dirs_next_home()
        .join(".engram")
        .join("models")
        .join("gliner2");

    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            if entry.path().join("gliner2_config.json").exists() {
                if let Some(name) = entry.file_name().to_str() {
                    models.push(name.to_string());
                }
            }
        }
    }
    models
}
