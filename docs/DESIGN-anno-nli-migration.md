# Anno + NLI Migration Design

**Version:** 1.2.0
**Date:** 2026-03-13
**Status:** Active
**Prerequisite:** v1.1.0 (561 tests passing, all features compile clean)

---

## Problem Statement

The current NER/RE pipeline has three problems:

1. **ort version conflict**: `gline-rs` pins `ort 2.0.0-rc.9`, `engram-core` uses `ort 2.0.0-rc.12`.
   This forces subprocess isolation via separate `engram-ner.exe` and `engram-rel.exe` binaries.

2. **GLiREL is impractical**: 1.7GB FP32 model, English-only, CC BY-NC-SA license.

3. **Complexity**: 3 separate binaries, 2 separate model downloads, 2 separate configs,
   2 subprocess JSON Lines protocols.

## Solution

Replace the subprocess-based NER/RE with:

- **NER**: `anno` crate (candle backend, pure Rust ML) -- in-process, no ort dependency
- **RE**: NLI-based relation extraction via `ort 2.0.0-rc.12` -- subprocess (`engram-rel.exe`), avoids CRT conflict
- **Coref**: `anno` rule-based coreference (MentionRankingCoref + SimpleCorefResolver)

Single binary. No subprocess overhead. No ort version conflict. Multilingual.

---

## Research Summary (Step 0 -- completed 2026-03-13)

### Verified

| Question | Result |
|----------|--------|
| anno candle feature avoids ort? | Confirmed -- `cargo tree -i ort` shows only our rc.12 |
| anno candle + ort rc.12 coexist? | Confirmed -- compiles clean |
| GLiNER2Candle implements Model + ZeroShotNER + RelationExtractor? | Confirmed |
| NLI models available in ONNX on HuggingFace? | Confirmed (MiniLM, mDeBERTa) |
| NLI RE viable in Rust via ort? | Confirmed -- clean API, ~5ms/pair CPU |

### Issues Found & Resolved

| Issue | Resolution |
|-------|------------|
| anno v0.3.9 has compilation bug (missing `Language` import) | Use git rev `2c4a232` (fix merged Mar 12) |
| GLiNER2Candle RE is heuristic-only (trigger patterns) | Supplement with NLI RE (neural) |
| FCoref neural coref requires ONNX (ort rc.11) | Use rule-based coref (MentionRankingCoref + SimpleCorefResolver) |
| GLiREL is English-only | Dropped entirely -- NLI RE is multilingual |
| StackedNER::default() doesn't auto-discover candle backends | Manually compose with GLiNER2Candle |

### Decisions Made

1. **Drop GLiREL entirely** -- NLI RE is multilingual, smaller, permissive license, good enough accuracy
2. **NER in-process** (candle, no ort). RE in subprocess (`engram-rel.exe`, ort rc.12 + tokenizers)
3. **Rule-based coref only** -- MentionRankingCoref (always available) + SimpleCorefResolver (analysis feature)
4. **NLI model: MiniLM** -- multilingual-MiniLMv2-L6-mnli-xnli (~100MB, fast, clean ONNX, 100+ langs)
5. **anno git dependency** -- pin to rev `2c4a232` until v0.3.10 ships on crates.io

---

## Architecture

### Before (3 binaries, 2 models, ort version conflict)

```
engram.exe (ort rc.12 for embeddings)
  NER chain: RuleBasedNer -> GraphGazetteer -> AnnoBackend
                                                  |
                                                  v
                                          [subprocess stdin/stdout]
                                                  |
                                                  v
                                        engram-ner.exe (gline-rs, ort rc.9)

  RE chain: KbRelation -> RelGazetteer -> KGE -> GlirelBackend
                                                    |
                                                    v
                                            [subprocess stdin/stdout]
                                                    |
                                                    v
                                          engram-rel.exe (ort rc.9)
```

### After (NER in-process, RE via subprocess, no ort version conflict)

