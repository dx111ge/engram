# Engram Roadmap

**Version:** 0.1.0
**Last updated:** 2026-03-08

This document tracks planned improvements, open design questions, and future directions for engram. Items are grouped by priority and effort estimate.

---

## Completed in v0.1.0

| Feature | Status |
|---------|--------|
| Knowledge graph with nodes, edges, properties, provenance | Done |
| Confidence scoring with reinforcement, correction, decay | Done |
| Memory tiers (Core, Active, Archival) with auto-promotion/demotion | Done |
| BM25 full-text search with boolean queries (AND/OR/NOT) | Done |
| Property filters and tier filters in search | Done |
| Forward-chaining inference engine | Done |
| CPU SIMD compute (AVX2+FMA on x86_64, NEON on aarch64) | Done |
| GPU compute via wgpu (DX12, Vulkan, Metal) with chunked dispatch | Done |
| NPU detection and low-power compute routing | Done |
| Compute planner with automatic workload routing | Done |
| HTTP REST API (axum) with 20 endpoints (incl. /batch, /compute) | Done |
| MCP server (JSON-RPC over stdio) | Done |
| A2A protocol support (agent-to-agent skill routing) | Done |
| Natural language interface (~35 patterns for /tell, ~12 for /ask) | Done |
| Optional LLM fallback for NL parsing (ENGRAM_LLM_ENDPOINT) | Done |
| Case-insensitive label matching | Done |
| Auto-growing storage (doubles capacity on demand) | Done |
| Single-file .brain storage with WAL crash recovery | Done |
| Cross-platform: Windows, macOS, Linux (x86_64 + aarch64) | Done |
| Knowledge mesh peer model with trust-based sync | Done |
| Ed25519 identity for mesh peers | Done |
| External API embedder (Ollama, OpenAI, vLLM compatible) | Done |
| Auto-detect embedding dimensions (Matryoshka model support) | Done |
| Web UI frontend (dashboard, graph, search, NL, import, learning) | Done |
| RwLock concurrency (concurrent readers, exclusive writers) | Done |
| Deferred checkpoint (background 5s flush, WAL-protected writes) | Done |
| Batch API for bulk ingestion (POST /batch) | Done |
| Push-based rule triggers (async after store/relate/tell) | Done |
| JSON-LD export/import (GET /export/jsonld, POST /import/jsonld) | Done |
| Int8 vector quantization (4x memory reduction, POST /quantize) | Done |
| Mesh HTTP transport (8 endpoints, peer management, sync, audit) | Done |
| Local ONNX embedder (ort + tokenizers, sidecar model auto-detect) | Done |
| Real gRPC endpoint (tonic + prost, protobuf binary, port 50051) | Done |

---

## Open Issues (v0.1.0 scope)

### 1. Push-Based Rule Triggers

**Status:** Done
**Priority:** High

Currently inference rules are pull-based: you call `POST /learn/derive` to evaluate rules. The rule engine already exists and works. The improvement is to auto-trigger matching rules after every `store` or `relate` operation.

**Approach:**
- Add an optional rule set to `AppState` (loaded at server startup from a config file or API call)
- After each `store`/`relate` handler completes, run the rule engine against the new/modified entities
- Make it opt-in (disabled by default to avoid surprise side effects)
- Consider async execution so rule evaluation doesn't block the HTTP response

**Effort:** Small -- the engine exists, just needs wiring into the mutation path.

**Design questions:**
- Should rules fire synchronously (caller waits) or asynchronously (fire-and-forget)?
- How to handle rule chains (rule A fires, creates edge, which triggers rule B)?
- Maximum chain depth to prevent infinite loops?

---

### 2. JSON-LD Export / Import

**Status:** Done
**Priority:** High

Engram's property graph maps naturally to RDF triples:
- Node = subject (with a URI)
- Edge = predicate
- Target node = object
- Properties = datatype properties

**Phase 1 -- JSON-LD Export:**
- Serialize engram nodes and edges as JSON-LD
- Each node gets a URI (e.g., `engram://{brain-id}/{node-label}`)
- Edge types become predicates
- Properties become datatype assertions
- Output consumable by any RDF-aware system

**Phase 2 -- JSON-LD Import:**
- Parse JSON-LD / Turtle / N-Triples into engram nodes and edges
- Map RDF subjects to engram nodes, predicates to edge types, objects to target nodes or properties
- Import from Wikidata, DBpedia, schema.org directly into a .brain file
- Handle URI-to-label conversion (strip namespace prefixes)

