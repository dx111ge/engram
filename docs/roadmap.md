# Engram Roadmap

**Last updated:** 2026-03-13

---

## Completed in v1.0.0

All items from the original v0.1.0 roadmap are complete. See `DESIGN.md` for full architecture details.

| Feature | Notes |
|---------|-------|
| Knowledge graph (nodes, edges, properties, provenance) | Core storage engine |
| Confidence scoring (reinforcement, correction, decay) | Learning engine |
| Memory tiers (Core, Active, Archival) | Auto-promotion/demotion |
| BM25 full-text search with boolean queries | AND/OR/NOT, property + tier filters |
| Forward-chaining inference engine | Push-based (async after store/relate) |
| CPU SIMD compute (AVX2+FMA, NEON) | x86_64 + aarch64 |
| GPU compute via wgpu (DX12, Vulkan, Metal) | Chunked dispatch, verified RTX 5070 |
| NPU detection + low-power routing | Compute planner |
| HTTP REST API (axum, 20+ endpoints) | Incl. /batch, /compute |
| MCP server (JSON-RPC over stdio) | Tool-calling integration |
| A2A protocol (agent-to-agent skill routing) | Ed25519 identity |
| Natural language interface (/tell, /ask) | ~35 + ~12 patterns, LLM fallback |
| Single-file .brain storage with WAL | Auto-growing, crash recovery |
| Cross-platform (Windows, macOS, Linux) | x86_64 + aarch64 |
| Knowledge mesh (trust-based sync, HTTP transport) | Delta sync, conflict resolution |
| External API embedder (Ollama, OpenAI, vLLM) | Auto-detect dimensions |
| Local ONNX embedder (ort + tokenizers) | Feature-gated, sidecar model |
| Web UI frontend | Dashboard, graph, search, NL, import, learning |
| RwLock concurrency | Concurrent readers, exclusive writers |
| Deferred checkpoint (5s background flush) | WAL-protected writes |
| JSON-LD export/import | RDF interop (Wikidata, DBpedia, schema.org) |
| Int8 vector quantization | 4x memory reduction |
| gRPC endpoint (tonic + prost) | Feature-gated, 13 RPCs, port 50051 |

**Resolved design questions from v0.1.0:**
- Push vs pull rule triggers: push (async), with chain depth limit
- Mesh transport: HTTP/JSON
- Embedding model bundling: BYOM, no bundled model
- RDF mapping: JSON-LD with URI-to-label conversion
- Quantization: int8 scalar first

---

## v1.1.0 -- Intelligence & Ingestion Layer

**Status:** Complete (all 100+ tasks done, 561 tests pass)
**Build plan:** `docs/BUILD-PLAN-v1.1.0.md`
**Design document:** `docs/DESIGN-v1.1.0.md`

Six new subsystems transform engram from passive storage into an active intelligence engine:

| Subsystem | Crate | Purpose |
|-----------|-------|---------|
| Ingest Pipeline | `engram-ingest` | High-speed ELT with NER, entity resolution, conflict detection |
| Action Engine | `engram-action` | Event-driven rules triggering effects from graph changes |
| Reasoning Layer | `engram-reason` | Black area detection, knowledge gap analysis |
| Assessment System | `engram-assess` | Hypothesis tracking with evidence-based probability scoring |
| Encrypted Secrets | `engram-api` | AES-256-GCM encrypted secrets storage (API keys, credentials) |
| Frontend (WASM) | `engram-ui` | Pipeline management, NER config, gap visualization |