```
engram.exe (candle for NER/coref, ort rc.12 for embeddings only)

  NER chain: RuleBasedNer -> GraphGazetteer -> AnnoBackend (GLiNER2Candle, in-process)
                                                    |
                                                    v
                                               [coreference]
                                          MentionRankingCoref (rule-based, in-process)
                                          "He" -> "John", "the company" -> "Apple"

  RE chain: KbRelation -> RelGazetteer -> KGE -> NliRelBackend (subprocess)
                                                    |
                                                    v
                                          engram-rel.exe (ort rc.12 + tokenizers)
                                            For each (head, tail) entity pair:
                                              For each relation template:
                                                premise = sentence with entities
                                                hypothesis = "{head} works at {tail}"
                                                NLI model -> entailment probability
                                                if > threshold -> emit relation

Note: NLI RE uses a separate binary because `tokenizers` (via esaxx-rs, C++ static CRT)
conflicts with `ort_sys` (C++ dynamic CRT) on Windows MSVC in debug builds. NER is
the latency-sensitive operation (per-sentence) and runs in-process. RE runs once per
text chunk and subprocess overhead is negligible.
```

### Model Sizes

| Component | Model | Size | Languages |
|-----------|-------|------|-----------|
| NER | GLiNER multi-v2.1 (safetensors via candle) | ~400MB | 100+ |
| RE | multilingual-MiniLMv2-L6-mnli-xnli (ONNX via ort) | ~100MB | 100+ |
| Coref | Rule-based (no model) | 0 | all |
| **Total** | | **~500MB** | **100+** |

vs. before: GLiNER (~400MB ONNX) + GLiREL (~1.7GB) = **2.1GB, English-only RE**

### Dependency Changes

| Crate | Before | After |
|-------|--------|-------|
| anno (candle) | not used (empty feature flag) | `anno` git rev 2c4a232, `default-features = false`, `features = ["candle", "analysis"]` |
| ort | rc.12 (engram-core embeddings) | rc.12 (embeddings + NLI RE) -- no conflict |
| tokenizers | not in engram-ingest | 0.22 (for NLI tokenization) |
| ndarray | not in engram-ingest | 0.16 (for NLI tensor ops) |
| gline-rs | via engram-ner.exe subprocess | **removed** |
| engram-ner.exe | required for anno NER | **removed from default build** |
| engram-rel.exe | required for GLiREL | **removed from default build** |

---

## NLI-Based Relation Extraction

### Algorithm

For each text chunk processed by the ingest pipeline:

1. NER chain extracts entities: `["John", "Google", "2015"]`
2. Coreference resolves pronouns: `"He" -> "John"`
3. For each entity pair `(head, tail)` where `head != tail`:
   a. Extract the sentence containing both entities (premise)
   b. For each relation template in the template set:
      - Build hypothesis: e.g., `"John works at Google"`
      - Tokenize `(premise, hypothesis)` as NLI sentence pair
      - Run NLI model: get `[entailment, neutral, contradiction]` logits
      - Softmax -> entailment probability
   c. If entailment > threshold (default 0.5) -> emit `CandidateRelation`
4. Deduplicate and merge with other RE backends in the chain

### Complexity

- O(entity_pairs x relation_templates) NLI forward passes per chunk
- Typical: 5 entities -> 20 pairs x 21 templates = 420 calls
- MiniLM: ~2-5ms/call CPU -> ~1-2s per chunk
- Acceptable for ingest pipeline (not real-time, batch-oriented)

### Default Relation Templates (21)

Sourced from TACRED/FewRel/Wikidata research. EMNLP 2021 "Label Verbalization and Entailment"
reports 63% F1 zero-shot on TACRED with hand-crafted templates.

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

### Template Ecosystem

- Users can add/edit/delete templates in system settings
- Import from JSON file or URL (Rel2Text: 1,522 templates, TACRED: 42, FewRel: 100)
- Domain preset packs: general (default 21), business, science, biomedical
- RelationGazetteer learned relations auto-export as NLI templates
- Template sharing across engram mesh instances

---

## Coreference Resolution

### Architecture

Coreference runs after NER, before RE. It resolves pronouns and noun phrases to their
antecedent entities, improving RE input quality.

Two backends available (both rule-based, no model download needed):

| Backend | Feature gate | Approach | Strengths |
|---------|-------------|----------|-----------|
| `MentionRankingCoref` | none (always available) | Antecedent ranking with transitive clustering | Acronyms, clinical text, "be-phrase" patterns |
| `SimpleCorefResolver` | `analysis` | 9-sieve cascade (exact match, head match, pronoun, fuzzy) | Broad coverage, pronoun gender matching |

### Pipeline Integration

