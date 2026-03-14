# Design: NER + NLI Relation Extraction (v1.2.0)

**Status:** NER migration COMPLETE, NLI backend COMPLETE, wizard/UI COMPLETE

---

## NER: GLiNER ONNX via gline-rs sidecar (COMPLETE)

### Problem

The `anno` crate with candle backend was unreliable for GLiNER NER:
SentencePiece unsupported, config.json bugs, no multilingual model worked.

### Solution

Replaced anno+candle with `gline-rs` (ONNX inference) as a long-running sidecar binary.
Sidecar avoids ort version conflict (gline-rs pins ort rc.9, engram uses rc.12).

### Architecture

```
engram.exe (ort rc.12 for embeddings)
  NER chain: RuleBasedNer -> GraphGazetteer -> GlinerBackend
                                                    |
                                               [long-running subprocess]
                                                    |
                                              engram-ner.exe (gline-rs, ort rc.9)
                                              Model loaded once at startup
```

### Default model

`knowledgator/gliner-x-small` (quantized ONNX, 173 MB, 20 languages, ~0.75 F1).

| Model | Quantized Size | Languages | Avg F1 |
|-------|---------------|-----------|--------|
| **knowledgator/gliner-x-small** | **173 MB** | 20 | ~0.75 | **Default** |
| onnx-community/gliner_multi-v2.1 | 349 MB (int8) | 6 | ~0.66 |
| knowledgator/gliner-x-base | 303 MB | 20 | ~0.75 |
| knowledgator/gliner-x-large | 610 MB | 20 | ~0.81 |

### What was done

- [x] `tools/engram-ner/` sidecar binary (long-running, model loaded once at startup)
- [x] `gliner_backend.rs` in engram-ingest (Extractor trait, Mutex-protected subprocess I/O)
- [x] `POST /config/ner-download` streams ONNX + tokenizer from HuggingFace
- [x] Wizard updated: 4 ONNX models, default gliner-x-small, variant auto-select
- [x] `anno` crate, `anno_backend.rs`, `hf-hub` dependency fully removed
- [x] Feature flag: `--features gliner` (replaces `--features anno`)
- [x] 630 tests pass, 0 failures

### Key files

| File | Purpose |
|------|---------|
| `tools/engram-ner/src/main.rs` | Sidecar: CLI arg model_dir, ready signal, JSON Lines |
| `crates/engram-ingest/src/gliner_backend.rs` | GlinerBackend, find_ner_model, find_ner_binary |
| `crates/engram-api/src/handlers.rs` | `#[cfg(feature = "gliner")]` in build_pipeline + download |

---

## NLI Relation Extraction via engram-rel sidecar (COMPLETE)

### Algorithm

For each text chunk processed by the ingest pipeline:

1. NER chain extracts entities: `["Tim Cook", "Apple", "Cupertino"]`
2. For each entity pair `(head, tail)` where `head != tail`:
   a. Extract the sentence containing both entities (premise)
   b. For each relation template:
      - Build hypothesis: e.g., `"Tim Cook works at Apple"`
      - Tokenize `(premise, hypothesis)` as NLI sentence pair
      - Run NLI model: softmax([entailment, neutral, contradiction])
   c. If entailment > threshold -> emit `CandidateRelation`
3. Deduplicate and merge with other RE backends in the chain

### NLI model

`MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli` (~428 MB ONNX, 100+ languages).
Downloaded to `~/.engram/models/rel/multilingual-MiniLMv2-L6-mnli-xnli/`.

### Verified (2026-03-14)

```
Input:  "Tim Cook is the CEO of Apple."
Entities: Tim Cook (person), Apple (organization)
Templates: works_at, holds_position

Output:
  Tim Cook -> Apple: holds_position (0.977)
  Tim Cook -> Apple: works_at (0.921)
```

### Complexity

- O(entity_pairs x relation_templates) NLI forward passes per chunk
- Typical: 5 entities -> 20 pairs x 21 templates = 420 calls
- MiniLM: ~2-5ms/call CPU -> ~1-2s per chunk
- Acceptable for batch ingest (not real-time)