**Key decisions:**
- Single binary (feature-gated, not separate processes)
- ~~NER via `anno` crate~~ **(superseded)** -- see v1.2.0 GLiNER ONNX migration below
- Conservative entity resolution (no Rust ER library exists -- competitive advantage)
- Mesh federated query (search across peers, don't copy facts)
- Mesh knowledge profiles (auto-derived, broadcast via gossip)
- Search ledger with 4 dedup layers (temporal cursor, content hash, query subsumption, adaptive frequency)
- Budget tracking via source usage endpoints (engram never calculates costs)
- 3-tier enrichment: mesh (free) > external free > external paid
- LLM-suggested investigations in frontend (never auto-execute, permanent warning)
- Assessment auto-evaluation via pure graph propagation (adaptive BFS, no LLM dependency)
- Encrypted secrets with AES-256-GCM + Argon2id, master password always prompted
- Edge soft-delete with WAL recovery for assessment evidence/watch removal
- Full sidecar persistence for rules, schedules, peers, audit (.brain.* files)
- Assessment integration across all 5 protocols (REST, MCP, A2A, gRPC, LLM tools)
- Mesh auto-enable from config with ed25519 keypair load-or-generate

---

## v1.2.0 -- NER Migration & Integrations

**Status:** In Progress

### GLiNER2 Unified NER+RE (replaces gline-rs + NLI sidecars)

**Design document:** `docs/eval-re-approaches.md`
**Model:** [dx111ge/gliner2-multi-v1-onnx](https://huggingface.co/dx111ge/gliner2-multi-v1-onnx) (Apache-2.0)
**Status:** Complete -- NER+RE working in-process, 10/10 tests pass

Evaluated 3 approaches (GLiNER-Multitask, GLiNER2, NLI). GLiNER2 (`fastino/gliner2-multi-v1`)
wins decisively: 100% recall on German, ~125ms/sentence, zero-shot multilingual via mDeBERTa-v3-base.

**Architecture:**
- In-process via `ort` rc.12 + `tokenizers` -- no sidecar, no subprocess, no gline-rs
- 5 ONNX sessions: encoder, span_rep, count_embed, count_pred, classifier
- FP16 hybrid encoder (530MB): weights FP16 on disk, Cast nodes auto-upcast to FP32 at runtime
- Combined multi-relation schema (single encoder pass, prevents noise from per-type scoring)
- rayon parallel span scoring, dynamic thread count via `available_parallelism()`
- `Extractor` + `RelationExtractor` traits on shared `Arc<Gliner2PipelineBackend>`
- NER threshold 0.5, RE threshold 0.85 (facts only, no noise)

**Why not INT8:** Special token embeddings (`[R]`, `[E]`) have cosine 0.80 vs FP32 (vs 0.97 overall).
This cascades through count_embed (tail cosine drops to 0.65) and flips relation rankings.
INT8 is acceptable for NER-only but produces wrong RE results. Full analysis in design doc.

**Removed:** `gliner_backend.rs`, `rel_nli.rs`, `rel_glirel.rs`, `gliner`/`nli-rel`/`glirel` features,
`engram-ner` sidecar, `engram-rel` sidecar, gline-rs dependency.

| # | Task | Status |
|---|------|--------|
| G2.1 | Evaluate GLiNER-Multitask, GLiNER2, NLI with German test corpus | DONE |
| G2.2 | ONNX export (5 models) + reusable `export_gliner2_onnx.py` | DONE |
| G2.3 | FP16 hybrid encoder (Cast-node trick: FP16 weights, FP32 compute) | DONE |
| G2.4 | INT8 investigation (special token precision loss root-caused) | DONE |
| G2.5 | `gliner2_backend.rs` -- full NER+RE pipeline in Rust | DONE |
| G2.6 | `[R]` token fix (250107, distinct from `[E]` 250106) | DONE |
| G2.7 | Word-level embedding fix (first-token pooling, not all subwords) | DONE |
| G2.8 | Combined multi-relation schema (single encoder pass, noise elimination) | DONE |
| G2.9 | Pipeline wiring (Extractor + RelationExtractor traits, handler, feature flag) | DONE |
| G2.10 | Remove old sidecars (gliner_backend, rel_nli, rel_glirel, 3 features) | DONE |
| G2.11 | HuggingFace upload (FP32 + FP16 + INT8 with warning + export script) | DONE |
| G2.12 | `POST /config/gliner2-download` endpoint | DONE |
| G2.13 | Wizard UI updated for GLiNER2 + RE threshold | DONE |
| G2.14 | 10 integration tests (EN, DE, FR, ES, mixed-lang) | DONE |
| G2.15 | End-to-end server test (analyze + ingest with fresh brain) | DONE |

### Performance optimization (deferred)

| # | Task | Priority | Notes |
|---|------|----------|-------|
| P.1 | Parallelize per-relation scoring loop with rayon | Low | Currently sequential over 6 relation types. Inner span scoring already uses `par_iter`. Constraint: `count_embed` ONNX session needs `&mut self`. Fix: batch all `count_embed` calls first (serial), then parallelize scoring (no ONNX calls needed). Encoder pass (~100ms) dominates runtime, scoring loop is ~5ms -- marginal gain. |
| P.2 | Memory-mapped model loading | Low | `ort` `commit_from_file` handles external data natively. Mmap only benefits single-file models. |
| P.3 | GPU/DirectML acceleration for encoder | Medium | `ort` supports DirectML/CUDA providers. Would reduce encoder from ~100ms to ~10ms. Needs feature flag + provider selection in config. |

### Google Workspace Integration (via `gws` CLI)

Google released `@googleworkspace/cli` (`gws`) -- a unified CLI for all Workspace APIs
(Gmail, Drive, Calendar, Sheets, Docs) with structured JSON output and a built-in MCP server,
dynamically generated from Google's Discovery Service.

**Why this matters for engram:**
- Structured JSON output skips NER/parsing -- straight to entity resolution + graph loading
- Built-in MCP server enables direct MCP-to-MCP bridge (engram <-> gws)
- Discovery Service dynamic API means the source adapter auto-updates when Google adds services
- Auth via `gws auth` (OAuth2) -- engram stays out of credential management

**Integration plan:**

| # | Task | Effort | Notes |
|---|------|--------|-------|
| 12.1 | `GwsSource` ingest adapter (shells out to `gws`, pipes JSON through mesh fast path) | Medium | Maps to `RawItem::Structured`, skips NER |
| 12.2 | Calendar event ingestion (temporal nodes, meeting participants as relationship edges) | Medium | `gws calendar events list --format json` |
| 12.3 | Gmail thread ingestion (provenance chains, who-said-what, corroboration scoring) | Medium | Thread -> facts with `authored_by` edges |
| 12.4 | Drive document source nodes (`authored_by` edges feeding learned trust) | Small | Document metadata as properties |
| 12.5 | Sheets batch ingest (each row = entity + properties) | Small | Maps to structured `RawItem` |
| 12.6 | MCP bridge: engram MCP client connects to `gws` MCP server | Medium | Action engine rules can trigger `gws` queries |
| 12.7 | Action engine rule templates for Workspace triggers | Small | E.g., "new email from X about Y -> ingest" |
| 12.8 | Workspace-aware entity resolution (match contacts to graph entities) | Medium | Google contact IDs as stable identifiers |

### Seed Enrichment Improvements (HIGH PRIORITY)

| # | Task | Effort | Priority | Notes |
|---|------|--------|----------|-------|
| SE.1 | **Interactive entity disambiguation** | Medium | HIGH | When Wikipedia search returns ambiguous results, show candidates to user in the wizard: "Did you mean: Emmanuel Macron (President of France) or Brigitte Macron (First Lady)?" Turns waiting time into engagement. Use SSE to stream candidates as they're discovered. |
| SE.2 | Skip KB enrichment for private/internal domains | Small | HIGH | Add wizard option "Skip Wikidata enrichment" for non-public knowledge graphs (e.g., internal network graphs, private research). Seed should still extract entities via GLiNER2 but not query Wikidata. |
| SE.3 | Seed progress indicator with SSE | Medium | HIGH | Show live progress during seed: "Linking entities... (12/20)", "Discovering connections... (45 found)", "Expanding properties...". Currently silent for ~25s. |
| SE.4 | Connect seed entities to domain events | Medium | MEDIUM | When seed text mentions an event ("Russia Ukraine war"), connect ALL seed persons to the event via Wikidata's "participant in" / "conflict" properties. Currently only Putin gets connected. |
| SE.5 | 2-hop shortest path SPARQL for remaining disconnected pairs | Medium | MEDIUM | Current batch shortest path does 1-hop. For entity pairs still disconnected after 1-hop, try 2-hop batch query. More connections at cost of ~5s more. |
| SE.6 | Web search fallback for entities not in Wikidata | Medium | LOW | For entities with no Wikipedia/Wikidata match, use web search to find factual text, then run GLiNER2 RE on discovered text to extract relations. |
| SE.7 | LLM seed expansion | Medium | LOW | If LLM configured, ask it to generate factual statements from entity list. Run GLiNER2 RE on output. Store with `method: LlmFallback` and lower confidence. |

### Other Planned Integrations

| Integration | Priority | Notes |
|-------------|----------|-------|
| Slack ingest source | Medium | Channel history as provenance chains |
| GitHub/Gitea ingest source | Medium | Issues, PRs, commits as knowledge nodes |
| RSS/Atom feed source | Low | Periodic polling via scheduler, content hash dedup |
| Webhook-triggered ingest templates | Low | Generic JSON webhook -> entity mapping config |

---

## Deferred (Post v1.2.0)

| Feature | Priority | Notes |
|---------|----------|-------|
| Coreference resolution | Medium | Pronoun/mention -> canonical entity name resolution. Was provided by anno's `MentionRankingCoref` (rule-based, ~5800 lines). **Research completed 2026-03-14:** No Rust coref crate exists outside anno. Viable options ranked: (1) **Rule-based Rust** (~200-300 lines, ~60-70% F1, <1ms, partial multilingual via pronoun lists per lang) -- recommended first step; (2) **coref-onnx sidecar** (`talmago/allennlp-coref-onnx-mMiniLMv2`, ~200 MB ONNX, ~70-73% F1, multilingual via XLM-R, needs ~500-800 lines Rust decoding or thin Python wrapper) -- recommended tier 2; (3) **LLM-based** via Ollama (~65-70% F1, 1-5s/paragraph, zero additional deps, hallucination risk) -- optional enrichment only; (4) **fastcoref Python sidecar** (78.5% F1, English only, heavy PyTorch dep) -- only if English accuracy critical. Extracting anno's MentionRankingCoref (5800 lines) not recommended due to maintenance burden. |
| SPARQL query adapter | Low | Translate SPARQL subset to engram traversal |
| Temporal queries | Low | Reconstruct graph state at a point in time (WAL infrastructure exists) |
| Distributed consensus (Raft) | Low | For deployments needing strong consistency (mesh uses eventual) |
| Encryption at rest | Low | Deferred since v0.1.0 |
| Binary vector quantization | Low | 32x reduction, useful for candidate filtering (int8 done) |
| Product quantization (PQ) | Low | Configurable compression, for very large collections |
| Node clustering / Type clouds | Medium | Group same-type nodes into expandable clusters. Reduces visual clutter for large graphs. Explored in `design-explore-v3-performance.md` (technique D). |
| Geometry sharing / Instanced rendering | Low | THREE.InstancedMesh for same-type nodes. Reduces draw calls from N to ~6 (one per type). Explored in technique F. |
| Web worker physics | Low | Move force simulation to Web Worker. Eliminates main-thread jank during layout. Explored in technique H. |
| ~~Temporal edge data pipeline~~ | **Done** | Implemented 2026-03-16. Edge struct has `valid_from`/`valid_to` (72 bytes). Edge property store (`.brain.edge_props`) for arbitrary qualifiers. Full chain: SPARQL -> relate_with_temporal() -> EdgeView -> EdgeResponse -> Frontend. |
| ~~Chat system redesign~~ | **Done** | Implemented 2026-03-16. Intelligence analyst workbench with 48 tools, auto-context retrieval, write confirmation batching, temporal awareness, follow-up suggestions. Page-aware visibility (Explore/Insights only). Design: `docs/design-chat.md`. |
| ~~Insights page redesign~~ | **Done** | Implemented 2026-03-16. 2-zone analyst dashboard: assessments (card grid + detail modal) as primary, auto-loaded intelligence gaps as secondary. Rules moved to System page. 909-line monolith split into 4 files (862 lines total). Design: `docs/design-insights.md`. |
