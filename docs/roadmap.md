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

### GLiNER ONNX Migration (anno+candle -> gline-rs+ort)

**Design document:** `docs/design-gliner-onnx-migration.md`

The `anno` crate with candle backend proved unreliable for GLiNER NER:
- 3 bugs hit in one session (config.json fallback, encoder resolution, SentencePiece unsupported)
- No multilingual GLiNER model works out of the box with anno's candle backend
- The encoder for all multilingual GLiNER models (`microsoft/mdeberta-v3-base`) ships SentencePiece only, which candle doesn't support

**Solution:** Replace anno+candle with `gline-rs` (Rust GLiNER ONNX inference) as a sidecar binary.
Default model: `knowledgator/gliner-x-small` (quantized ONNX = 173 MB, 20 languages, ~0.75 F1).
Alternative: `onnx-community/gliner_multi-v2.1` (INT8 = 349 MB, 6 languages, ~0.66 F1).
Sidecar pattern avoids ort version conflict (gline-rs pins ort rc.9, engram uses rc.12).

| # | Task | Effort | Status |
|---|------|--------|--------|
| NER.1 | Create `engram-ner` sidecar binary (gline-rs + JSON stdin/stdout) | Medium | DONE |
| NER.2 | Create `gliner_backend.rs` in engram-ingest (subprocess, Extractor trait) | Medium | DONE |
| NER.3 | NER model download endpoint for ONNX models from onnx-community | Small | DONE |
| NER.4 | Update wizard for ONNX model download + progress indication | Small | DONE |
| NER.5 | Remove `anno` dependency, delete `anno_backend.rs` | Small | DONE |
| NER.6 | End-to-end test: wizard -> download -> analyze -> ingest | Medium | DONE |
| NLI.1 | Wizard: NLI threshold slider + download progress | Small | DONE |
| NLI.2 | System: threshold slider + template import/export | Small | DONE |
| NLI.3 | Backend: rel_threshold merge fix, GET /config fields, template endpoints | Small | DONE |

**NLI relation extraction** (`engram-rel` sidecar) is migrated and working. Wizard + System UI complete.

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