### Default relation templates (21)

```json
{
  "works_at":           "{head} works at {tail}",
  "born_in":            "{head} was born in {tail}",
  "lives_in":           "{head} lives in {tail}",
  "educated_at":        "{head} was educated at {tail}",
  "spouse":             "{head} is married to {tail}",
  "parent_of":          "{head} is the parent of {tail}",
  "child_of":           "{head} is the child of {tail}",
  "citizen_of":         "{head} is a citizen of {tail}",
  "member_of":          "{head} is a member of {tail}",
  "holds_position":     "{head} holds the position of {tail}",
  "founded_by":         "{head} was founded by {tail}",
  "headquartered_in":   "{head}'s headquarters are in {tail}",
  "subsidiary_of":      "{head} is a subsidiary of {tail}",
  "acquired_by":        "{head} was acquired by {tail}",
  "located_in":         "{head} is located in {tail}",
  "instance_of":        "{head} is a {tail}",
  "part_of":            "{head} is part of {tail}",
  "capital_of":         "{head} is the capital of {tail}",
  "cause_of":           "{head} causes {tail}",
  "author_of":          "{head} was written by {tail}",
  "produces":           "{head} produces {tail}"
}
```

### Threshold tuning needed

Default threshold 0.5 produces too many false positives. Verified results show:
- Good relations: 0.92+ (holds_position, works_at, headquartered_in)
- Noise: 0.5-0.8 (e.g., "Cupertino" -> "Apple" -> "capital_of")
- **Recommended default: 0.7** (reduces noise significantly while keeping good relations)

### Key files

| File | Purpose |
|------|---------|
| `tools/engram-rel/src/main.rs` | Sidecar: NLI inference via ort, per-request |
| `crates/engram-ingest/src/rel_nli.rs` | NliRelBackend, find_nli_model, templates |
| `crates/engram-api/src/handlers.rs` | `#[cfg(feature = "nli-rel")]` in build_pipeline |

---

## DONE: Wizard + System UI for NLI (2026-03-14)

### Wizard (onboarding_wizard.rs) -- COMPLETE

| Task | Status |
|------|--------|
| NLI model download visible progress | DONE -- spinner with message during download |
| Threshold slider (0.30-0.95, default 0.9) | DONE -- saved as `rel_threshold` in config |
| General (21 templates) preset | DONE -- hardcoded defaults, works air-gapped |

### System settings (system.rs) -- COMPLETE

| Task | Status |
|------|--------|
| Threshold slider | DONE -- loads from config, saved with NER/RE config |
| Template import/export | DONE -- Export downloads JSON (includes learned relations), Import via file picker |
| Template editing | DONE -- JSON textarea with Reset to Defaults |
| NLI model install/status | DONE -- quick install + custom HF model |

### Backend (handlers.rs, state.rs) -- COMPLETE

| Task | Status |
|------|--------|
| `rel_threshold` in merge() | DONE -- bug fix, config updates now persist threshold |
| GET /config returns rel fields | DONE -- `rel_model`, `rel_threshold`, `relation_templates`, `coreference_enabled` |
| GET /config/relation-templates/export | DONE -- configured templates + learned relation types from gazetteer |
| POST /config/relation-templates/import | DONE -- merges templates, validates {head}/{tail}, invalidates rel cache |

### Template ecosystem (future)

- Domain preset packs: business, science, biomedical (only general shipped so far)
- RelationGazetteer learned relations auto-export as NLI templates (learned types included in export)
- Template sharing across engram mesh instances

---

## Coreference Resolution (DEFERRED)

Out of scope for v1.2.0. Research completed 2026-03-14, findings in `docs/roadmap.md`.

Best option when revisited: rule-based Rust (~200 lines, ~60-70% F1) as tier 1,
coref-onnx sidecar as tier 2. See roadmap for full analysis.