```
NER output: ["John Smith", "He", "Apple", "the company", "Steve Jobs"]
                |
                v
         Coreference resolution
                |
                v
Resolved: ["John Smith", "John Smith", "Apple", "Apple", "Steve Jobs"]
          (pronouns and noun phrases mapped to canonical entities)
                |
                v
         RE input (entity pairs use canonical names)
```

---

## Files to Modify

### Cargo Configuration

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `nli-rel` feature, update `anno` feature, remove `glirel` from `all` |
| `crates/engram-ingest/Cargo.toml` | Add `anno` crate (candle+analysis), add `ort`/`tokenizers`/`ndarray` behind `nli-rel` |
| `crates/engram-api/Cargo.toml` | Forward `nli-rel` feature |

### Backend Code

| File | Change |
|------|--------|
| `crates/engram-ingest/src/anno_backend.rs` | **REWRITE** -- replace subprocess with direct anno candle API + coreference |
| `crates/engram-ingest/src/rel_nli.rs` | **NEW** -- NLI-based relation extraction via ort rc.12 |
| `crates/engram-ingest/src/lib.rs` | Export `rel_nli` module, add re-exports |

### Wiring

| File | Change |
|------|--------|
| `crates/engram-api/src/state.rs` | Add `relation_templates`, `coreference_enabled` to EngineConfig |
| `crates/engram-api/src/handlers.rs` | Wire anno + NLI + coref in `build_pipeline()` |
| `crates/engram-api/src/mcp.rs` | Pass new config fields |
| `crates/engram-a2a/src/skill.rs` | Pass new config fields |

### Frontend (Leptos WASM)

| File | Change |
|------|--------|
| `crates/engram-ui/src/components/onboarding_wizard.rs` | New NER/RE model presets, template UI, coref toggle |
| `crates/engram-ui/src/pages/system.rs` | Template management, export, coref settings |

### Unchanged (learning intact)

| File | Why unchanged |
|------|--------------|
| `gazetteer.rs` | NER learning from graph -- backend-agnostic |
| `rel_gazetteer.rs` | REL learning from graph -- backend-agnostic |
| `ner_chain.rs` | Trait-based orchestrator -- agnostic |
| `rel_chain.rs` | Trait-based orchestrator -- agnostic |
| `pipeline.rs` | Orchestration layer -- agnostic |
| `rel_glirel.rs` | Kept but deprecated, removed from `all` features |

---

## Verification Criteria

1. `cargo build --features all` compiles without ort version conflict
2. `cargo tree -i ort` shows only one ort version (rc.12)
3. No `ort`/`onnxruntime` C++ libraries linked from anno (candle-only)
4. NER test: GLiNER2Candle extracts entities from English + German text
5. Coref test: "John founded Apple. He was born in 1955." -> "He" resolves to "John"
6. NLI RE test: given entities + text, produces `works_at` relation with entailment > 0.5
7. Template import/export works in UI
8. Dashboard stats show nodes + connections
9. `trunk build` compiles frontend
10. Full test suite passes (561+ tests)
11. Old `engram-ner.exe`/`engram-rel.exe` still compile separately (not broken, just not in default build)

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| anno crate abandoned (195 downloads, single author) | Pin git rev, have exit strategy: fork or extract GLiNER2Candle code (MIT license) |
| anno breaks on update | Pinned to specific rev, manual review before bumping |
| candle inference slower than ONNX | Acceptable for batch ingest pipeline; if needed, add optional ONNX path later |
| NLI RE accuracy insufficient (63% F1 zero-shot) | Templates are tunable; RelationGazetteer learns from corrections; KGE handles known relations |
| MiniLM NLI model not available in ONNX | Export ourselves: `optimum-cli export onnx --model MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli --task text-classification` |
| Large candle dependency tree | 22 direct deps for anno-lib -- acceptable, all well-known crates (candle, tokenizers, hf-hub, safetensors) |

---

## Open Items (to resolve during implementation)

- [ ] Verify GLiNER multi-v2.1 safetensors available on HuggingFace for candle loading
- [ ] Test MentionRankingCoref quality on real ingest text
- [ ] Benchmark candle NER inference speed vs previous ONNX subprocess approach
- [ ] Export MiniLM NLI model to ONNX and verify inference correctness
- [ ] Determine if anno v0.3.10 ships on crates.io (switch from git dep when it does)
