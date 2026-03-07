# Engram Roadmap

**Version:** 0.1.0
**Last updated:** 2026-03-07

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
| GPU compute via wgpu (DX12, Vulkan, Metal) | Done |
| NPU detection and low-power compute routing | Done |
| Compute planner with automatic workload routing | Done |
| HTTP REST API (axum) with 18 endpoints | Done |
| MCP server (JSON-RPC over stdio) | Done |
| A2A protocol support (agent-to-agent) | Done |
| Natural language interface (~35 patterns for /tell, ~12 for /ask) | Done |
| Optional LLM fallback for NL parsing (ENGRAM_LLM_ENDPOINT) | Done |
| Case-insensitive label matching | Done |
| Auto-growing storage (doubles capacity on demand) | Done |
| Single-file .brain storage with WAL crash recovery | Done |
| Cross-platform: Windows, macOS, Linux (x86_64 + aarch64) | Done |
| Knowledge mesh peer model with trust-based sync | Done |
| SHA-256 identity for mesh peers | Done |

---

## Near-Term Improvements

### 1. Push-Based Rule Triggers

**Status:** Open topic -- low effort
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

**Status:** Planned
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

**Phase 3 -- SPARQL Query Adapter (future):**
- Translate a subset of SPARQL queries to engram's graph traversal
- Support basic graph patterns (triple patterns, FILTER, OPTIONAL)
- Not a full SPARQL 1.1 implementation -- focus on the queries agents actually need

**Why this matters:** The agent ecosystem is moving toward semantic interoperability. Agents will need to share structured knowledge across systems. JSON-LD is the bridge between engram's property graph and the linked data ecosystem (Wikidata, DBpedia, schema.org, other agent knowledge stores).

---

### 3. Vector Quantization

**Status:** Planned
**Priority:** Medium

Currently all vectors are stored as full f32 (4 bytes per dimension). For large vector collections, this is wasteful.

**Options:**
- **int8 quantization**: 4x memory reduction, ~1% accuracy loss for cosine similarity
- **Binary quantization**: 32x reduction, useful for initial candidate filtering (rerank with f32)
- **Product quantization (PQ)**: configurable compression ratio, good for very large collections

**Impact:** A 1M vector collection at 384 dimensions currently needs ~1.5 GB. With int8: ~375 MB. With binary: ~47 MB.

---

### 4. Mesh Federation -- Production Hardening

**Status:** Architecture exists, needs real-world testing
**Priority:** Medium

The `engram-mesh` crate provides the peer model, trust scoring, delta sync, and conflict resolution. What's needed:

- **Transport layer**: actual network sync (currently the sync engine operates on in-memory data structures)
- **Discovery**: how peers find each other (mDNS for LAN, manual config for WAN)
- **Selective sync**: sync only specific node types or tiers between peers
- **Conflict UI**: surface merge conflicts to the user when automatic resolution isn't possible

**Target use case:** A user with multiple machines (laptop, workstation, home server) where each holds knowledge relevant to its role. The laptop has daily work notes, the workstation has project knowledge with GPU compute, the home server has archival memory. Knowledge flows between them via mesh sync with trust-based confidence propagation.

---

### 5. Multilingual Embedder Integration

**Status:** Stub exists (dummy embedder), needs real implementation
**Priority:** Critical -- required for semantic search to work

For semantic similarity search (`POST /similar`), engram needs an embedding model. The model must be **multilingual** to support knowledge in any language.

**Recommended models:**

| Model | Dimensions | Languages | Size | Notes |
|-------|-----------|-----------|------|-------|
| multilingual-e5-small | 384 | 100+ | ~120 MB | Best balance of size, speed, and multilingual quality |
| multilingual-e5-base | 768 | 100+ | ~280 MB | Better accuracy, heavier |
| paraphrase-multilingual-MiniLM-L12-v2 | 384 | 50+ | ~120 MB | Sentence-Transformers classic |
| all-MiniLM-L6-v2 | 384 | English only | ~23 MB | Smallest, English-only fallback |

**Implementation approach:**
- **ONNX Runtime** (primary): load ONNX-exported models directly in Rust via `ort` crate. No Python dependency, runs on CPU/GPU. Adds the model weight file as a sidecar (not baked into the binary).
- **External API** (fallback): call an OpenAI-compatible embedding endpoint (`ENGRAM_EMBED_ENDPOINT`). Works with OpenAI, Ollama, vLLM, or any compatible server. Zero local model overhead.
- **Auto-detect**: if a local ONNX model file exists next to the .brain file, use it. Otherwise check for `ENGRAM_EMBED_ENDPOINT`. Otherwise disable vector search (BM25 still works).

**Why multilingual is essential:**
- Use Case 3 (multi-language knowledge) requires cross-language similarity
- "Hund" and "Dog" should be semantically similar even though they share no characters
- A German question should find English knowledge and vice versa
- Enterprise users operate in multiple languages

The HNSW index infrastructure already exists. What's missing is the vectorization step.

---

### 6. Frontend / Web UI

**Status:** Discussed, not started
**Priority:** Critical -- essential for usability

