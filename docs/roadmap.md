# Engram Roadmap

**Last updated:** 2026-03-10

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

**Status:** Design phase
**Design document:** `docs/DESIGN-v1.1.0.md`

Four new subsystems transform engram from passive storage into an active intelligence engine:

| Subsystem | Crate | Purpose |
|-----------|-------|---------|
| Ingest Pipeline | `engram-ingest` | High-speed ELT with NER, entity resolution, conflict detection |
| Action Engine | `engram-action` | Event-driven rules triggering effects from graph changes |
| Reasoning Layer | `engram-reason` | Black area detection, knowledge gap analysis |
| Frontend (WASM) | `engram-ui` | Pipeline management, NER config, gap visualization |

**Key decisions:**
- Single binary (feature-gated, not separate processes)
- NER via `anno` crate (GLiNER2, coreference, candle backend), feature-gated, BYOM
- Conservative entity resolution (no Rust ER library exists -- competitive advantage)
- Mesh federated query (search across peers, don't copy facts)
- Mesh knowledge profiles (auto-derived, broadcast via gossip)
- Search ledger with 4 dedup layers (temporal cursor, content hash, query subsumption, adaptive frequency)
- Budget tracking via source usage endpoints (engram never calculates costs)
- 3-tier enrichment: mesh (free) > external free > external paid
- LLM-suggested investigations in frontend (never auto-execute, permanent warning)

---

## Deferred (Post v1.1.0)

| Feature | Priority | Notes |
|---------|----------|-------|
| SPARQL query adapter | Low | Translate SPARQL subset to engram traversal |
| Temporal queries | Low | Reconstruct graph state at a point in time (WAL infrastructure exists) |
| Distributed consensus (Raft) | Low | For deployments needing strong consistency (mesh uses eventual) |
| Encryption at rest | Low | Deferred since v0.1.0 |
| Binary vector quantization | Low | 32x reduction, useful for candidate filtering (int8 done) |
| Product quantization (PQ) | Low | Configurable compression, for very large collections |