**Why this matters:** The agent ecosystem is moving toward semantic interoperability. Agents will need to share structured knowledge across systems. JSON-LD is the bridge between engram's property graph and the linked data ecosystem (Wikidata, DBpedia, schema.org, other agent knowledge stores).

---

### 3. Vector Quantization

**Status:** Done (int8 scalar quantization)
**Priority:** Medium

Currently all vectors are stored as full f32 (4 bytes per dimension). For large vector collections, this is wasteful.

**Options:**
- **int8 quantization**: 4x memory reduction, ~1% accuracy loss for cosine similarity
- **Binary quantization**: 32x reduction, useful for initial candidate filtering (rerank with f32)
- **Product quantization (PQ)**: configurable compression ratio, good for very large collections

**Impact:** A 1M vector collection at 384 dimensions currently needs ~1.5 GB. With int8: ~375 MB. With binary: ~47 MB.

---

### 4. Mesh Federation -- Transport Layer

**Status:** Done (HTTP transport endpoints wired)
**Priority:** Medium

The `engram-mesh` crate provides the peer model, trust scoring, delta sync, and conflict resolution. What's needed:

- **Transport layer**: actual network sync (currently the sync engine operates on in-memory data structures)
- **Discovery**: how peers find each other (mDNS for LAN, manual config for WAN)
- **Selective sync**: sync only specific node types or tiers between peers
- **Conflict UI**: surface merge conflicts to the user when automatic resolution isn't possible
- Wire `/batch` endpoint as the receiver for incoming delta sync payloads

**Target use case:** A user with multiple machines (laptop, workstation, home server) where each holds knowledge relevant to its role. The laptop has daily work notes, the workstation has project knowledge with GPU compute, the home server has archival memory. Knowledge flows between them via mesh sync with trust-based confidence propagation.

---

### 5. Local ONNX Embedder

**Status:** Done
**Priority:** Medium

Feature-gated (`--features onnx`) local embedding using `ort` v2.0 + `tokenizers` + `ndarray`:

- `OnnxEmbedder` implements `Embedder` trait with `Mutex<Session>` for thread safety
- Auto-detects sidecar files: `{brain}.model.onnx` + `{brain}.tokenizer.json`
- Mean-pooling + L2 normalization for transformer outputs
- Tested with multilingual-e5-small (384D, ~120 MB)
- Falls back to API embedder, then BM25

---

### 6. Real gRPC Endpoint

**Status:** Done
**Priority:** Low

Feature-gated (`--features grpc`) real protobuf binary gRPC via tonic + prost:

- Code generated from `proto/engram.proto` (13 RPCs)
- Runs on separate port (default 50051) alongside HTTP API
- All 13 service methods: Store, Relate, Query, Search, GetNode, DeleteNode, Reinforce, Correct, Decay, Health, Stats, Ask, Tell
- Without `--features grpc`: JSON-over-HTTP/2 fallback on same paths
- Tested end-to-end with Python grpcio client

---

## Deferred (Post v0.1.0)

| Feature | Priority | Notes |
|---------|----------|-------|
| SPARQL query adapter | Low | Translate SPARQL subset to engram traversal |
| Temporal queries | Low | Reconstruct graph state at a point in time (WAL infrastructure exists) |
| Multi-tenant namespaces | Low | Per-namespace .brain files with isolated access |
| Distributed consensus (Raft) | Low | For mesh deployments needing strong consistency |
| Graph algorithms (PageRank, Louvain) | Medium | Could be GPU-accelerated |
| Plugin / extension system | Low | Custom importers, exporters, webhooks |
| Encryption at rest | Low | Designed as deferred in v0.1.0 |

---

## Open Design Questions

These need further discussion before implementation:

1. **Push vs pull rule triggers**: should rules auto-fire (convenient but potentially surprising) or remain on-demand (explicit but requires caller discipline)?

2. **Mesh transport protocol**: HTTP/JSON for simplicity, or a custom binary protocol for efficiency? Or piggyback on the A2A protocol?

3. **Embedding model bundling**: include a small model (MiniLM, ~20 MB) in the binary for zero-config semantic search, or keep the binary small and require external embedding?

4. **RDF mapping**: how to handle RDF features that don't map cleanly to property graphs (blank nodes, named graphs, reification)?

5. **Quantization strategy**: which quantization method to implement first? int8 is simplest and most universally useful. Binary is fastest for candidate filtering. PQ is most flexible but most complex.