A web-based UI for visualizing and interacting with the knowledge graph. This is not optional polish -- without it, engram is accessible only via CLI and curl, which limits adoption to developers.

**Core features:**
- Graph visualization (nodes + edges, force-directed layout with D3.js or vis.js)
- Search interface with filter builder (confidence, properties, tiers, boolean)
- Node detail view with properties, edges, provenance history
- Learning controls (reinforce, correct, decay) with visual confidence indicators
- Import wizards (Wikipedia, documents, NER pipeline, web search)
- Natural language interface (ask/tell with response display)
- Mesh status dashboard (peer connections, sync history, trust scores)

**Technology decision:**
- **Separate frontend project** (recommended): React or Svelte app, talks to the HTTP API
- Served by engram itself (embed static files in binary) or as a standalone SPA
- CORS is already permissive in the API, so the frontend can run on any port

**Design principles:**
- Works without JavaScript frameworks for basic views (progressive enhancement)
- Mobile-responsive (knowledge lookup on phone)
- Dark mode (developers spend enough time staring at screens)

---

### 7. Real gRPC Endpoint

**Status:** Not started
**Priority:** Low

The current "gRPC-style" routes are standard HTTP/JSON endpoints. Real gRPC would provide:
- Protobuf binary serialization (faster than JSON for large payloads)
- HTTP/2 framing (multiplexed streams)
- Strongly typed service contracts
- Better tooling for service-to-service communication

**Approach:**
- Define `.proto` files for all 18 API operations
- Use `tonic` + `prost` for Rust code generation
- Run gRPC on a separate port (e.g., 50051) alongside the HTTP API
- Both APIs backed by the same handlers

**Priority is low** because the HTTP/JSON API works for all current use cases (Python clients, curl, LLM tool calling, MCP). gRPC mainly benefits high-throughput service-to-service communication.

---

## Medium-Term Improvements

### 8. Graph Algorithms (PageRank, Community Detection)

**Status:** Not started
**Priority:** Medium

Built-in graph algorithms would enable analytics without external tools:

- **PageRank**: identify the most connected/important entities
- **Community detection** (Louvain): find clusters of related knowledge
- **Shortest path**: find the most direct relationship chain between two entities
- **Betweenness centrality**: identify bridge entities that connect different knowledge domains
- **Connected components**: find isolated knowledge islands

These could be GPU-accelerated using the existing wgpu compute infrastructure for large graphs.

---

### 9. Plugin / Extension System

**Status:** Not started
**Priority:** Low

Allow custom importers, exporters, and rule evaluators as plugins:

- **Importers**: RSS feed watcher, email inbox scanner, Slack channel monitor, Git commit history
- **Exporters**: JSON-LD, CSV, Markdown reports, GraphML
- **Rule evaluators**: custom rule syntax, external rule engines
- **Webhooks**: notify external systems when knowledge changes

Could use Rust's dynamic loading (`libloading`) or a simpler approach: shell commands that read/write JSON to engram's API.

---

## Long-Term Vision

### 10. SPARQL Query Adapter

Translate SPARQL queries to engram's graph operations. Not a full SPARQL 1.1 implementation -- focus on the subset that agents and knowledge tools actually use:
- Basic graph patterns (triple patterns)
- FILTER with comparison operators
- OPTIONAL for left-join semantics
- LIMIT and OFFSET
- Property paths (simple, not recursive)

### 11. Temporal Queries

Query knowledge as it was at a specific point in time:
- "What did we know about server-01 on January 15th?"
- "Show me the confidence history of this claim"
- "What changed in the last 24 hours?"

The WAL already records all mutations with timestamps. The temporal index exists. What's needed is a query API that takes a timestamp parameter and reconstructs the graph state at that point.

### 12. Multi-Tenant / Namespace Support

Multiple isolated knowledge graphs in a single engram instance:
- Each tenant/namespace has its own .brain file
- API routes include namespace prefix (`/ns/{name}/store`, etc.)
- Cross-namespace queries with explicit opt-in
- Per-namespace access control

### 13. Distributed Consensus for Mesh

For mesh deployments where consistency matters more than availability:
- Raft consensus for critical knowledge (tier: core)
- Eventual consistency for non-critical knowledge (tier: active, archival)
- Configurable per-node-type consistency level

---

## Open Design Questions

These need further discussion before implementation:

1. **Push vs pull rule triggers**: should rules auto-fire (convenient but potentially surprising) or remain on-demand (explicit but requires caller discipline)?

2. **Mesh transport protocol**: HTTP/JSON for simplicity, or a custom binary protocol for efficiency? Or piggyback on the A2A protocol?

3. **Embedding model bundling**: include a small model (MiniLM, ~20 MB) in the binary for zero-config semantic search, or keep the binary small and require external embedding?

4. **Frontend architecture**: embedded in the binary (simple deployment) or separate project (better developer experience, larger ecosystem of UI frameworks)?

5. **RDF mapping**: how to handle RDF features that don't map cleanly to property graphs (blank nodes, named graphs, reification)?

6. **Quantization strategy**: which quantization method to implement first? int8 is simplest and most universally useful. Binary is fastest for candidate filtering. PQ is most flexible but most complex.
