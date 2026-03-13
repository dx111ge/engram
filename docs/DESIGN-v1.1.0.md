# Engram v1.1.0 Design Document

## Intelligence & Ingestion Layer

**Version:** 1.1.0
**Date:** 2026-03-10
**Status:** Draft
**Prerequisite:** v1.0.0 (all phases complete, 355 tests passing)

---

## Executive Summary

Version 1.1.0 transforms engram from a passive knowledge store into an **active intelligence engine**. Three new subsystems are introduced:

1. **Ingest Pipeline** (`engram-ingest`) -- high-speed, multi-threaded ETL with built-in NER, entity resolution, and conflict detection
2. **Action Engine** (`engram-action`) -- event-driven rule system that triggers effects from graph changes
3. **Reasoning Layer** (`engram-reason`) -- black area detection, knowledge gap analysis, and query-triggered enrichment

Together these create a feedback loop: engram detects what it doesn't know, hunts for knowledge, ingests it through an intelligent pipeline, and triggers actions based on what changes. Engram never invents knowledge -- every fact has provenance, every extraction has a confidence score, every conflict is auditable.

**Core identity preserved:** Engram remains a database. It does not crawl, does not proactively fetch, and does not guess. External data enters only through explicit ingestion (push) or query-triggered enrichment (pull). The ingest pipeline is the gatekeeper -- nothing enters the graph without passing through extraction, resolution, and conflict checking.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Bulk Upload Endpoint](#2-bulk-upload-endpoint)
3. [Ingest Pipeline (`engram-ingest`)](#3-ingest-pipeline)
4. [NER Engine](#4-ner-engine)
5. [Confidence Model](#5-confidence-model)
6. [Conflict Resolution](#6-conflict-resolution)
7. [Action Engine (`engram-action`)](#7-action-engine)
8. [Black Area Detection (`engram-reason`)](#8-black-area-detection)
9. [Query-Triggered Enrichment](#9-query-triggered-enrichment)
10. [Streaming Architecture](#10-streaming-architecture)
11. [Frontend (WASM-Compiled)](#11-frontend)
12. [Crate Structure](#12-crate-structure)
13. [Build Order](#13-build-order)
14. [Testing Strategy](#14-testing-strategy)
15. [Use Cases](#15-use-cases)
16. [Security Considerations](#16-security-considerations)

---

## 1. Architecture Overview

```
v1.0.0 (current):

  [LLM/User] --> [API] --> [Graph] --> [Response]

v1.1.0 (target):

  [External Sources]
        |
        v
  +-----------------+
  | INGEST PIPELINE |  <-- push mode (bulk, scheduled, streaming)
  | Parse > NER >   |
  | Resolve > Dedup |
  | > Conflict >    |
  | Confidence >    |
  | Store           |
  +---------+-------+
            |
            v
  +---------+-------+     +----------------+     +-----------------+
  |   ENGRAM CORE   |<--->|  ACTION ENGINE |<--->| REASON ENGINE   |
  |   Graph +       |     |  Event bus     |     | Black area      |
  |   Storage +     |     |  Rules         |     | detection       |
  |   Index         |     |  Effects       |     | Gap analysis    |
  +---------+-------+     +-------+--------+     +--------+--------+
            |                     |                       |
            v                     v                       v
  +---------+-------+     +-------+--------+     +--------+--------+
  |   API LAYER     |     | External       |     | Enrichment      |
  |   HTTP/MCP/A2A  |     | Webhooks, msgs |     | Spearhead search|
  |   gRPC          |     | API calls      |     | Source fan-out  |
  +--------+--------+     +----------------+     +--------+--------+
           |                                              |
           v                                              |
     [LLM/User/Agent]                                     |
           |                                              |
           +--- query gap detected? ---> triggers --------+
```

**Key design principle:** The graph (`engram-core`) emits lightweight events on every mutation. It does not know about ingest, actions, or reasoning. The new subsystems subscribe to these events and react. Dependency flows one way: new crates depend on core, core depends on nothing new.

### 1.1 Single Binary, Feature-Gated

Engram is **one binary**. Ingest, action, and reason are separate crates compiled into the same process via feature flags. This is a performance-critical decision.

**Why not separate processes?**

Entity resolution must read the graph to match extracted entities against existing nodes. In a two-process model, every resolution check is an HTTP round trip (~1-5ms with serialization). In a single process, it's a direct function call under a read lock (~0.001ms). At scale, the difference is 1000x:

```
Two processes:  3 HTTP round trips/fact × 10ms avg = 30ms/fact = ~33 facts/sec/worker
One process:    direct calls = ~0.05ms/fact (excluding NER) = ~20,000 facts/sec/worker
```

NER is the actual bottleneck (~5-50ms/text), not graph access. Keeping graph access in-process means NER throughput is the only limit.

**Binary variants:**

```
cargo build                                   # core only (~5MB)
cargo build --features ingest                 # core + pipeline (~8MB)
cargo build --features ingest,anno            # core + pipeline + NER (~12MB)
cargo build --features full                   # everything
```

**Runtime toggle:**

```toml
# engram.toml
[ingest]
enabled = true       # on/off without recompile
workers = 4          # leave cores free for API serving
max_queue_depth = 10000
```

Engram without ingest is exactly v1.0.0. Ingest is purely additive.

### 1.2 Two Doors Into Engram

```
┌─────────────────────────┐       ┌─────────────────────────┐
│     DIRECT STORE        │       │    INGEST PIPELINE      │
│                         │       │                         │
│  POST /store            │       │  POST /ingest           │
│  POST /batch            │       │  POST /ingest/file      │
│  LLM tool-calling       │       │  Scheduled sources      │
│  MCP protocol           │       │  File watch (dropzone)  │
│  Mesh sync (peer did    │       │  Streaming (webhooks,   │
│    NER + resolve)       │       │    SSE, WebSocket)      │
│                         │       │  Event-triggered jobs   │
│  Structured data in,    │       │  (dynamic, from graph   │
│  straight to graph.     │       │   intelligence)         │
│  No NER, no resolve,    │       │                         │
│  no pipeline.           │       │  Raw/unstructured in,   │
│                         │       │  full pipeline:         │
│                         │       │  parse→NER→resolve→     │
│                         │       │  dedup→conflict→store   │
└────────────┬────────────┘       └────────────┬────────────┘
             │                                  │
             └──────────┬───────────────────────┘
                        v
              ┌─────────────────┐
              │   ENGRAM CORE   │
              │   Graph + Store │
              │   + Index       │
              └─────────────────┘
```

Users never lose the simple path. `POST /batch` works exactly as v1.0.0.

### 1.3 Pipeline Shortcuts

When data is partially structured, full pipeline processing is waste. Callers can skip stages:

```
POST /ingest                          Full pipeline (unstructured text)
POST /ingest?skip=ner,resolve         Structured data, skip NER + resolve
POST /ingest?skip=ner                 Structured data, still resolve against graph
POST /batch                           Full bypass: direct store, no pipeline
```

| Scenario | Skip | Why |
|----------|------|-----|
| Plain text file | Nothing | Needs full pipeline |
| CSV with entity columns | NER | Already structured |
| JSON from trusted internal API | NER + conflict | Structured + trusted |
| Mesh sync from peer | NER + resolve + conflict | Peer already did this (see 1.3.1) |
| LLM tool-calling | NER | LLM extracted entities, still resolve + conflict |
| GitHub release webhook | NER (partial) | Semi-structured, version/repo known |

#### 1.3.1 Mesh Discovery Shortcut

When mesh sync delivers facts from a trusted peer, the peer has already run full NER, entity resolution, and conflict checking on their side. Re-running the full ingest pipeline is redundant and wastes CPU.

**Scenario: Analyst A (iran-desk) syncs to Analyst B (russia-desk)**

```
iran-desk has fact:
  Entity: "Iran Titanium Corp"
  Type: organization
  Confidence: 0.85
  Source: reuters.com (trust 0.7)
  Extraction: GLiNER (0.92)
  Resolved: yes (matched against iran-desk graph)
  Conflicts: none detected

Mesh sync delivers this to russia-desk.
```

**Without shortcut:** russia-desk runs full pipeline -- NER (pointless, entity already extracted), resolve against its own graph (useful!), conflict check (useful), store. Two stages wasted.

**With shortcut:** Mesh-synced facts enter a **mesh fast path**:

```
Mesh sync received
  |
  v
Skip: parse, language detect, NER (peer did this)
  |
  v
DO: resolve against LOCAL graph
  (peer resolved against THEIR graph, but this entity
   might match something different in our graph)
  |
  v
DO: dedup against LOCAL graph
  (might already have this fact from another source)
  |
  v
Skip: conflict check IF peer trust > threshold
  (trusted peer already checked -- but configurable)
  |
  v
Adjust confidence: apply peer trust multiplier
  peer_confidence * peer_trust_level = local_confidence
  |
  v
Store with provenance: source="mesh:{peer_id}"
```

The key insight: **resolution must still run locally** because the same entity might match different nodes in different graphs. "Iran Titanium Corp" might be an exact match in iran-desk but a fuzzy match to "Iranian Titanium Corporation" in russia-desk. But NER is definitively skippable -- the text has already been analyzed.

**Configuration:**

```toml
[mesh.ingest]
skip_ner = true                     # always skip NER for mesh-synced facts
skip_conflict_above_trust = 0.80    # skip conflict check if peer trust > 0.80
always_resolve = true               # always resolve against local graph
trust_multiplier = true             # apply peer trust to confidence
```

This makes mesh sync nearly as fast as direct store while preserving local graph integrity.

### 1.4 Event-Triggered Ingest (Intelligence-Driven)

Beyond user-configured scheduled ingest, engram can create ingest jobs dynamically based on graph intelligence. These are jobs that didn't exist before -- the system decided it needs them.

**Example: Geopolitical cascade**
```
State: Analyst monitors Russia. 80K facts in graph.
Event: Scheduled ingest picks up "Iran declares war on [X]"
Graph knows: Russia --[arms_dealer]--> Iran (0.9 confidence)
             Russia --[oil_trade]--> Iran (0.8)
             Russia --[UN_veto_for]--> Iran (0.7)

Reasoning: High-impact event on entity with 3 strong edges
           to monitored entity (2 hops). Impact assessment needed.

Action engine creates dynamic ingest job:
  queries: derived from graph edges
    ["Iran Russia military impact",
     "Iran war Russia arms exports",
     "Iran sanctions Russia oil"]
  sources: [news APIs, gov databases]
  parent_event: "iran-war-detected"

Facts age normally via existing confidence decay.
War stops → no new corroboration → facts decay naturally.
```

**Example: Repo monitoring**
```
State: Monitoring facebook/react via GitHub Releases API.
       Graph has React v19.0.0 with APIs, breaking changes, deps.

Event: New release v19.1.0 detected.
Store: React:v19.1.0 --[supersedes]--> React:v19.0.0

Reasoning: New version of tracked dependency. Gap on v19.1.0.

Action engine creates dynamic ingest job:
  queries: ["React 19.1 changelog", "React 19.1 breaking changes",
            "React 19.1 migration guide", "React 19.1 security fixes"]
  sources: [github API (CHANGELOG.md), web search]
  reconcile: DeltaAgainstEntity("React:v19.0.0")
    → deprecated API? Mark v19.0.0 fact as superseded
    → security fix? Flag old vulnerability as resolved
    → new API? Store as new knowledge, link to v19.1.0
```

**Example: File dropzone**
```
Config: Watch /data/reports/ for new files

Source: FileSource with notify crate (filesystem events)
  capabilities:
    temporal_cursor: true (file mtime)
    cost_model: Free

New file dropped → enters pipeline → NER → resolve → store
Changed file detected → re-ingest, reconcile against existing facts
```

### 1.5 Scaling Strategy

**Single process capacity:**

| Resource | Capacity | Notes |
|----------|----------|-------|
| Graph size | Hundreds of millions of facts | mmap'd, limited by physical RAM |
| Structured ingest | 50,000-100,000 facts/sec | No NER, 8 workers |
| NER on CPU | ~800 texts/sec (8 cores) | NER is the bottleneck, not storage |
| NER on GPU | ~16,000 texts/sec (batched) | candle/ort CUDA support |
| Write throughput | 200,000-1,000,000 facts/sec | Batched writes, brief lock hold |
| Concurrent reads | Unlimited | RwLock, readers never block each other |

**Scale-out: mesh federation, not multi-process**

For throughput beyond a single machine, the v1.0.0 mesh protocol is the answer:

```
[engram: iran-desk]  ←── mesh sync ──→  [engram: russia-desk]
  Own brain file                           Own brain file
  Own ingest pipeline                      Own ingest pipeline
  Own NER workers                          Own NER workers
  Iran sources                             Russia sources
        │                                         │
        └──── shared facts sync automatically ────┘
```

Each analyst runs their own engram instance with their own sources. Mesh syncs relevant facts between them. One analyst's ingest enriches everyone's graph. No shared database, no coordination overhead, no multi-process complexity.

This means engram scales horizontally by domain, not by splitting one domain across processes. This aligns with how intelligence work actually operates -- analysts own domains.

---

## 2. Bulk Upload Endpoint

### Current State (`POST /batch`)

The existing batch endpoint (v1.0.0) accepts a JSON array of entities and relations, processes them sequentially under a single write lock, and returns counts. This works for small batches (<1000 items) but does not scale.

### Improvements for v1.1.0

#### 2.1 NDJSON Streaming

Accept newline-delimited JSON for large payloads. Each line is parsed and processed independently, allowing the server to start processing before the full payload arrives.

```
POST /batch/stream
Content-Type: application/x-ndjson

{"entity":"Apple Inc.","entity_type":"ORG","confidence":0.9,"source":"manual"}
{"entity":"Tim Cook","entity_type":"PERSON","confidence":0.9,"source":"manual"}
{"from":"Tim Cook","to":"Apple Inc.","relationship":"ceo_of","confidence":0.95}
```

Benefits:
- No need to hold full payload in memory
- Processing starts on first line
- Natural backpressure via TCP flow control
- Clients can send millions of facts without buffering

#### 2.2 Chunked Write Locking

Instead of one write lock for the entire batch, acquire the lock per chunk (configurable, default 1000 items). This keeps reads alive during large imports.

```
Chunk size: 1000 items
Batch of 50,000 items:
  - 50 lock acquisitions instead of 1
  - Readers can interleave between chunks
  - Each chunk is atomic (all-or-nothing within the chunk)
```

#### 2.3 Upsert Semantics

New mode: "store if new, update confidence if exists." Caller does not need to check existence first.

```json
{
  "entities": [...],
  "relations": [...],
  "mode": "upsert",
  "confidence_strategy": "max"  // "max", "replace", "average"
}
```

Strategies:
- `max`: keep whichever confidence is higher (existing or incoming)
- `replace`: incoming always wins
- `average`: new confidence = (existing + incoming) / 2

#### 2.4 Progress Reporting

For large batches, return a job ID and support polling:

```
POST /batch/stream → 202 Accepted, {"job_id": "abc123"}
GET  /batch/jobs/abc123 → {"status": "running", "processed": 12000, "total": 50000, "errors": 3}
```

Or: Server-Sent Events for real-time progress.

#### 2.5 Endpoint Summary

```
POST /batch              existing (enhanced with upsert mode)
POST /batch/stream       new: NDJSON streaming ingestion
GET  /batch/jobs/{id}    new: job progress polling
POST /ingest             new: raw text/document ingestion (routes through pipeline)
POST /ingest/configure   new: pipeline configuration
GET  /ingest/status      new: pipeline health and stats
```

---

## 3. Ingest Pipeline (`engram-ingest`)

The ingest pipeline is a multi-stage, multi-threaded ETL engine that transforms raw data into graph-ready facts. It sits between external sources and engram's storage layer.

### 3.1 Pipeline Stages

```
[Raw Input]
     |
     v
  EXTRACT        Pull from source (file, API, stream, web)
     |
     v
  PARSE          Convert format (PDF, HTML, CSV, JSON, plaintext)
     |
     v
  DETECT         Language detection (per-segment for mixed-language docs)
     |
     v
  ANALYZE        NER, relation extraction, topic detection
     |
     v
  RESOLVE        Match against existing graph (entity resolution)
     |
     v
  DEDUPLICATE    Content hash + semantic similarity
     |
     v
  CONFLICT       Check against existing facts, flag contradictions
     |
     v
  CONFIDENCE     Calculate initial confidence (learned trust x extraction confidence)
     |
     v
  TRANSFORM      Normalize, tag provenance, apply custom rules
     |
     v
  LOAD           Bulk write into graph (direct Arc<RwLock<Graph>> access)
     |
     v
  LEARN          Post-store: corroboration boost + co-occurrence update (see 7.1.1)
```

The LEARN step is implicit (handled by the execution model in section 7.1.1, not a separate pipeline stage). When LOAD stores a fact:
- If the fact corroborates an existing fact from a different source: reinforcement (+0.10)
- Co-occurrence counters are updated for all entities extracted from the same document
- These feed back into NER learned patterns and evidence surfacing on queries

**Memory tier assignment during LOAD:**
- Ingested facts default to `tier=active`
- Facts corroborating existing `tier=core` facts: promoted to `tier=active` (not core -- promotion to core requires sustained access over time)
- Facts from paid sources: tagged `cost_justified=true` to influence eviction priority (prefer evicting free-source facts first)

Every stage is a trait. Users can replace, skip, or extend any stage.

### 3.2 Stage Traits

```rust
/// A raw item from a source before any processing.
pub struct RawItem {
    pub content: Content,           // text, bytes, structured data
    pub source_url: Option<String>,
    pub source_type: SourceType,
    pub fetched_at: i64,
    pub metadata: HashMap<String, String>,
}

/// A processed fact ready for graph insertion.
pub struct ProcessedFact {
    pub entity: String,
    pub entity_type: Option<String>,
    pub properties: HashMap<String, String>,
    pub confidence: f32,
    pub provenance: Provenance,
    pub extraction_method: ExtractionMethod,
    pub language: String,
    pub relations: Vec<ExtractedRelation>,
    pub conflicts: Vec<ConflictRecord>,
}

/// Source: produces RawItems from an external source.
pub trait Source: Send + Sync {
    async fn fetch(&self, params: &SourceParams) -> Result<Vec<RawItem>>;
    fn name(&self) -> &str;
    fn source_type(&self) -> SourceType;
}

/// Extractor: analyzes raw text and produces entities/relations.
pub trait Extractor: Send + Sync {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity>;
    fn name(&self) -> &str;
    fn method(&self) -> ExtractionMethod;
    fn supported_languages(&self) -> Vec<String>; // empty = all
}

/// Resolver: matches extracted entities against existing graph.
pub trait Resolver: Send + Sync {
    fn resolve(
        &self,
        entity: &ExtractedEntity,
        graph: &Graph,  // read access
    ) -> ResolutionResult; // Matched(node_id), New, Ambiguous(candidates)
}

/// Transformer: applies custom transformations to processed facts.
pub trait Transformer: Send + Sync {
    fn transform(&self, fact: &mut ProcessedFact) -> TransformResult;
}
```

### 3.3 Multi-Threaded Execution

The pipeline parallelizes at every level for maximum throughput:

```
                     Source Dispatcher (async I/O, tokio)
                +---------+-----------+
                v         v           v
           [Web pool] [API pool] [File readers]
                |         |           |
                +----+----+-----------+
                     v
               Bounded Channel (backpressure)
                     |
      +--------------+--------------+
      v              v              v
  Worker 1       Worker 2       Worker N        <-- rayon / tokio tasks
  +--------+    +--------+     +--------+
  | Parse  |    | Parse  |     | Parse  |
  | NER    |    | NER    |     | NER    |
  | Resolve|    | Resolve|     | Resolve|       <-- read lock only (concurrent)
  | Dedup  |    | Dedup  |     | Dedup  |
  +---+----+    +---+----+     +---+----+
      +--------------+--------------+
                     v
              Batch Accumulator
              (collects N facts or waits T ms)
                     |
                     v
              Single Write Lock
              (bulk insert, one lock acquisition)
                     |
                     v
              Conflict Resolution
              (compare against graph during write lock)
                     |
                     v
              Event Emission
              (FactStored, ConflictDetected, etc.)
```

**Performance characteristics:**
- Source fetching: async I/O, never blocks a thread
- Workers: CPU-bound NER/extraction parallelized across all cores via rayon
- Entity resolution: read lock only -- multiple workers resolve concurrently, zero contention
- Writing: single batch writer, amortizes write lock cost across thousands of facts
- Backpressure: bounded channels prevent memory blowup on large imports
- Process model: multi-threaded within one process (shared `Arc<RwLock<Graph>>`)

**Compute planner integration:**

The v1.0.0 compute planner (`engram-compute`) auto-routes workloads to CPU/GPU/NPU. The ingest pipeline leverages this for two bottleneck operations:

1. **NER inference:** When GPU is available and batch size exceeds threshold (default: 32 texts), the pipeline batches texts and dispatches NER model inference to GPU via the compute planner. This increases throughput from ~800 texts/sec (CPU) to ~16,000 texts/sec (GPU). When GPU is unavailable, workers fall back to CPU inference (per-text, via rayon).

2. **Entity resolution similarity search:** When the graph exceeds 100K entities with embeddings, HNSW cosine similarity search during resolution is dispatched to GPU via the compute planner. Below 100K, CPU SIMD (AVX2/NEON) is sufficient.

The pipeline does not manage GPU directly -- it calls the compute planner, which decides the optimal dispatch target based on workload size, device availability, and current utilization. This is the same pattern used by v1.0.0's `/compute` endpoint.

```toml
[ingest.compute]
ner_gpu_batch_threshold = 32       # batch texts for GPU if queue >= 32
resolution_gpu_threshold = 100000  # use GPU for HNSW if graph > 100K entities
prefer_npu_for_embedding = true    # route embedding generation to NPU if available
```

### 3.4 Pipeline Configuration

Pipelines are defined in TOML for common cases, with code/WASM for advanced custom stages:

```toml
# engram-ingest.toml

[pipeline.default]
name = "standard-ingest"
workers = 8                     # parallel processing threads
batch_size = 1000               # facts per write lock acquisition
batch_timeout_ms = 100          # flush batch after this delay even if not full
channel_buffer = 10000          # backpressure threshold

[pipeline.default.stages]
parse = true
language_detect = true
ner = true
entity_resolve = true
dedup = true
conflict_check = true
confidence_calc = true
```

### 3.5 Entity Resolution (Conservative Strategy)

Entity resolution (ER) is the hardest part of the ingest pipeline: deciding whether an extracted entity ("J. Smith") matches an existing graph node ("John Smith"). **No Rust ER library exists** (3 projects on GitHub vs 194 in Python). Building embedded ER in Rust is a first-of-kind competitive advantage.

**Principle: Never merge without high confidence.** Engram follows a conservative strategy aligned with its core rule of never inventing knowledge:

- **High confidence (>0.90):** Auto-match. Link extracted entity to existing node.
- **Medium confidence (0.60-0.90):** Create entity, add `maybe_same_as` edge to candidate. Human/LLM confirms later.
- **Low confidence (<0.60):** Create new entity. No link.
- **Ambiguous (multiple candidates >0.60):** Create entity, add `maybe_same_as` edges to all candidates. Flag for review.

**Progressive ER Framework (four-step):**

```
Step 1: FILTER (reduce search space)
  - Blocking by entity type (don't compare a PERSON to an ORG)
  - Hash index lookup for exact label match (free -- already built in v1.0.0)
  - Fulltext index for fuzzy label match (free -- already built in v1.0.0)
  - Result: candidate set (dozens, not millions)

Step 2: WEIGHT (similarity scoring)
  - String distance: Jaro-Winkler for names, Levenshtein for codes
  - Embedding cosine similarity (HNSW lookup -- already built in v1.0.0)
  - Property overlap: shared attributes boost score
  - Graph neighborhood overlap: shared neighbors boost score
  - Weighted combination -> match score [0.0, 1.0]

Step 3: SCHEDULE (prioritize execution)
  - Process high-confidence matches first
  - Transitive closure: if A=B and B=C, then A=C (skip redundant comparisons)
  - Priority queue ordered by descending match score

Step 4: MATCH (apply decision)
  - Above threshold: auto-match (link to existing node)
  - Borderline: create maybe_same_as edge (human review)
  - Below threshold: create new node
  - Never merge destructively -- original nodes persist, linked by edges
```

**Resolution result types:**

```rust
pub enum ResolutionResult {
    /// Exact or high-confidence match to existing node
    Matched(NodeId),
    /// New entity, no match found
    New,
    /// Borderline -- candidates exist but confidence insufficient
    Ambiguous(Vec<(NodeId, f32)>),  // (candidate, score)
}
```

**Key constraint:** Resolution runs under a **read lock** during parallel processing. The actual graph mutation (creating nodes, adding `maybe_same_as` edges) happens later in the batch write phase. This means resolution is lock-free for throughput.

---

## 4. NER Engine

### 4.1 Positioning: NER is Ingest, Not Core

NER is part of the ingest pipeline, not part of engram's core functionality. Engram without NER is a fully functional knowledge graph database. Users can store entities manually, via API, via LLM tool-calling, or via structured batch import. NER is only needed when the user wants to extract entities from raw unstructured text.

**Behavior when NER model is not installed:**

- Engram starts normally. All graph operations work.
- `POST /ingest` (raw text) returns a clear error pointing the user to model setup.
- Scheduled web searches store metadata but cannot extract entities from content.
- Frontend Ingest tab shows setup instructions, all other tabs work normally.
- API responses include `X-Engram-NER: not_configured` header on relevant endpoints.

**Model installation is explicit and user-driven:**

```
engram model list ner              Show available models (free + commercial)
engram model install ner <id>      Download and install from registry
engram model install ner custom    Drop in your own ONNX model
```

Engram ships no NER model. The ONNX runtime is included (already there for embeddings). The user selects and downloads a model, accepting its license terms. This cleanly separates engram's proprietary code from third-party model licensing.

### 4.2 Design Principles

- **Facts, not guesses.** NER must produce deterministic, reproducible output. Same input = same output.
- **LLM is last resort.** LLMs hallucinate entities. Statistical models and rules come first.
- **Language-aware.** Every stage adapts to the detected language.
- **Graph-learning.** The knowledge graph teaches the NER, and the NER feeds the graph.
- **Pluggable.** NER model is BYOM (Bring Your Own Model). Any ONNX NER model works. User handles licensing.
- **Exchangeable.** Every NER backend is a trait implementation. Swap at config level.

### 4.3 NER Priority Chain

```
Priority order (first match wins in cascade mode):

  Layer 0: ONNX NER Model       REQUIRED for text ingestion (user installs)
  Layer 1: Graph Gazetteer       AUTO (builds from confirmed entities in graph)
  Layer 2: Custom User Rules     OPTIONAL (domain-specific regex/patterns)
  Layer 3: Learned Patterns      AUTO (grows from graph co-occurrence statistics)
  Layer 4: Disambiguation        AUTO (graph topology resolves ambiguity)
  Layer 5: SpaCy HTTP            OPTIONAL (user runs sidecar)
  Layer 6: LLM Fallback          OPTIONAL (last resort, lowest confidence)
```

### 4.3.1 Model Registry and Installation

Available models are listed in a built-in registry. Users can also provide custom models.

```
Available NER models:

  FREE (open license, commercial use OK):
  ─────────────────────────────────────────────────────────────────
  xlm-roberta-ner-hrl     180 MB  AFL-3.0      10 langs (en,de,zh,ru,ar,fr,es,it,nl,pt)
  gliner-x-base           188 MB  Apache-2.0   20+ langs (zero-shot entity types)
  gliner-moe-multi        600 MB  Apache-2.0   40+ langs (zero-shot, broadest)

  RESTRICTED (non-commercial only):
  ─────────────────────────────────────────────────────────────────
  gliner-multi            634 MB  CC-BY-NC-4.0  6 langs (NON-COMMERCIAL ONLY)

  CUSTOM:
  ─────────────────────────────────────────────────────────────────
  [any ONNX NER model]    --      user's license   user provides .onnx + tokenizer
```

Model files follow the same sidecar pattern as embeddings:

```
~/.engram/models/ner/<model-name>/
    model.onnx
    tokenizer.json
    config.json          # entity types, thresholds, model type
```

Multiple models can be installed. Different pipelines can use different models:

```toml
[[pipelines]]
name = "eu-news"
ner_model = "gliner-x-base"

[[pipelines]]
name = "china-market"
ner_model = "gliner-moe-multi"

[[pipelines]]
name = "internal-docs"
ner_model = "custom-internal-v3"
```

### 4.4 Core Traits

```rust
pub struct ExtractedEntity {
    pub text: String,               // surface form: "Angela Merkel"
    pub entity_type: String,        // PERSON, ORG, LOC, DATE, MONETARY_VALUE...
    pub span: (usize, usize),       // character offsets in source text
    pub confidence: f32,            // extraction confidence
    pub method: ExtractionMethod,   // how it was extracted
    pub language: String,           // language of the surface form
    pub resolved_to: Option<u64>,   // graph node_id if resolved
}

pub enum ExtractionMethod {
    Gazetteer,          // dictionary lookup from graph
    RuleBased,          // regex/pattern match
    LearnedPattern,     // graph-derived co-occurrence rule
    StatisticalModel,   // SpaCy, ONNX model, etc.
    LlmFallback,        // clearly marked, restricted
}

pub trait NerBackend: Send + Sync {
    fn extract(&self, text: &str, lang: &DetectedLanguage) -> Vec<ExtractedEntity>;
    fn supported_languages(&self) -> Vec<String>; // empty = all
    fn name(&self) -> &str;
    fn method(&self) -> ExtractionMethod;
}
```

### 4.5 Chain Strategies

```rust
pub struct NerChain {
    language_detector: Box<dyn LanguageDetector>,
    /// language code -> ordered backends for that language
    pipelines: HashMap<String, Vec<Box<dyn NerBackend>>>,
    /// fallback when no language-specific pipeline exists
    default_pipeline: Vec<Box<dyn NerBackend>>,
    /// resolves ambiguous entities against graph topology
    disambiguator: Box<dyn Disambiguator>,
}

pub enum ChainStrategy {
    /// Stop at first backend that produces results
    Cascade,
    /// Run all backends, merge and deduplicate results
    MergeAll,
    /// Run next backend only if previous found < N entities
    CascadeThreshold(usize),
}
```

### 4.6 Graph Gazetteer (Self-Learning)

Every confirmed entity in the graph (above a confidence threshold) automatically becomes a gazetteer entry. As the graph grows, NER improves without any model retraining.

```rust
pub struct GraphGazetteer {
    graph: Arc<RwLock<Graph>>,
    min_confidence: f32,                      // only entities above this
    cache: DashMap<String, GazetteerEntry>,    // refreshed periodically
    aliases: DashMap<String, String>,          // "Apple Inc" -> "Apple"
    refresh_interval: Duration,
}

pub struct GazetteerEntry {
    pub node_id: u64,
    pub entity_type: String,
    pub confidence: f32,
    pub aliases: Vec<(String, String)>,  // (surface_form, language_code)
}
```

**Multilingual aliases from graph properties:**

```
Graph entity: node_id=42, label="Apple Inc."
  Properties:
    alias:en = "Apple"
    alias:de = "Apple"
    alias:zh = "苹果公司"
    alias:ru = "Эппл"
    alias:ar = "أبل"
    alias:ja = "アップル"
```

When NER runs on Chinese text and finds "苹果公司", the gazetteer maps it directly to node 42. Same entity, different surface form. No statistical model needed.

### 4.7 Disambiguation via Graph Topology

When an entity is ambiguous (e.g., "Mercury" = planet, element, or Roman god), the graph's neighborhood provides context that statistical NER cannot:

```
Text: "Mercury levels in the river exceeded safe limits"

NER finds "Mercury" -> ambiguous

Graph lookup:
  Mercury(planet)  -> edges: orbits->Sun, diameter->4879km
  Mercury(element) -> edges: found_in->thermometer, toxic_to->humans,
                              measured_in->rivers, regulated_by->EPA

Co-occurring entities in text: "river", "limits"
Graph match: Mercury(element) has edges to river-related concepts

  -> Disambiguate to Mercury(element), confidence: high
```

```rust
pub trait Disambiguator: Send + Sync {
    fn disambiguate(
        &self,
        surface_form: &str,
        context: &[&str],            // surrounding extracted entities
        candidates: &[NodeId],       // graph nodes with this label
        graph: &Graph,
    ) -> Option<(NodeId, f32)>;      // best match + confidence
}
```

### 4.8 Learned Patterns from Graph Statistics

Over time, the graph accumulates co-occurrence data. These statistics can generate NER rules -- not invented knowledge, but observed patterns with evidence thresholds:

```rust
pub struct LearnedPattern {
    pub pattern: String,              // regex or token pattern
    pub predicted_type: String,       // PERSON, ORG, etc.
    pub evidence_count: u32,          // how many times observed
    pub accuracy: f32,                // confirmed vs total predictions
    pub min_evidence: u32,            // don't activate until N observations
    pub source: &'static str,        // always "graph_cooccurrence"
}
```

**Guardrails:**
- Only activate after `min_evidence` threshold (default: 50 observations)
- Only if `accuracy` stays above floor (default: 0.85)
- Every prediction logged with pattern provenance
- Human corrections update accuracy, patterns with declining accuracy auto-deactivate

### 4.9 Correction Feedback Loop

```
1. NER extracts "Apple" as FOOD from a tech article
2. Human corrects -> "Apple" is ORG in this context
3. Correction stored in graph:
   - Apple(ORG) confidence increases
   - Context pattern "Apple" + "CEO|iPhone|revenue" -> ORG
     gets +1 evidence count
4. Next time NER sees "Apple" + "revenue" -> ORG, no hesitation
```

```rust
pub struct NerCorrection {
    pub surface_form: String,
    pub wrong_type: String,
    pub correct_type: String,
    pub context_window: Vec<String>,
    pub corrected_by: Provenance,
    pub timestamp: i64,
}
```

### 4.10 Language-Specific NER

NER is fundamentally language-dependent. Tokenization rules, capitalization semantics, entity patterns, and scripts all differ.

#### Language Detection

First step, always. Uses `lingua-rs` (Rust-native, 75+ languages).

```rust
pub struct DetectedLanguage {
    pub code: String,        // "en", "de", "zh", "ar", "ru"
    pub script: Script,      // Latin, Cyrillic, CJK, Arabic...
    pub confidence: f32,
    pub mixed: bool,         // multiple languages in one text
}

pub trait LanguageDetector: Send + Sync {
    fn detect(&self, text: &str) -> DetectedLanguage;
    /// For mixed-language documents: detect per segment
    fn detect_segments(&self, text: &str) -> Vec<(Range<usize>, DetectedLanguage)>;
}
```

#### Language-Specific Challenges

| Language | Challenge | Solution |
|----------|-----------|----------|
| English | Capitalization signals entities | Standard tokenization, cap-based rules |
| German | All nouns capitalized -- caps useless for NER | Compound word splitting, model-dependent |
| Chinese | No spaces, no capitalization | Dictionary segmentation (jieba-rs) |
| Japanese | Mixed scripts (kanji, hiragana, katakana) | Dictionary segmentation (lindera) |
| Arabic | Right-to-left, rich morphology | Morphological analysis, model-dependent |
| Russian | Cyrillic, case declensions change surface form | Lemmatization, model-dependent |
| Mixed | Multiple languages in one document | Per-segment detection and pipeline routing |

#### Per-Language Model Configuration

Users can assign different installed models to different languages, or use a single multilingual model for all:

```toml
# Option A: Single multilingual model (simplest)
[ner.model]
default = "gliner-x-base"          # handles all languages

# Option B: Per-language specialization (best accuracy)
[ner.model]
default = "gliner-x-base"          # fallback for unlisted languages
en = "gliner-x-base"
de = "gliner-x-base"
zh = "gliner-x-base"               # or a Chinese-specialized model
ru = "gliner-x-base"

# Option C: Mixed backends
[ner.model]
default = "gliner-x-base"          # ONNX for most languages

[ner.spacy]                         # SpaCy sidecar for specific languages
enabled = true
endpoint = "http://localhost:8090/ner"
languages = ["zh", "ja"]           # use SpaCy for CJK only
```

#### Per-Language Rule Files

```toml
# rules/de.toml  (German)
[[rules]]
pattern = '\b\w+\s+(GmbH|AG|KG|e\.V\.|SE)\b'
entity_type = "ORG"
confidence = 0.95

# rules/zh.toml  (Chinese)
[[rules]]
pattern = '.+(公司|集团|有限|股份|银行|大学|研究院)'
entity_type = "ORG"
confidence = 0.90

# rules/ru.toml  (Russian)
[[rules]]
pattern = '(ООО|ОАО|ЗАО|ПАО|АО)\s+[«"].*?[»"]'
entity_type = "ORG"
confidence = 0.95
```

### 4.11 NER Backend Options

| Backend | Speed | Accuracy | Dependencies | Use When |
|---------|-------|----------|-------------|----------|
| Graph Gazetteer | Instant | Perfect (known entities) | None (graph itself) | Always (first check) |
| Regex/Rules | Very fast | Domain-specific | None | Domain entities (tickers, codes) |
| Learned Patterns | Fast | Evidence-based | None (graph stats) | After enough data |
| anno (GLiNER2, candle) | Fast | High | anno crate (feature-gated, pure Rust) | Zero-shot NER, in-process, no ort dependency |
| NLI RE (subprocess) | Medium | High | ort + tokenizers (engram-rel binary) | Zero-shot relation extraction, multilingual |
| SpaCy HTTP | Medium | High | Python sidecar | Full NLP pipeline needed |
| LLM Fallback | Slow | Variable | External API | Last resort only |

**Integration pattern for SpaCy:**

SpaCy runs as a thin HTTP sidecar (FastAPI). Engram calls `POST /ner` with text and receives entities. This keeps the Rust binary free of Python dependencies while leveraging SpaCy's excellent models.

**ONNX approach (via `anno`):**

The `anno` crate is the recommended ONNX NER backend. It wraps GLiNER2 models, provides zero-shot entity extraction, coreference resolution, and multi-task extraction (entities + relations in one pass). It uses `candle` (HuggingFace pure Rust ML) as its inference backend, avoiding the heavy `libtorch` dependency.

**Decision:** Use `anno` as a **feature-gated dependency**, isolated behind the `NerBackend` trait in a single file. This gives us GLiNER2, coreference, and candle for free while maintaining full exit strategies.

**Why `anno` over raw `gline-rs` + `ort`:**
- Coreference resolution (pronoun-to-entity linking) -- critical for text ingestion
- Multi-task extraction (entities + relations in one pass) -- reduces processing time
- Candle backend (pure Rust, no C++ deps) -- simpler builds and deployment
- GLiNER v1/v2 model support -- zero-shot entity types without fine-tuning
- Falls back from GLiNER to heuristic NER -- graceful degradation

**Isolation pattern:**

```
engram-ingest/src/ner/
  mod.rs              NerBackend trait (our contract)
  chain.rs            NerChain orchestration
  gazetteer.rs        Graph gazetteer (our code)
  rules.rs            Rule-based NER (our code)
  learned.rs          Learned patterns (our code)
  anno_backend.rs     <-- ONLY file that imports anno
  spacy.rs            SpaCy HTTP backend
  llm.rs              LLM fallback
```

Only `anno_backend.rs` imports the `anno` crate. All other NER code depends only on our `NerBackend` trait. If `anno` breaks or is abandoned, we replace one file.

**Feature gating:**

```toml
# engram-ingest/Cargo.toml
[dependencies]
anno = { git = "...", optional = true, default-features = false, features = ["candle", "analysis"] }

[features]
default = []
anno = ["dep:anno"]    # In-process NER via candle (no ort dependency)
nli-rel = []           # NLI RE via engram-rel subprocess (ort + tokenizers in separate binary)
```

```rust
// anno_backend.rs -- uses candle backend (pure Rust ML, no ONNX Runtime)
#[cfg(feature = "anno")]
pub struct AnnoBackend {
    config: AnnoConfig,
    model: anno::backends::GLiNER2Candle,
    coref: Option<anno::backends::coref::mention_ranking::MentionRankingCoref>,
}
```

**Exit strategies (if `anno` breaks):**
1. **Pin version** -- lock to last known good release, never update
2. **Fork** -- `anno` is MIT/Apache-2.0, fork and maintain ourselves
3. **Extract code** -- copy the GLiNER inference logic into our codebase
4. **Rewrite** -- use `gline-rs` + `ort` directly (lose coreference, gain control)

### 4.12 NER Configuration

```toml
[ner]
strategy = "cascade"               # "cascade", "merge_all", "cascade_threshold"
cascade_threshold = 3              # for cascade_threshold: proceed to next if < N entities
min_confidence = 0.3               # discard entities below this

[ner.gazetteer]
enabled = true
refresh_interval = "5m"            # rebuild from graph every 5 minutes
min_entity_confidence = 0.6        # only graph entities above this confidence

[ner.rules]
files = ["rules/common.toml", "rules/finance.toml", "rules/geopolitics.toml"]

[ner.learned_patterns]
enabled = true
min_evidence = 50                  # observations before activation
min_accuracy = 0.85                # accuracy floor
max_patterns = 1000                # cap to prevent bloat

[ner.model]
default = "urchade/gliner_multi-v2.1"  # HuggingFace model ID, auto-downloaded via candle
# Per-language overrides possible:
# zh = "gliner-moe-multi"
# ja = "gliner-moe-multi"

[ner.spacy]                        # optional SpaCy sidecar
enabled = false
endpoint = "http://localhost:8090/ner"
languages = []                     # empty = all languages via SpaCy

[ner.llm_fallback]
enabled = false                    # off by default
provider = "ollama"
model = "llama3"
max_calls_per_minute = 10
can_trigger_actions = false        # safety: LLM-extracted entities can't fire rules
can_supersede = false              # safety: can't override existing facts

[ner.embeddings]
precision = "int8"                 # "int8" (default for NER), "float16", "float32"
# NER-generated entity embeddings use int8 by default.
# int8 provides 4x memory reduction with ~1% accuracy loss on cosine similarity.
# Sufficient for entity resolution fuzzy matching.
# User-stored entities (via /store, /batch) use the global embedding precision setting.
# At 768 dimensions: float32=3GB/M entities, int8=750MB/M entities.
```

### 4.13 Model Lifecycle Management

```
engram model install ner gliner-x-base   # download + verify checksum
engram model list                         # show installed models
engram model active                       # show currently active NER model
engram model switch ner gliner-moe-multi  # switch to different model
```

**Model upgrade behavior:**
- Running ingest jobs continue with the model loaded at pipeline start
- New ingest jobs use the latest active model
- Switching models does NOT re-run NER on previously ingested text (explicit opt-in only)
- To re-run NER with a new model: `engram reindex --ner` (expensive, full re-extraction)
- Model files stored in `{data_dir}/models/` alongside the `.brain` file

### 4.14 Relation Extraction (NLI-based)

**v1.2.0 update:** Relation extraction uses NLI (Natural Language Inference) instead of GLiREL.

**Why NLI over GLiREL:**
- GLiREL: 1.7GB FP32, English-only, CC BY-NC-SA license
- NLI RE: ~100MB, 100+ languages, MIT license, zero-shot via hypothesis templates

**Algorithm (EMNLP 2021, Sainz et al.):**

For each text chunk after NER + coreference:
1. Extract entity pairs (head, tail) from the same sentence
2. For each entity pair x relation template:
   - Premise: sentence containing both entities
   - Hypothesis: template with entities filled in (e.g., "John works at Google")
   - NLI model classifies: entailment / neutral / contradiction
   - If entailment > threshold (default 0.5) -> emit relation
3. Deduplicate and merge with other RE backends in the chain

**Architecture:** NLI inference runs in a separate `engram-rel` binary (subprocess, JSON Lines protocol) because `tokenizers` (esaxx-rs, static CRT) and `ort_sys` (dynamic CRT) conflict on Windows MSVC debug builds. NER is in-process (candle, latency-sensitive). RE runs once per chunk (subprocess overhead negligible).

**Relation templates (21 defaults):**

Users can customize templates in system settings. Default set sourced from TACRED/FewRel/Wikidata:
`works_at`, `born_in`, `lives_in`, `educated_at`, `spouse`, `parent_of`, `child_of`, `citizen_of`, `member_of`, `holds_position`, `founded_by`, `headquartered_in`, `subsidiary_of`, `acquired_by`, `located_in`, `instance_of`, `part_of`, `capital_of`, `cause_of`, `author_of`, `produces`.

Template format: `"{head} works at {tail}"` -- natural language hypothesis with `{head}` and `{tail}` placeholders.

**NLI models (recommended):**

| Model | Size | Speed | Languages |
|-------|------|-------|-----------|
| multilingual-MiniLMv2-L6-mnli-xnli | ~100MB | Fast (~5ms/pair CPU) | 100+ |
| mDeBERTa-v3-base-xnli | ~280MB | Medium | 100+ |

**Performance:** O(entity_pairs x templates) NLI calls. Typical: 5 entities -> 20 pairs x 21 templates = 420 calls at ~5ms = ~2s per chunk. Acceptable for batch ingest.

### 4.15 Coreference Resolution

Coreference runs after NER, before RE. Resolves pronouns and noun phrases to canonical entity names.

**Backends (rule-based, no model download):**

| Backend | Feature gate | Approach |
|---------|-------------|----------|
| MentionRankingCoref | always available | Antecedent ranking, acronyms, "be-phrase" patterns |
| SimpleCorefResolver | `analysis` feature | 9-sieve cascade (exact match, head match, pronoun, fuzzy) |

**Pipeline:**
```
NER output: ["John Smith", "He", "Apple", "the company"]
     -> Coreference -> ["John Smith", "John Smith", "Apple", "Apple"]
     -> RE input uses canonical names
```

Coreference is enabled by default and configured via `coreference_enabled` in `EngineConfig`.

---

## 5. Confidence Model

### 5.1 Learned Trust (No Hardcoded Tiers)

**Design principle:** Engram does not pre-judge sources. There are no hardcoded trust tiers ("government = 0.50", "social media = 0.10"). Every source and every author starts at the same baseline. The graph learns who to trust from evidence -- corroboration raises trust, correction lowers it.

This eliminates human bias from the trust model. RT and Reuters start equal. The graph observes which sources' facts get corroborated by independent evidence and which get corrected. Trust emerges from the data, not from a configuration file.

```
initial_confidence = effective_trust * extraction_confidence

effective_trust resolution (most specific wins):
  1. author_trust    (if author is known and has enough history)
  2. source_trust    (if source node exists and has enough history)
  3. global_baseline (configurable, default 0.15)
```

**Only two exceptions** where trust is set differently:
- **Human-confirmed facts** (`POST /store` with `source: "user:alice"`) enter at 0.90. A human explicitly storing a fact is a direct assertion, not an inference.
- **LLM-generated facts** are capped at 0.10 regardless of learned trust. LLMs hallucinate; this is a safety rail, not a bias.

Everything else -- news agencies, social media, government sites, blogs, academic papers -- starts at baseline and earns its trust.

### 5.2 Trust Hierarchy: Source -> Author -> Fact

Sources and authors are **first-class nodes in the graph**, subject to the same learning mechanics as any other entity.

```
Trust hierarchy (graph structure):

  Source:x.com                     (platform-level trust, learned)
    |
    +-- Author:twitter:@analyst_1  (author-level trust, learned)
    |     |
    |     +-- Fact:claim_A         (confidence = author_trust * extraction_conf)
    |     +-- Fact:claim_B
    |
    +-- Author:twitter:@bot_42     (author-level trust, learned)
          |
          +-- Fact:claim_C         (confidence = author_trust * extraction_conf)


  Source:reuters.com               (platform-level trust, learned)
    |
    +-- Fact:claim_D               (no author extracted, uses source_trust)
```

#### Source Lifecycle

```
1. FIRST ENCOUNTER
   When ingest processes a new source domain for the first time:
     Store node: "Source:{domain}" (e.g. "Source:reuters.com")
     Set confidence = global_baseline (e.g. 0.15)
     Edge: fact --from_source--> Source:{domain}

2. CORROBORATION (auto-adjust UP)
   When facts from this source get confirmed by independent sources:
     reinforce_confirm(source_node) → existing learning mechanics
     Source confidence rises: 0.15 → 0.20 → 0.28 → ...

3. CORRECTION (auto-adjust DOWN)
   When facts from this source are corrected:
     correct() propagates distrust to source node
     Source confidence drops: 0.15 → 0.12 → 0.08 → ...

4. DECAY
   Source nodes decay like any other node. A source that stops
   publishing gradually loses accumulated trust.
```

#### Author Lifecycle

```
1. FIRST ENCOUNTER
   Ingest extracts author from source metadata (Twitter API → screen_name,
   RSS → <author>, GDELT → no author info).

   If author is new:
     Store node: "Author:{source}:{author_id}"
     Set confidence = global_baseline (same as everyone else)
     Edge: fact --authored_by--> Author:{source}:{author_id}
     Edge: Author:{source}:{author_id} --publishes_on--> Source:{domain}

2. CORROBORATION (auto-adjust UP)
   When a fact from this author gets confirmed by an independent source:
     reinforce_confirm(author_node) → existing learning mechanics
     Author confidence rises: 0.15 → 0.20 → 0.30 → ...

3. CORRECTION (auto-adjust DOWN)
   When a fact from this author is corrected:
     correct() propagates distrust to neighbors → author is a neighbor
     Author confidence drops. Repeated corrections sink them.

4. DECAY
   Author nodes are subject to normal decay. An author who stops publishing
   gradually loses accumulated trust. Re-establishes trust on next
   corroborated fact.
```

#### Author-Scoped Trust

Author trust is **per source platform**, not global:

```
After 3 months of operation:

  Author:twitter:@analyst_jane    → 0.65  (many corroborated OSINT threads)
  Author:substack:@analyst_jane   → 0.40  (fewer, less verified articles)
  Author:reddit:u/analyst_jane    → 0.15  (baseline, no corroboration yet)

  Source:reuters.com               → 0.72  (consistently corroborated)
  Source:rt.com                    → 0.08  (frequently corrected)
  Source:x.com                     → 0.14  (mixed, near baseline)
```

Same person, different reliability per platform. Same platform, different reliability per author. The graph captures both dimensions without any manual configuration.

#### What Sources Provide Author Info

| Source | Author field | Example |
|--------|-------------|---------|
| Twitter/X API | `author_id`, `username` | `Author:twitter:@IABORZDEH` |
| RSS/Atom feeds | `<author>`, `<dc:creator>` | `Author:rss:John Smith` |
| GitHub API | `login` | `Author:github:torvalds` |
| arXiv | `authors[]` | `Author:arxiv:Y. LeCun` |
| GDELT | none | Falls back to source-level trust |
| Web scraping | varies | Best-effort extraction, falls back to source trust |

When no author can be extracted, the system falls back to source-level trust. No author node is created.

#### Why This Is Better Than Hardcoded Tiers

| Hardcoded tiers | Learned trust |
|----------------|---------------|
| "RT = 0.05" is human bias baked into config | RT starts equal, sinks if facts get corrected |
| Can't distinguish good/bad authors on same platform | Per-author trust within each platform |
| Static -- doesn't adapt to quality changes | Dynamic -- tracks source quality over time |
| Requires manual maintenance | Self-maintaining via corroboration/correction |
| Can be gamed by knowing the tier list | Can only be gamed by producing corroborated facts |
| Censorship risk (blocking sources by policy) | No censorship -- low-trust facts still enter graph |

### 5.3 Corroboration Boost

When multiple independent sources agree on a fact, confidence increases:

```
corroboration_boost = min(0.15, 0.05 * (source_count - 1))
```

"Independent" means different source nodes (not same source, not same author across platforms linked via `same_as`). 10 bot accounts from the same coordinated cluster do NOT count as 10 independent sources -- co-occurrence detection (see Use Case 23) catches this.

### 5.4 Confidence by Extraction Method

```
Method weights (multiply with effective_trust):

  Gazetteer:          1.0    exact match, no doubt
  RuleBased:          0.9    pattern matched, very likely
  LearnedPattern:     0.8    evidence-based, statistically validated
  StatisticalModel:   0.7    SpaCy/ONNX, good but not perfect
  LlmFallback:        0.3    useful for discovery, not authoritative
```

### 5.5 Trust Configuration

```toml
[trust]
global_baseline = 0.15           # starting trust for all new sources/authors
human_confirmed = 0.90           # trust for direct human assertions (POST /store)
llm_cap = 0.10                   # hard cap on LLM-generated fact trust
min_facts_for_divergence = 3     # source/author needs 3+ facts before trust moves
cap = 0.85                       # learned trust never exceeds this (only human exceeds)
```

No source registry. No tier assignments. No manual trust management. The graph learns.

---

## 6. Conflict Resolution

When incoming data contradicts existing data in the graph, engram **never silently overwrites**.

### 6.1 Conflict Cases

| Case | Condition | Action |
|------|-----------|--------|
| **New supersedes** | new_conf > old_conf + 0.1 | Old fact gets `valid_until=now`. New becomes active. Audit edge: `superseded_by` |
| **Alternative** | new_conf < old_conf - 0.1 | Store as "contested" alternative. Don't touch active fact. Edge: `contested_by` |
| **Disputed** | abs(new_conf - old_conf) <= 0.1 | Flag for human review. Both stored, neither authoritative. Edge: `conflicts_with` |
| **Temporal succession** | Both true at different times | Bi-temporal timestamps handle this. Edge: `succeeded_by` with `event_time` |

### 6.2 Conflict Detection

```rust
pub struct ConflictRecord {
    pub existing_node_id: u64,
    pub existing_confidence: f32,
    pub incoming_confidence: f32,
    pub conflict_type: ConflictType,
    pub resolution: ConflictResolution,
    pub detected_at: i64,
}

pub enum ConflictType {
    Contradiction,       // facts are mutually exclusive
    ValueDifference,     // same entity, different property values
    TypeMismatch,        // same label, different entity types
    TemporalOverlap,     // same fact, overlapping time ranges
}

pub enum ConflictResolution {
    Superseded,          // new replaced old
    Contested,           // stored as alternative
    Disputed,            // flagged for human review
    TemporalSuccession,  // both valid at different times
    Merged,              // properties merged from both
}
```

### 6.3 Audit Trail

Every conflict and resolution is stored as edges in the graph:

```
(New Fact) --[superseded_by]--> (Old Fact)
  Properties:
    resolution: "superseded"
    old_confidence: 0.3
    new_confidence: 0.7
    resolved_at: 1741582400
    resolved_by: "ingest_pipeline"
```

Full traceability: "Why does engram believe X?" can always be answered by traversing provenance and conflict edges.

---

## 7. Action Engine (`engram-action`)

### 7.1 Event Bus

Every graph mutation emits a lightweight event through a bounded channel. The action engine consumes events, evaluates rules, and dispatches effects.

```rust
pub enum GraphEvent {
    FactStored {
        node_id: u64,
        label: Arc<str>,
        confidence: f32,
        source: Arc<str>,
        entity_type: Option<Arc<str>>,
    },
    FactUpdated {
        node_id: u64,
        old_confidence: f32,
        new_confidence: f32,
    },
    EdgeCreated {
        edge_id: u64,
        from: u64,
        to: u64,
        rel_type: Arc<str>,
    },
    ConflictDetected {
        existing: u64,
        incoming: u64,
        conflict_type: ConflictType,
    },
    QueryGap {
        query: Arc<str>,
        result_count: usize,
        avg_confidence: f32,
    },
    TimerTick {
        rule_id: Arc<str>,
    },
    ThresholdCrossed {
        node_id: u64,
        old_confidence: f32,
        new_confidence: f32,
        direction: ThresholdDirection, // Up or Down
    },
}
```

Events are cheap (enum + Arc for strings, no cloning of graph data). The channel is bounded to prevent memory blowup -- if the action engine can't keep up, events are logged and dropped (never block the graph).

**Additional event types for v1.1.0 auditing:**

```rust
    // Action engine events (for SSE subscription and audit)
    ActionFired {
        rule_id: Arc<str>,
        trigger_event: Box<GraphEvent>,
        effects_count: u32,
    },
    ActionCompleted {
        rule_id: Arc<str>,
        success: bool,
        duration_ms: u64,
    },
    ActionFailed {
        rule_id: Arc<str>,
        error: Arc<str>,
    },
    // Ingest events
    IngestJobCreated {
        job_id: Arc<str>,
        origin: IngestOrigin,  // UserConfigured, Intelligence, OnDemand
        query_count: u32,
    },
    IngestJobCompleted {
        job_id: Arc<str>,
        facts_stored: u64,
        conflicts: u32,
        duration_ms: u64,
    },
```

These enable the frontend dashboard to subscribe via SSE and show real-time action/ingest activity without polling.

**Event bus sizing and overflow:**

```toml
[events]
channel_capacity = 10000           # bounded channel size
overflow = "log_and_drop"          # "log_and_drop" (default) or "backpressure"
```

- `log_and_drop`: events exceeding capacity are logged to stderr with full context, then dropped. Graph operations never block. Acceptable for most workloads.
- `backpressure`: graph mutation blocks until channel has space. Use only when action rules are critical and must not miss events. Slows ingest throughput.

Under high ingest load (100K facts/sec), `log_and_drop` is the safe default. If action rules are mission-critical, use `backpressure` but accept reduced throughput.

### 7.1.1 Execution Model: Inference -> Learning -> Action

When a fact is stored, three subsystems react in a defined order. This prevents unbounded loops and ensures predictable behavior.

```
Fact stored (via ingest, /store, /batch, or mesh sync)
  │
  ▼
PHASE 1: LEARNING ENGINE (synchronous, in write lock)
  │ If fact corroborates existing fact: reinforcement (+0.10)
  │ Update co-occurrence counters for all entities in same document
  │ These are cheap O(1) operations, no graph traversal
  │
  ▼
PHASE 2: INFERENCE ENGINE (synchronous, runs to fixpoint)
  │ Forward-chaining rules evaluate against new fact
  │ Derived facts are stored with source="rule:{id}"
  │ Derived facts may trigger MORE inference rules (chaining)
  │ Runs until fixpoint (no new facts derivable) or depth limit
  │ Derived facts also go through Phase 1 (learning updates)
  │ DOES NOT re-enter Phase 2 for derived facts (fixpoint = done)
  │
  ▼
PHASE 3: EVENT EMISSION
  │ Emit GraphEvent::FactStored for the original fact
  │ Emit GraphEvent::FactStored for each derived fact (batched)
  │ All events enter the bounded channel at once
  │
  ▼
PHASE 4: ACTION ENGINE (asynchronous, off the write lock)
  │ Consumes events from channel
  │ Evaluates rules against each event
  │ Fires effects (webhook, enrichment, dynamic ingest job, etc.)
  │ Action effects that store NEW facts re-enter at Phase 1
  │ But: action-derived facts DO NOT re-trigger the same action rule
  │ (cooldown per entity prevents this)
```

**Loop prevention guarantees:**
- **Inference:** fixpoint detection. If no new facts are derived, stop. Max depth configurable (default: 10).
- **Action:** cooldown per entity per rule. Same entity cannot re-fire the same rule within cooldown window.
- **Cross-system:** action effects that store facts go through inference (Phase 2), but inference-derived facts only trigger action rules (Phase 4), never re-enter inference for the same transaction.
- **Hard cap:** maximum total facts created per original store operation (default: 1000). If exceeded, remaining derivations are queued for next cycle.

### 7.2 Triggers

What can trigger an action:

| Trigger | Example |
|---------|---------|
| Fact stored | New entity appears in graph |
| Confidence change | Fact drops below 0.3 (stale) |
| Conflict detected | Two sources disagree |
| Threshold crossed | Confidence rises above 0.9 (firm) |
| Pattern match | 3+ entities share a trait |
| Query gap detected | Search returns too few / too low confidence results |
| Temporal expiry | Fact older than N days without re-confirmation |
| Scheduled | "Every 6h, re-check topic X" |
| External event | Webhook received |
| Graph topology change | Node becomes hub (>N edges) |

### 7.3 Effects

What an action can do:

**Internal effects (within engram):**
- Trigger enrichment (spearhead search)
- Recalculate confidence (cascade propagation)
- Create derived edges (with `source: "rule:{rule_id}"`)
- Move fact between tiers (active -> archival)
- Flag for human review
- Trigger another rule (chaining, with depth limit)

**External effects (outside engram):**
- Webhook (POST to URL with payload)
- MCP tool call
- Message notification (Slack, email)
- API call (generic REST/GraphQL)
- Write to file/stream
- Trigger ingest pipeline (go fetch more data)
- Create dynamic ingest job (intelligence-driven, queries derived from graph context)

**Dynamic ingest job creation:**

```rust
pub enum ActionEffect {
    // ... existing effects ...

    /// Create a new ingest job dynamically, with queries derived from graph context.
    CreateIngestJob {
        /// Queries derived from graph edges around the triggering entity.
        queries: Vec<String>,
        /// Which sources to use.
        sources: Vec<String>,
        /// Template for generating queries from graph topology.
        query_template: Option<QueryTemplate>,
        /// Link back to the triggering event for provenance.
        parent_event: String,
        /// Priority (affects scheduling order).
        priority: Priority,
        /// How to reconcile results against existing knowledge.
        reconcile: ReconcileStrategy,
    },
}

pub struct QueryTemplate {
    /// Template with graph variables: "{entity} {relationship_type} {target} impact"
    pub template: String,
    /// How many hops from triggering entity to walk for context.
    pub graph_depth: u32,
    /// Which relationship types to follow.
    pub edge_filter: Option<Vec<String>>,
}

pub enum ReconcileStrategy {
    /// Normal ingest, no special handling.
    Standard,
    /// Compare new facts against an existing entity's facts.
    DeltaAgainstEntity(String),
    /// New version supersedes old: mark contradicted facts as superseded.
    VersionSupersede { old_version: String },
}
```

Dynamic jobs appear in the `/sources/ledger` API and frontend dashboard, tagged as `origin: intelligence` (vs `origin: user_configured`). Facts produced by dynamic jobs age via the normal confidence decay system -- no separate TTL needed.

### 7.4 Rule Definition

```toml
# rules/actions.toml

[[rules]]
id = "sanctions-alert"
trigger = "fact_stored"
condition = """
  entity_type == "sanction"
  && label matches "russia*"
  && confidence > 0.4
"""
actions = [
    { type = "enrich", query = "{entity}", sources = ["gov-sanctions-db"] },
    { type = "webhook", url = "https://internal/alerts", payload = "fact" },
    { type = "create_edge", from = "{entity}", to = "active-alerts", rel = "flagged" },
]
cooldown = "1h"
priority = "high"
enabled = true

[[rules]]
id = "stale-fact-decay"
trigger = "timer"
schedule = "every 6h"
condition = """
  days_since_confirmed > 30
  && confidence > 0.3
  && memory_tier != "core"
"""
actions = [
    { type = "reduce_confidence", factor = 0.95 },
    { type = "enrich", query = "{entity}", sources = ["web"], mode = "eager" },
]
cooldown = "24h"
priority = "low"
enabled = true

[[rules]]
id = "query-gap-enrich"
trigger = "query_gap"
condition = """
  result_count < 3
  && avg_confidence < 0.4
"""
actions = [
    { type = "enrich", query = "{query}", sources = ["web", "news"], mode = "eager" },
]
cooldown = "30m"
priority = "medium"
enabled = true
```

### 7.5 Safety Constraints

- **Cooldown:** Rules cannot re-fire for same entity within cooldown period. Prevents infinite loops and spam.
- **Max chain depth:** Action triggers action triggers action... capped at configurable depth (default: 5).
- **Effect budget:** Max N external calls per minute per rule (default: 30).
- **Dry run mode:** Evaluate rules, log what would happen, don't execute.
- **LLM restriction:** Facts with `method: LlmFallback` cannot trigger actions by default.
- **Audit trail:** Every action execution logged: trigger, condition evaluation, effects executed, results.

---

## 8. Black Area Detection (`engram-reason`)

### 8.1 Concept

In a knowledge graph, what's **missing** is as informative as what's present. "Black areas" are structural gaps, missing links, and knowledge frontiers that indicate where investigation should focus.

No competitor system detects its own ignorance. Engram maps what it doesn't know and can act on it.

### 8.2 Types of Black Areas

| Pattern | Detection Method | Severity Scoring |
|---------|-----------------|-----------------|
| **Frontier node** | Entity with <= 2 edges, dangling at edge of knowledge | Based on entity type importance |
| **Structural hole** | A->B and B->C exist but A->C doesn't. Expected link missing. | Based on relationship type patterns |
| **Asymmetric cluster** | Topic X has 50 facts, closely related topic Y has 2 | Ratio of coverage between related topics |
| **Temporal gap** | Facts about entity stop at a date. Nothing after. | Based on expected update frequency |
| **Type gap** | People, orgs, locations exist but no events connecting them | Based on expected relationship patterns |
| **Confidence desert** | Cluster where all facts are low confidence | Average confidence in cluster |
| **Coordinated cluster** | Dense internal edges, sparse external, low avg author trust, temporal sync | Internal/external edge ratio * (1 - avg_author_trust) * temporal_sync_score |
| **Isolated cluster** | Group of entities with zero connections to rest of graph | Cluster size and entity importance |

### 8.3 Detection Algorithm

```
For every node in the graph:
  1. Degree analysis: count edges. Low degree = frontier candidate.
  2. Neighbor density: if neighbors are densely connected to each
     other but sparse toward this node = structural hole.
  3. Temporal freshness: last update > N days = temporal gap.
  4. Cluster comparison: this node's topic has N facts, sibling
     topic has 10N = asymmetric coverage.

For every pair of related clusters:
  5. Expected connections (based on shared entity types)
     vs actual connections. Big delta = missing link territory.

Score and rank all detected black areas.
Emit as GraphEvent::BlackAreaDetected -> ACTION system picks them up.
```

### 8.4 Data Model

```rust
pub struct BlackArea {
    pub kind: BlackAreaKind,
    pub entities: Vec<u64>,            // involved node IDs
    pub severity: f32,                 // 0.0 = minor gap, 1.0 = critical blind spot
    pub suggested_queries: Vec<String>, // what to search for to fill the gap
    pub domain: Option<String>,        // topic cluster
    pub detected_at: i64,
    pub last_checked: i64,
}

pub enum BlackAreaKind {
    FrontierNode,
    StructuralHole,
    AsymmetricCluster,
    TemporalGap,
    TypeGap,
    ConfidenceDesert,
    IsolatedCluster,
    /// Dense internal edges, sparse external, low author trust.
    /// Signals potential coordinated network (influence ops, bot farms).
    CoordinatedCluster,
}
```

### 8.5 Integration with Action Engine

Black areas become enrichment triggers:

```
Black area detected: "Russia sanctions" cluster has 40 facts,
                     "Russia titanium supply chain" has 2 facts.
                     Severity: 0.8 (asymmetric cluster)

  -> ACTION: EnrichmentTrigger
  -> Suggested queries: ["Russia titanium exports sanctions",
                          "titanium supply chain disruption"]
  -> Spearhead search fans out to web, news, gov DB
  -> Results flow through ingest pipeline
  -> New facts stored, black area re-evaluated
```

### 8.6 API Endpoints

```
GET  /reason/gaps              List all detected black areas, ranked by severity
GET  /reason/gaps/{id}         Details of a specific black area
POST /reason/scan              Trigger black area detection (or it runs on schedule)
GET  /reason/frontier          List frontier nodes (entities at edge of knowledge)
GET  /reason/coverage/{topic}  Coverage analysis for a specific topic
```

---

## 9. Query-Triggered Enrichment

### 9.1 Concept

When a query reveals insufficient local knowledge, engram can fan out to external sources to fill the gap. This is **not** proactive crawling -- it's on-demand, triggered by an actual information need.

### 9.2 Spearhead Search Pattern

```
User/LLM asks: "What sanctions apply to Russian titanium exports?"

     engram local search
            |
            v
    Results: 2 facts, avg confidence 0.3
            |
            v
    Threshold not met -> ENRICH
            |
            v
    ┌───────────────────────────────────┐
    │ TIER 1: MESH FEDERATED QUERY     │  FREE, fast, pre-verified
    │ Search across mesh peer graphs   │  No copying, live results
    │ Match by knowledge profile topic │
    └───────────────┬───────────────────┘
                    │
                    v
            Sufficient? ──yes──> Return merged results
                    │
                    no
                    │
    ┌───────────────┴───────────────────┐
    │ TIER 2: EXTERNAL FREE SOURCES    │  Free APIs, moderate speed
    │ GDELT, RSS, SearXNG, Semantic    │  Results go through ingest
    │ Scholar, public databases        │  pipeline (full processing)
    └───────────────┬───────────────────┘
                    │
                    v
            Sufficient? ──yes──> Return merged results
                    │
                    no
                    │
    ┌───────────────┴───────────────────┐
    │ TIER 3: EXTERNAL PAID SOURCES    │  Costly, last resort
    │ Brave Search, Recorded Future,   │  Usage endpoint checked
    │ NewsAPI, commercial intel        │  before each call
    └───────────────┬───────────────────┘
                    │
                    v
              Return all merged results
              (each result tagged with source provenance and tier)
```

**Tier 1 (mesh) is always tried first** because:
- Zero cost (peer-to-peer, no API charges)
- Facts are already NER'd, resolved, confidence-scored by the peer
- No ingest pipeline needed (results are structured facts, not raw text)
- Peer trust is typically higher than web sources
- Live query = always fresh (no stale copies)

### 9.2.1 Mesh Federated Query

Instead of copying facts from peers into the local graph, engram searches across peer graphs and merges results. Each node keeps only what it owns.

```
russia-desk query: "Iran titanium supply chain"

Step 1: Local graph search → 2 facts (sparse)

Step 2: Check mesh knowledge profiles
  → iran-desk: covers "Iran" (50K facts, depth 0.9)
  → Match! Send federated query to iran-desk

Step 3: iran-desk searches its own graph
  → Returns 15 facts about Iran titanium, confidence-scored
  → Results tagged: source="mesh:iran-desk"

Step 4: Merge
  → 2 local + 15 remote = 17 facts
  → Ranked by confidence, source provenance preserved
  → No facts copied into russia-desk's graph
```

**Federated query protocol:**

```rust
/// Sent from querying node to peer.
pub struct FederatedQuery {
    pub query: String,
    pub query_type: QueryType,        // semantic, fulltext, hybrid
    pub max_results: u32,
    pub min_confidence: f32,
    pub requesting_node: PeerId,
    pub sensitivity_clearance: String, // "public", "internal" -- ACL filter
}

/// Returned from peer.
pub struct FederatedResult {
    pub facts: Vec<FederatedFact>,
    pub peer_id: PeerId,
    pub query_time_ms: u64,
    pub total_matches: u64,           // may be more than returned
}

pub struct FederatedFact {
    pub label: String,
    pub entity_type: String,
    pub properties: HashMap<String, String>,
    pub confidence: f32,
    pub edges: Vec<FederatedEdge>,    // relationships to include
    pub provenance: Provenance,       // original source attribution
    // NOT included: internal node IDs (meaningless outside peer)
}
```

**When to federated query vs selective sync:**

| Pattern | Use Federated Query | Use Selective Sync |
|---------|--------------------|--------------------|
| Ad-hoc search | Yes -- search, merge, discard | No |
| One-time enrichment | Yes -- get results, move on | No |
| Ongoing monitoring of cross-domain topic | Query first, then... | Sync high-value intersection facts |
| Offline access needed | No | Yes -- need local copies |
| Action rules depend on peer facts | No | Yes -- rules need local facts to trigger |
| Dashboard metrics on peer domains | Query periodically | No |

**Selective sync remains for specific cases:** When a node needs certain peer facts locally (offline access, local rule triggers, performance), it can still request sync. But the default is federated query -- ask, don't copy.

```
# Selective sync config (opt-in, specific topics only)
[mesh.sync]
mode = "selective"                    # "selective" (default) or "full"

[[mesh.sync.subscriptions]]
peer = "iran-desk"
topics = ["Iran-Russia", "sanctions"]  # only sync facts matching these topics
min_confidence = 0.7                   # only high-confidence facts
```

### 9.2.2 Mesh Knowledge Discovery

Each mesh node automatically announces its areas of knowledge so peers can find the right node to query.

**Knowledge profile (auto-derived from graph):**

```rust
pub struct KnowledgeProfile {
    pub node_id: PeerId,
    pub name: String,                        // "iran-desk"
    pub domains: Vec<DomainCoverage>,        // auto-derived from graph clusters
    pub total_facts: u64,
    pub last_updated: i64,
    pub capabilities: Vec<NodeCapability>,
}

pub struct DomainCoverage {
    pub topic: String,                       // "Iran", "sanctions", "oil"
    pub fact_count: u64,
    pub avg_confidence: f32,
    pub freshness: i64,                      // most recent fact timestamp
    pub depth: f32,                          // 0.0-1.0, coverage density
}

pub enum NodeCapability {
    NerAvailable(Vec<String>),               // NER languages supported
    GpuCompute,                              // can handle heavy workloads
    SourceAccess(Vec<String>),               // paid sources available
    HighAvailability,                        // always online
}
```

**Auto-derivation (no manual configuration):**

```
On schedule (or on significant graph change):
  1. Run cluster analysis on graph topology
  2. Extract top-N topic clusters by fact count
  3. Calculate coverage metrics (fact count, avg confidence, freshness, depth)
  4. Diff against last published profile
  5. If changed: broadcast via gossip protocol
```

A fresh engram instance starts ingesting Iran news. After accumulating enough facts, it automatically announces "I cover Iran" to the mesh. Zero manual domain registration.

**Discovery protocol:**

```
Gossip messages (v1.0.0 existing):
  FactDelta         new/changed facts for selective sync
  BloomFilter       efficient "do you have X?" checks

Gossip messages (v1.1.0 additions):
  KnowledgeProfile  "here's what I cover" (broadcast periodically)
  ProfileQuery      "who covers Iran?" (broadcast, peers respond)
  FederatedQuery    "search your graph for X" (directed to specific peer)
  FederatedResult   "here are the matching facts" (response)
```

**Discovery API:**

```
GET  /mesh/profiles                     All known peer profiles
GET  /mesh/discover?topic=Iran          Find peers covering a topic
GET  /mesh/discover?query=titanium      Semantic match against peer domains
GET  /mesh/profile                      This node's own profile
POST /mesh/profile/refresh              Force profile recalculation
POST /mesh/query                        Execute federated query across mesh
```

**Frontend: Mesh Topology View**

```
+--------------------------------------------------------------+
|  <i class="fa fa-project-diagram"></i> MESH NETWORK                              [Refresh]    |
+--------------------------------------------------------------+
|                                                               |
|  [iran-desk]          [russia-desk]        [china-desk]      |
|   Iran (50K)  <------>  Russia (80K)  <--->  China (35K)     |
|   sanctions            military             trade            |
|   oil, energy          sanctions            tech             |
|   nuclear              energy               maritime         |
|                                                               |
|  Coverage overlap: iran-desk <-> russia-desk: 12%            |
|  (sanctions, energy, military topics)                        |
|                                                               |
|  <i class="fa fa-exclamation-circle"></i> Uncovered: South America, Africa, SE Asia             |
|                                                               |
|  Federated queries today: 47 | Avg response: 120ms          |
+--------------------------------------------------------------+
```

The "uncovered areas" detection extends black area analysis to the mesh level -- not just what this node doesn't know, but what the entire team doesn't cover.

### 9.3 Response Modes

| Mode | Behavior | API Parameter |
|------|----------|---------------|
| **Eager** (default) | Return local results immediately. Enrich in background. Next query is richer. | `?enrich=eager` |
| **Await** | Hold response. Fan out to sources. Return combined results. | `?enrich=await` |
| **None** | Only local results, no enrichment. | `?enrich=none` |

### 9.4 Source Taxonomy and Capabilities

Each external source has different characteristics. Rather than treating all sources the same, each source connector declares its capabilities so the ledger and scheduler can adapt.

| Source Type | Example | Auth | Temporal Cursor? | Cost Model | Native Dedup |
|-------------|---------|------|-------------------|------------|-------------|
| Web search API | Google, Bing, Brave | API key | Yes (date range) | Per query | No |
| News API | GDELT, NewsAPI, Mediastack | API key / free | Yes (date range) | Per query or subscription | No |
| Social media | X/Twitter, Reddit, Mastodon | OAuth2 / Bearer | Yes (since_id) | Free tier + paid | Yes (since_id) |
| RSS/Atom feeds | Any feed URL | None | Yes (ETag/Last-Modified) | Free | Yes (ETag + GUID) |
| Paid intelligence | Recorded Future, Janes, Stratfor | Bearer / API key | Varies | Per-seat / per-query | No |
| Webhook (push) | Incoming, not pulled | Shared secret | N/A (they push) | Free | N/A |
| Database/API | REST endpoints, GraphQL | Various | Pagination token | Varies | Varies |

#### Source Trait

```rust
pub trait Source: Send + Sync {
    /// Fetch data from the source.
    async fn fetch(&self, params: &SourceParams) -> Result<Vec<RawItem>>;

    /// Query the source's own usage/billing endpoint.
    /// Returns None if the source has no usage endpoint.
    async fn usage(&self) -> Result<Option<UsageReport>>;

    fn name(&self) -> &str;
    fn source_type(&self) -> SourceType;

    /// Declare what this source supports so the scheduler can adapt.
    fn capabilities(&self) -> SourceCapabilities;
}

pub struct SourceCapabilities {
    pub supports_temporal_cursor: bool,   // date range or since_id
    pub supports_etag: bool,              // HTTP conditional requests (304 Not Modified)
    pub supports_pagination_token: bool,  // resume from last page
    pub has_native_dedup: bool,           // source guarantees no dupes (e.g. since_id)
    pub cost_model: CostModel,
    pub rate_limit: Option<RateLimit>,
}

pub enum CostModel {
    Free,
    PerQuery(f64),          // cost per API call
    PerResult(f64),         // cost per result returned
    PerByte(f64),           // cost per byte transferred
    Subscription,           // flat rate, no per-query cost
    QuotaBased(u64),        // N free calls per period
}

pub struct UsageReport {
    pub calls_used: Option<u64>,
    pub calls_limit: Option<u64>,
    pub cost_used: Option<f64>,
    pub cost_limit: Option<f64>,
    pub reset_at: Option<i64>,          // epoch timestamp
    pub raw: serde_json::Value,         // full provider response for transparency
}
```

Each source connector implements the `usage()` method by calling the provider's own usage/billing API endpoint and mapping the provider-specific response into the `UsageReport` struct. Engram does **not** calculate costs itself -- it queries the source.

### 9.5 Search Ledger

The search ledger is the central mechanism for minimizing API calls and processing time. It tracks every query-source combination and enables four dedup layers.

#### 9.5.1 Ledger Record

```rust
pub struct LedgerRecord {
    pub query: String,                  // the search query
    pub query_hash: u64,                // FNV-1a hash for fast lookup
    pub source: String,                 // source name (e.g. "brave-search")
    pub last_run: i64,                  // epoch timestamp of last execution
    pub run_count: u32,                 // how many times this query+source ran
    pub temporal_cursor: Option<String>,// last date range end / since_id / ETag
    pub result_hashes: HashSet<u64>,    // content hashes of all seen results
    pub total_results_seen: u64,        // cumulative results returned by API
    pub total_ingested: u64,            // cumulative results actually new
    pub new_results_history: Vec<u32>,  // last N runs: how many new results each time
    pub current_interval: Duration,     // adaptive: current schedule interval
    pub next_scheduled: Option<i64>,    // epoch timestamp of next run
}
```

#### 9.5.2 Dedup Layer 1: Temporal Cursors

Many APIs support date-range or cursor-based pagination. The ledger stores the cursor after each run and uses it for the next fetch. Zero overlap possible.

```
Run 1: fetch("Russia sanctions", after=None)
  -> newest result: 2026-03-10T14:00:00
  -> ledger stores temporal_cursor = "2026-03-10T14:00:00"

Run 2: fetch("Russia sanctions", after="2026-03-10T14:00:00")
  -> only results newer than 14:00 returned
  -> zero overlap with Run 1
```

**Source-specific cursors:**
- **Web/News APIs:** `after=<datetime>` parameter
- **Twitter/X:** `since_id=<tweet_id>` (monotonic IDs)
- **RSS feeds:** `If-None-Match: <ETag>` or `If-Modified-Since: <date>` headers (server returns 304 if unchanged)
- **GDELT:** `startdatetime=<YYYYMMDDHHMMSS>` parameter

The source connector maps these into the generic `temporal_cursor` field. If the source declares `supports_temporal_cursor = false`, this layer is skipped.

#### 9.5.3 Dedup Layer 2: Content Hash

For sources without temporal cursors, or to catch cross-query duplicates (different queries returning the same article), every result is hashed before processing.

```
Hash = FNV-1a(url) or FNV-1a(url + title) for sources without stable URLs

Before processing a result:
  if result_hash in ledger.result_hashes:
      skip (already seen, possibly from a different query)
  else:
      process and add hash to ledger
```

This catches:
- Same article returned by "Russia sanctions" and "EU Russia gas embargo"
- API returning stale results despite date parameters
- Duplicate webhook deliveries

#### 9.5.4 Dedup Layer 3: Query Subsumption

Broad queries subsume narrow ones. If "Russia sanctions" ran 5 minutes ago, "Russia sanctions EU gas" is likely already covered.

```
When scheduling query Q:
  for each recent query R in ledger (same source, within subsumption_window):
      if Q is a substring of R or R is a substring of Q:
          if R ran within subsumption_window:
              skip Q (subsumed by R)
```

This is a simple substring check, not semantic similarity. Keeps it fast and predictable. The subsumption window is configurable (default: equal to the source's schedule interval).

#### 9.5.5 Dedup Layer 4: Adaptive Frequency

The scheduler adjusts fetch intervals based on result yield:

```
After each run, record new_results_count in history.

If last 3 runs returned 0 new results:
    current_interval = min(current_interval * 2, max_interval)

If last run returned > threshold new results:
    current_interval = max(current_interval / 2, min_interval)

Otherwise:
    current_interval unchanged
```

**Bounds are mandatory:**
- `min_interval`: never faster than this (default: 5 minutes)
- `max_interval`: never slower than this (default: 24 hours)

This auto-tunes without user intervention. A breaking news query speeds up. A stale topic slows down.

#### 9.5.6 Scheduler Flow

```
Before every scheduled fetch for source S, query Q:

  1. Check ledger for Q + S
  2. If temporal cursor available AND source supports it:
       -> set fetch params to start from cursor
  3. Check query subsumption:
       -> if subsumed by recent broader query, skip
  4. If source has usage endpoint:
       -> call source.usage()
       -> check against configured thresholds
       -> above soft_limit_pct: emit ActionEvent (warn user)
       -> above hard_limit_pct: skip fetch, emit ActionEvent
  5. Execute fetch
  6. For each result:
       -> hash content
       -> if hash in ledger.result_hashes: skip
       -> else: send to ingest pipeline, add hash to ledger
  7. Update ledger record:
       -> temporal_cursor, run_count, result_hashes
       -> new_results_history (for adaptive frequency)
  8. Recalculate next_scheduled using adaptive frequency
```

#### 9.5.7 Ledger Storage

The ledger is stored as a sidecar file `.brain.ledger` alongside the main brain file. Format: append-only log of ledger records, compacted on startup. Small footprint -- even 10,000 queries with 1,000 result hashes each is <50MB.

#### 9.5.8 Two Search Modes

| Mode | Trigger | Dedup | Use Case |
|------|---------|-------|----------|
| **Scheduled** | Timer (cron-like) | Full ledger (all 4 layers) | Continuous monitoring, recurring queries |
| **On-demand** | User query or black area detection | Content hash only (skip subsumption) | New investigation, gap filling |

Scheduled searches use all dedup layers aggressively. On-demand searches skip query subsumption (the user explicitly asked for this specific query) but still use content hash dedup (no point re-ingesting known results).

**On-demand queries also update the ledger.** When enrichment runs for a user query, the query + source + timestamp + result hashes are recorded. If a second user (or the same user) queries the same thing within the cooldown window (section 9.7), the ledger short-circuits: cached results are returned, no external API calls. This prevents duplicate spend on the same query from multiple users or repeated MCP/A2A calls.

### 9.6 Source Authentication and Paid APIs

Users will connect paid/authenticated sources (Brave Search, OpenAI, commercial news APIs, proprietary databases). Engram must handle authentication securely and flexibly.

#### Authentication Methods

| Method | Config | Use Case |
|--------|--------|----------|
| **API key (header)** | `auth.type = "api_key"`, `auth.header = "X-API-Key"` | Brave, SerpAPI, most REST APIs |
| **Bearer token** | `auth.type = "bearer"`, `auth.token_env = "..."` | OAuth2 APIs, cloud services |
| **Basic auth** | `auth.type = "basic"`, `auth.user_env`, `auth.pass_env` | Legacy APIs, internal services |
| **OAuth2 client credentials** | `auth.type = "oauth2"`, `auth.token_url`, `auth.client_id_env` | Google APIs, enterprise services |
| **Custom header** | `auth.type = "custom"`, `auth.headers = {...}` | APIs with non-standard auth |

**All secrets are stored in environment variables, never in config files.**

#### Source Configuration with Auth and Usage

```toml
# sources.toml

[[sources]]
name = "brave-search"
type = "web_search"
endpoint = "https://api.search.brave.com/res/v1/web/search"
max_results = 10
timeout_ms = 5000
schedule = "30m"
min_interval = "10m"
max_interval = "6h"
adaptive_frequency = true

[sources.auth]
type = "api_key"
header = "X-Subscription-Token"
key_env = "BRAVE_API_KEY"

[sources.usage]
endpoint = "https://api.search.brave.com/res/v1/usage"
auth_env = "BRAVE_API_KEY"
check_before_fetch = true
soft_limit_pct = 80
hard_limit_pct = 95

[[sources]]
name = "twitter-osint"
type = "social"
schedule = "15m"
min_interval = "5m"
max_interval = "24h"
adaptive_frequency = true

[sources.auth]
type = "oauth2"
token_env = "TWITTER_BEARER_TOKEN"

[sources.usage]
endpoint = "https://api.twitter.com/2/usage/tweets"
auth_env = "TWITTER_BEARER_TOKEN"
check_before_fetch = true
soft_limit_pct = 80
hard_limit_pct = 95

[[sources]]
name = "recorded-future"
type = "paid_intel"
endpoint = "https://api.recordedfuture.com/v2/..."
schedule = "6h"
min_interval = "1h"
max_interval = "24h"

[sources.auth]
type = "bearer"
token_env = "RF_API_KEY"

[sources.usage]
endpoint = "https://api.recordedfuture.com/v2/usage"
auth_env = "RF_API_KEY"
check_before_fetch = true
soft_limit_pct = 70
hard_limit_pct = 90

[[sources]]
name = "gdelt-news"
type = "news_api"
endpoint = "https://api.gdeltproject.org/api/v2/doc/doc"
schedule = "30m"
min_interval = "5m"
max_interval = "12h"
adaptive_frequency = true
# No auth block -- GDELT is free
# No usage block -- no usage endpoint, no budget enforcement

[[sources]]
name = "custom-rss-feeds"
type = "rss"
urls = ["https://feed1.xml", "https://feed2.xml"]
schedule = "10m"
min_interval = "5m"
max_interval = "24h"
# No auth, no usage -- RSS is free and uses ETag natively

[[sources]]
name = "report-dropzone"
type = "file"
path = "/data/reports/"
watch = true                       # filesystem events via notify crate
poll_fallback = "30s"              # fallback for network drives
formats = ["txt", "md", "pdf", "json", "csv", "html"]
# No hardcoded trust -- internal docs earn trust through corroboration like any other source
# No schedule -- event-driven (watch mode)
# No auth, no usage -- local filesystem
```

**Key principle:** If a source has no `[sources.*.usage]` block, there is no budget enforcement. The user accepted this when configuring the source. Engram never guesses costs.

#### Source Health Monitoring

Each source tracks:
- Success/failure rate (last 100 requests)
- Average latency
- Last successful call
- Authentication status (valid/expired/error)
- Usage report (if usage endpoint configured)

#### API Endpoints

```
GET  /sources                    List all sources with health + usage status
GET  /sources/{name}             Single source details
GET  /sources/{name}/usage       Query source's usage endpoint (live, not cached)
GET  /sources/{name}/test        Test connectivity and auth (dry run)
GET  /sources/{name}/ledger      Ledger records for this source (queries, hashes, intervals)
GET  /sources/usage              Aggregated usage across all sources with usage endpoints
POST /sources/{name}/trigger     Manually trigger a source fetch (on-demand mode)
```

### 9.7 Enrichment Cooldown

To prevent excessive external calls:
- Same query (or semantically similar) within cooldown window: return cached enrichment results
- Default cooldown: 30 minutes per unique query
- Configurable per source

---

## 10. Streaming Architecture

Three distinct streaming concerns exist in v1.1.0, all built on the same foundation: tokio async channels and Server-Sent Events (SSE).

### 10.1 Ingest Streaming (External World -> Engram)

Beyond batch imports, engram needs to consume continuous data streams. Sources don't always deliver data in neat batches -- webhooks arrive in bursts, feeds update continuously, file systems change in real time.

#### Supported Ingest Modes

| Mode | Protocol | Use Case |
|------|----------|----------|
| **Batch** | `POST /batch` or `POST /ingest` | One-time imports, scheduled jobs |
| **NDJSON stream** | `POST /batch/stream` | Large file uploads, pipe from CLI |
| **Webhook receiver** | `POST /ingest/webhook/{pipeline_id}` | GitHub, Slack, GDELT push notifications |
| **SSE consumer** | Outbound connection to SSE source | News feeds, real-time APIs |
| **File watcher** | inotify/ReadDirectoryChanges | Local document directories |
| **WebSocket** | `WS /ingest/ws/{pipeline_id}` | Real-time bidirectional (status + data) |

#### Webhook Receiver

External services push data to engram via configured webhook endpoints. Each webhook is bound to a pipeline that processes the payload:

```
POST /ingest/webhook/gdelt-feed
Content-Type: application/json

{ "articles": [...] }

-> Pipeline "gdelt-feed" activates
-> Parse -> NER -> Resolve -> Store
-> 202 Accepted (processing is async)
```

Configuration:

```toml
[[webhooks]]
id = "gdelt-feed"
pipeline = "news-ingest"
secret_env = "GDELT_WEBHOOK_SECRET"   # HMAC validation
max_payload_bytes = 10_485_760         # 10MB
rate_limit = "60/min"
```

#### WebSocket Ingest

For bidirectional streaming where the client needs real-time feedback:

```
Client                          Engram
  |                                |
  |-- WS connect ----------------->|
  |-- {"type":"data", ...} ------->|  (fact ingested)
  |<- {"type":"ack", id:1} --------|  (confirmed)
  |-- {"type":"data", ...} ------->|
  |<- {"type":"conflict", ...} ----|  (conflict detected)
  |-- {"type":"data", ...} ------->|
  |<- {"type":"ack", id:3} --------|
  |-- {"type":"flush"} ----------->|
  |<- {"type":"stats", ...} -------|  (batch stats)
```

### 10.2 Response Streaming (Engram -> Client)

When a query triggers enrichment in `await` mode, results should stream back as they arrive rather than waiting for all sources to complete.

#### SSE Response Stream

```
GET /query?q=titanium+sanctions&enrich=await
Accept: text/event-stream

event: local
data: {"results": [...], "count": 2, "avg_confidence": 0.3}

event: enriching
data: {"sources": ["brave-search", "semantic-scholar", "mesh-peers"], "started": true}

event: enriched
data: {"source": "brave-search", "new_facts": 5, "elapsed_ms": 1200}

event: enriched
data: {"source": "mesh-peers", "new_facts": 2, "elapsed_ms": 800}

event: enriched
data: {"source": "semantic-scholar", "new_facts": 3, "elapsed_ms": 3400}

event: complete
data: {"total_results": 12, "new_facts_stored": 10, "conflicts": 1}
```

The client renders local results immediately, then progressively updates as enrichment results arrive. This is critical for UX -- the user sees something instantly, and the display gets richer over seconds.

#### Ingest Progress Stream

For large batch ingests, real-time progress via SSE:

```
GET /batch/jobs/{id}/stream
Accept: text/event-stream

event: progress
data: {"processed": 1000, "total": 50000, "entities": 342, "errors": 2}

event: progress
data: {"processed": 2000, "total": 50000, "entities": 687, "errors": 3}

event: conflict
data: {"entity": "Mercury", "type": "ambiguous", "candidates": 3}

event: complete
data: {"processed": 50000, "entities": 17234, "relations": 8901, "errors": 47}
```

### 10.3 Event Streaming (Engram Events -> Subscribers)

External systems (dashboards, LLMs, other agents, monitoring) should be able to subscribe to engram's event bus and receive real-time notifications of graph changes.

#### SSE Event Subscription

```
GET /events/stream?topics=fact_stored,conflict_detected&min_confidence=0.5
Accept: text/event-stream

event: fact_stored
data: {"node_id": 42, "label": "TSMC", "confidence": 0.85, "source": "news-ingest"}

event: conflict_detected
data: {"existing": 42, "incoming": 99, "type": "value_difference"}

event: black_area
data: {"kind": "temporal_gap", "entity": "EU-sanctions", "severity": 0.78}

event: action_fired
data: {"rule": "sanctions-alert", "trigger": "fact_stored", "effects": ["webhook"]}
```

Subscribers filter by event type and optional criteria. This enables:
- **Live dashboards** that update without polling
- **LLM agents** that react to graph changes in real time
- **Monitoring systems** that track ingestion health
- **Mesh peers** that receive change notifications (lighter than full sync)

#### Subscription Configuration

```
GET /events/stream?topics=*                           # all events
GET /events/stream?topics=fact_stored&entity_type=ORG  # only new organizations
GET /events/stream?topics=conflict_detected            # only conflicts
GET /events/stream?topics=action_fired&rule=sanctions*  # specific rule firings
```

### 10.4 Implementation: Shared Infrastructure

All three streaming modes use the same tokio infrastructure:

```rust
/// Internal broadcast channel for graph events.
/// All subscribers receive all events, filter client-side.
pub struct EventBus {
    sender: broadcast::Sender<GraphEvent>,
    capacity: usize,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self { ... }
    pub fn publish(&self, event: GraphEvent) { ... }
    pub fn subscribe(&self) -> broadcast::Receiver<GraphEvent> { ... }
}
```

SSE endpoints use axum's `Sse` extractor with `tokio_stream`:

```rust
async fn event_stream(
    State(state): State<AppState>,
    Query(filter): Query<EventFilter>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_bus.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter(move |event| filter.matches(event))
        .map(|event| Ok(Event::default()
            .event(event.event_type())
            .data(serde_json::to_string(&event).unwrap())
        ));
    Sse::new(stream)
}
```

### 10.5 API Endpoints Summary (Streaming)

```
POST /batch/stream                    NDJSON streaming ingestion
GET  /batch/jobs/{id}/stream          SSE: ingest progress
POST /ingest/webhook/{pipeline_id}    Webhook receiver
WS   /ingest/ws/{pipeline_id}        WebSocket bidirectional ingest
GET  /query?enrich=await              SSE: progressive enrichment results
GET  /events/stream                   SSE: graph event subscription
```

### 10.6 gRPC Service Definitions for v1.1.0

v1.0.0 provides 13 gRPC RPCs via `proto/engram.proto` (feature-gated `--features grpc`). v1.1.0 adds new endpoints that need gRPC equivalents, especially streaming operations which map naturally to gRPC Server-Streaming RPCs.

**New proto definitions (`proto/engram_v110.proto`):**

```protobuf
syntax = "proto3";
package engram.v110;

service IngestService {
  // Unary: submit text for pipeline processing
  rpc Ingest (IngestRequest) returns (IngestResponse);
  // Client-streaming: bulk ingest (replaces NDJSON)
  rpc IngestStream (stream IngestItem) returns (IngestSummary);
  // Unary: get ingest job status
  rpc GetIngestJob (JobId) returns (IngestJobStatus);
  // Server-streaming: ingest progress events
  rpc WatchIngestJob (JobId) returns (stream IngestEvent);
}

service SourceService {
  // Unary: list all sources with health
  rpc ListSources (Empty) returns (SourceList);
  // Unary: query a source's usage endpoint
  rpc GetSourceUsage (SourceId) returns (UsageReport);
  // Unary: trigger a source fetch
  rpc TriggerSource (SourceId) returns (TriggerResponse);
  // Unary: get ledger records for a source
  rpc GetSourceLedger (SourceId) returns (LedgerRecords);
}

service ReasonService {
  // Unary: list black areas
  rpc ListGaps (GapFilter) returns (GapList);
  // Unary: get coverage for a topic
  rpc GetCoverage (TopicQuery) returns (CoverageReport);
  // Unary: trigger gap scan
  rpc ScanGaps (Empty) returns (GapList);
}

service EnrichmentService {
  // Server-streaming: query with progressive enrichment results
  rpc EnrichQuery (EnrichRequest) returns (stream EnrichEvent);
}

service MeshService {
  // Unary: list peer knowledge profiles
  rpc ListProfiles (Empty) returns (ProfileList);
  // Unary: discover peers by topic
  rpc Discover (DiscoverRequest) returns (ProfileList);
  // Unary: federated query across peers
  rpc FederatedQuery (FederatedQueryRequest) returns (FederatedQueryResponse);
}

service EventService {
  // Server-streaming: subscribe to graph events (replaces SSE)
  rpc Subscribe (EventFilter) returns (stream GraphEvent);
}
```

**Streaming mapping:**

| HTTP Endpoint | gRPC RPC | Streaming Type |
|---------------|----------|---------------|
| `POST /batch/stream` (NDJSON) | `IngestStream` | Client-streaming |
| `GET /batch/jobs/{id}/stream` (SSE) | `WatchIngestJob` | Server-streaming |
| `GET /query?enrich=await` (SSE) | `EnrichQuery` | Server-streaming |
| `GET /events/stream` (SSE) | `Subscribe` | Server-streaming |

All gRPC services are feature-gated behind `--features grpc` (same as v1.0.0). Proto files live in `proto/` directory. Code generation via `tonic-build` in `build.rs`.

### 10.7 MCP Tool Additions for v1.1.0

v1.0.0 provides MCP tools for basic graph operations (`engram_ask`, `engram_tell`, `engram_query`, `engram_prove`, `engram_explain`, `engram_search`). v1.1.0 adds new capabilities that LLMs should be able to access via MCP.

**New MCP tools:**

| Tool | Type | Description | LLM-Safe? |
|------|------|-------------|-----------|
| `engram_gaps` | Read | List black areas with severity, type, and suggested queries | Yes |
| `engram_enrich` | Read+trigger | Trigger enrichment for a query, return merged results | Yes (cooldown enforced) |
| `engram_sources` | Read | List configured sources with health and usage status | Yes |
| `engram_source_usage` | Read | Query a specific source's usage endpoint | Yes |
| `engram_mesh_discover` | Read | Find mesh peers covering a topic | Yes |
| `engram_mesh_query` | Read | Federated query across mesh peers | Yes (ACL enforced) |
| `engram_ingest_status` | Read | List running/completed ingest jobs | Yes |
| `engram_suggest_queries` | Read | Get LLM-suggested investigation queries for a gap | Yes (suggestions only) |

**Restricted tools (require explicit opt-in in config):**

| Tool | Type | Description | Default |
|------|------|-------------|---------|
| `engram_ingest` | Mutating | Submit raw text for ingest pipeline processing | Disabled |
| `engram_create_rule` | Mutating | Create a new action rule | Disabled |
| `engram_trigger_source` | Mutating | Manually trigger a source fetch | Disabled |

```toml
# engram.toml
[mcp.v110_tools]
gaps = true                     # engram_gaps
enrich = true                   # engram_enrich
sources = true                  # engram_sources, engram_source_usage
mesh = true                     # engram_mesh_discover, engram_mesh_query
ingest_status = true            # engram_ingest_status
suggest_queries = true          # engram_suggest_queries

# Mutating tools -- disabled by default, human opt-in
allow_ingest = false            # engram_ingest
allow_create_rule = false       # engram_create_rule
allow_trigger_source = false    # engram_trigger_source
```

**Trust boundaries:**
- Read tools: always safe. LLM can explore gaps, sources, mesh -- no side effects.
- `engram_enrich`: triggers external API calls but respects cooldown, usage limits, and budget. Safe with guardrails.
- Mutating tools: disabled by default. An LLM creating ingest jobs or action rules without human review could cause runaway API costs or unintended graph mutations. Admin explicitly enables per deployment.

**Example MCP interaction:**

```json
// LLM calls engram_gaps
{"method": "engram_gaps", "params": {"min_severity": 0.5}}

// Response
{"gaps": [
  {"type": "asymmetric_cluster", "domain": "Russia/titanium", "severity": 0.92,
   "suggested_queries": ["Russia titanium supply chain", "titanium sanctions impact"]},
  {"type": "temporal_gap", "domain": "EU sanctions", "severity": 0.78,
   "last_update": "2026-02-15"}
]}

// LLM calls engram_enrich to fill the gap
{"method": "engram_enrich", "params": {
  "query": "Russia titanium supply chain sanctions",
  "mode": "eager"
}}

// Response
{"status": "enrichment_started", "tiers_queried": ["mesh", "gdelt"],
 "local_results": 2, "estimated_enrichment_time_ms": 3000}
```

### 10.8 A2A Skills for v1.1.0

v1.0.0 A2A skills (`store`, `query`, `relate`, `learn`, `prove`, `explain`, `chain`) cover basic graph operations. v1.1.0 adds skills for multi-agent workflows involving ingest, enrichment, and reasoning.

**New A2A skills:**

| Skill | Input | Output | Streaming? |
|-------|-------|--------|------------|
| `ingest_text` | Raw text + source metadata | Extracted entities + relations stored | No (batch result) |
| `enrich_query` | Query string + enrichment mode | Local + enriched results | Yes (SSE progress) |
| `analyze_gaps` | Optional topic filter | List of black areas with severity | No |
| `federated_search` | Query + sensitivity clearance | Merged results from mesh peers | No |
| `suggest_investigations` | Gap ID or topic | LLM-generated query suggestions | No |

**Multi-agent coordination example:**

```
Agent A (monitoring-agent):
  -> Detects breaking news event via RSS
  -> Calls engram A2A skill: ingest_text(article_text, source="reuters")

Agent B (analysis-agent):
  -> Subscribed to FactStored events via SSE
  -> Sees new fact about Iran conflict
  -> Calls engram A2A skill: analyze_gaps(topic="Iran")
  -> Gets black area: "Iran-Russia arms supply" severity 0.85
  -> Calls engram A2A skill: enrich_query("Iran Russia arms supply", mode="await")
  -> Receives progressive results via streaming task

Agent C (reporting-agent):
  -> Queries engram for updated Iran-Russia picture
  -> Generates briefing document
```

**Streaming tasks via A2A:**

For long-running operations (enrichment, large ingest), A2A tasks support streaming results. The task status transitions:

```
SUBMITTED -> WORKING -> STREAMING (partial results available) -> COMPLETED
```

Agents poll task status or subscribe to updates. This aligns with the A2A protocol's existing task lifecycle from v1.0.0.

---

## 11. Frontend (Leptos WASM)

### 11.1 Design Philosophy

The backend is complex. The frontend makes it simple. Users should be able to configure NER pipelines, define ingest rules, monitor black areas, and manage actions -- all through a visual interface.

**Full WASM rewrite.** The v1.0.0 frontend (vanilla JS + vis.js) is replaced entirely with a Leptos-based WASM application. No readable JavaScript ships to the browser -- only a compiled `.wasm` binary and auto-generated JS glue code. This protects IP and unifies the technology stack (Rust everywhere).

**Why Leptos:**
- Fine-grained reactivity (signals, not virtual DOM diffing) -- minimal overhead
- CSR mode compiles to WASM via `trunk` -- no server-side rendering needed
- Type-safe routing, components, and API calls -- compile-time guarantees
- Single language (Rust) for frontend and backend -- shared types possible
- Active ecosystem, MIT licensed, production-ready

**What the browser sees (before vs after):**

| | v1.0.0 | v1.1.0 |
|---|---|---|
| Framework | None (vanilla JS) | Leptos (compiled WASM) |
| Routing | Hash-based (`#/graph`) | `leptos_router` (hash mode) |
| Reactivity | Manual DOM manipulation | Leptos signals (fine-grained) |
| API calls | `fetch()` in JS | `gloo-net` in Rust |
| Graph viz | vis.js (JS) | vis.js via `wasm-bindgen` interop |
| Icons | Font Awesome CDN | Font Awesome CDN (unchanged) |
| Shipped JS | ~2000 LOC across 7 files | 0 LOC (WASM + auto-generated glue only) |
| IP exposure | Full source readable | Binary only |

**Only external JS dependency:** vis.js for graph visualization. All other logic is compiled Rust.

### 11.2 Frontend Crate: `engram-ui`

Full Leptos application compiled to WASM via `trunk`. Served by engram's HTTP server.

```
engram-ui/
  src/
    main.rs                 Leptos app mount, router setup
    app.rs                  Root <App/> component, nav layout
    api/
      mod.rs                API client (gloo-net), base URL config
      types.rs              Request/response types (shared with backend where possible)
    components/
      mod.rs
      nav.rs                Navigation bar (11 items, Font Awesome icons)
      toast.rs              Toast notification system
      modal.rs              Reusable modal component
      settings.rs           API settings modal
      table.rs              Sortable data table
      stat_card.rs          Dashboard stat card
      graph_canvas.rs       vis.js interop wrapper (wasm-bindgen extern)
      sse_listener.rs       SSE event stream -> Leptos signals
      warning_banner.rs     Permanent warning banner (for LLM suggestions)
    pages/
      mod.rs
      dashboard.rs          System overview, stats, health
      graph.rs              Knowledge graph visualization (vis.js)
      search.rs             BM25 + semantic search interface
      nl.rs                 Natural language /tell and /ask
      import.rs             JSON-LD import/export
      learning.rs           Confidence scores, reinforcement, decay
      ingest.rs             Pipeline configuration, monitoring, live progress
      sources.rs            Source management, health, usage, ledger view
      actions.rs            Action rules dashboard, rule editor, dry run
      gaps.rs               Black area map, severity table, suggested investigations
      mesh.rs               Mesh topology, peer profiles, federated query, trust controls
  index.html                Trunk entry point (minimal: Font Awesome CDN, vis.js script)
  Cargo.toml
  Trunk.toml                Build configuration
```

### 11.3 Navigation Structure

```
engram (v1.1.0)
  <i class="fa fa-gauge"></i> Dashboard          /              System overview
  <i class="fa fa-diagram-project"></i> Graph              /graph          Knowledge graph viz
  <i class="fa fa-magnifying-glass"></i> Search             /search         BM25 + semantic
  <i class="fa fa-comments"></i> Natural Language   /nl             /tell and /ask
  <i class="fa fa-file-import"></i> Import             /import         JSON-LD import/export
  <i class="fa fa-graduation-cap"></i> Learning           /learning       Confidence lifecycle
  ---- v1.1.0 additions ----
  <i class="fa fa-gears"></i> Ingest             /ingest         Pipeline config & monitoring
  <i class="fa fa-plug"></i> Sources            /sources        Source health & budget
  <i class="fa fa-bolt"></i> Actions            /actions        Event-driven rules
  <i class="fa fa-map"></i> Gaps               /gaps           Black area detection
  <i class="fa fa-network-wired"></i> Mesh               /mesh           Peer federation & trust
```

### 11.4 Key Technical Patterns

#### vis.js Interop

The graph visualization page uses vis.js through `wasm-bindgen` extern bindings. vis.js is loaded as a `<script>` tag in `index.html` and accessed from Rust:

```rust
// components/graph_canvas.rs

#[wasm_bindgen]
extern "C" {
    type VisNetwork;
    type VisDataSet;

    #[wasm_bindgen(js_namespace = vis, js_name = DataSet)]
    fn new_dataset() -> VisDataSet;

    #[wasm_bindgen(js_namespace = vis, js_name = Network)]
    fn new_network(container: &web_sys::HtmlElement, data: &JsValue, options: &JsValue) -> VisNetwork;

    #[wasm_bindgen(method)]
    fn setData(this: &VisNetwork, data: &JsValue);

    #[wasm_bindgen(method)]
    fn on(this: &VisNetwork, event: &str, callback: &Closure<dyn FnMut(JsValue)>);

    #[wasm_bindgen(method)]
    fn destroy(this: &VisNetwork);
}

#[component]
fn GraphCanvas(nodes: Signal<Vec<GraphNode>>, edges: Signal<Vec<GraphEdge>>) -> impl IntoView {
    let container_ref = NodeRef::<html::Div>::new();
    let network: StoredValue<Option<VisNetwork>> = StoredValue::new(None);

    // Initialize on mount
    Effect::new(move || {
        if let Some(el) = container_ref.get() {
            let net = new_network(&el, &build_data(&nodes.get(), &edges.get()), &options());
            network.set_value(Some(net));
        }
    });

    // React to data changes
    Effect::new(move || {
        if let Some(ref net) = network.get_value() {
            net.setData(&build_data(&nodes.get(), &edges.get()));
        }
    });

    // Cleanup on unmount
    on_cleanup(move || {
        if let Some(ref net) = network.get_value() {
            net.destroy();
        }
    });

    view! { <div node_ref=container_ref class="graph-container"></div> }
}
```

#### SSE Integration with Leptos Signals

Server-Sent Events drive real-time updates through Leptos signals:

```rust
// components/sse_listener.rs

use gloo_net::eventsource::EventSource;

#[component]
fn SseListener(
    #[prop(into)] endpoint: String,
    on_event: WriteSignal<Option<GraphEvent>>,
) -> impl IntoView {
    let es = StoredValue::new(None::<EventSource>);

    Effect::new(move || {
        let source = EventSource::new(&endpoint).expect("SSE connect");
        let set_event = on_event;
        source.add_event_listener("message", move |msg| {
            if let Ok(evt) = serde_json::from_str::<GraphEvent>(&msg.data()) {
                set_event.set(Some(evt));
            }
        });
        es.set_value(Some(source));
    });

    on_cleanup(move || {
        if let Some(source) = es.get_value() {
            source.close();
        }
    });

    view! {} // invisible component, only side effects
}
```

Usage in pages:

```rust
// pages/ingest.rs

#[component]
fn IngestPage() -> impl IntoView {
    let (event, set_event) = signal(None::<GraphEvent>);
    let pipeline_stats = Memo::new(move || {
        // Recompute stats when new event arrives
        event.get().map(|e| update_stats(e))
    });

    view! {
        <SseListener endpoint="/events/stream?filter=ingest" on_event=set_event />
        <div class="ingest-dashboard">
            <StatCard label="Facts Ingested" value=move || pipeline_stats.get() />
            // ...
        </div>
    }
}
```

#### API Client

```rust
// api/mod.rs

use gloo_net::http::Request;

pub struct ApiClient {
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self { base_url: base_url.to_string() }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let resp = Request::get(&format!("{}{}", self.base_url, path))
            .send().await?;
        Ok(resp.json().await?)
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T, ApiError> {
        let resp = Request::post(&format!("{}{}", self.base_url, path))
            .json(body)?
            .send().await?;
        Ok(resp.json().await?)
    }
}
```

The API client instance is provided via Leptos context (`provide_context` / `use_context`) so all pages share one instance with consistent base URL.

### 11.5 UI Sections

#### Ingest Pipeline Manager

```
+--------------------------------------------------------------+
|  INGEST PIPELINES                                    [+ New] |
+--------------------------------------------------------------+
| Pipeline        | Status   | Last Run  | Facts In | Errors  |
|-----------------|----------|-----------|----------|---------|
| news-feed       | Active   | 2m ago    | 1,247    | 3       |
| internal-docs   | Paused   | 1h ago    | 89       | 0       |
| web-enrichment  | On-query | --        | 342      | 12      |
+--------------------------------------------------------------+

Pipeline Editor:
+--------------------------------------------------------------+
| Name: [news-feed                    ]                         |
| Schedule: [Every 30m  v]  Source: [GDELT API     v]          |
|                                                               |
| Stages:                                                       |
| [x] Parse         Format: [JSON  v]                          |
| [x] Language Det.  Min confidence: [0.8    ]                 |
| [x] NER            Chain: [Cascade v]                        |
|     Backends: [Gazetteer] [Rules: finance.toml] [SpaCy: en]  |
| [x] Entity Resolve  Strategy: [Fuzzy + Embedding v]          |
| [x] Dedup          Threshold: [0.95   ]                      |
| [x] Conflict Check                                           |
| [x] Confidence     Learned trust: [auto  ]                   |
|                                                               |
| [Test with sample] [Save] [Run Now]                          |
+--------------------------------------------------------------+
```

#### NER Configuration

```
+--------------------------------------------------------------+
|  NER CONFIGURATION                                           |
+--------------------------------------------------------------+
| Graph Gazetteer: [ON ]  Refresh: [5m  ]  Min conf: [0.6  ]  |
|                                                               |
| Active Rules:                                                 |
| +----------------------------------------------------------+ |
| | File              | Rules | Language | Last match         | |
| |-------------------|-------|----------|--------------------| |
| | common.toml       | 24    | all      | 30s ago            | |
| | finance.toml      | 18    | en       | 2m ago             | |
| | geopolitics.toml  | 31    | en,de,ru | 5m ago             | |
| +----------------------------------------------------------+ |
|                                                               |
| Learned Patterns: 47 active / 12 pending (below threshold)  |
|                                                               |
| [Test NER]  Input: [paste text here...                     ] |
|             Lang:  [auto-detect v]                           |
|             Result: PERSON: "Tim Cook" (0.95, gazetteer)     |
|                     ORG: "Apple Inc." (0.92, gazetteer)      |
|                     MONETARY: "$3.2 billion" (0.90, rule)    |
+--------------------------------------------------------------+
```

#### Action Rules Dashboard

```
+--------------------------------------------------------------+
|  ACTION RULES                                        [+ New] |
+--------------------------------------------------------------+
| Rule              | Trigger      | Fires | Last    | Status |
|-------------------|-------------|-------|---------|--------|
| sanctions-alert   | fact_stored | 23    | 10m ago | Active |
| stale-fact-decay  | timer (6h)  | 1,204 | 2h ago  | Active |
| query-gap-enrich  | query_gap   | 89    | 5m ago  | Active |
| hub-detection     | topology    | 7     | 1h ago  | Paused |
+--------------------------------------------------------------+
```

#### Black Area Map

```
+--------------------------------------------------------------+
|  KNOWLEDGE GAPS                              [Scan Now]      |
+--------------------------------------------------------------+
| Severity | Type              | Domain        | Suggested     |
|----------|-------------------|---------------|---------------|
| 0.92     | Asymmetric cluster| Russia/supply | 3 queries     |
| 0.78     | Temporal gap      | EU sanctions  | 2 queries     |
| 0.65     | Frontier node     | TSMC          | 4 queries     |
| 0.51     | Confidence desert | China/trade   | 5 queries     |
+--------------------------------------------------------------+
|                                                               |
| [Graph visualization showing gaps highlighted in red]         |
|                                                               |
+--------------------------------------------------------------+
```

#### LLM-Suggested Investigations

```
+--------------------------------------------------------------+
|  <i class="fa fa-lightbulb"></i> SUGGESTED INVESTIGATIONS                       |
+--------------------------------------------------------------+
|  <i class="fa fa-exclamation-triangle"></i> AI-generated suggestions. NOT facts.              |
|  Review before running. May contain hallucinations.           |
|  This warning cannot be dismissed.                            |
+--------------------------------------------------------------+
|                                                               |
|  Based on: Knowledge gap "Russia/supply chain"                |
|            (severity 0.92, asymmetric cluster)                |
|                                                               |
|  1. "Iran titanium exports to Russia 2025-2026"              |
|     Reason: 40 facts on sanctions, 2 on titanium supply      |
|     [Run] [Edit query] [Dismiss]                              |
|                                                               |
|  2. "Russian military equipment components Iran"              |
|     Reason: arms_dealer edge exists, no specifics on types    |
|     [Run] [Edit query] [Dismiss]                              |
|                                                               |
|  3. "Iran drone program Russian technology transfer"          |
|     Reason: frontier node, mentioned once, no investigation   |
|     [Run] [Edit query] [Dismiss]                              |
|                                                               |
|  [Generate more] [Configure LLM]                              |
+--------------------------------------------------------------+
```

**Rules:**
- **Never auto-execute.** User clicks [Run] explicitly.
- **Always show reasoning.** Why the LLM suggested this query.
- **Editable.** User can modify the query before running.
- **Warning is permanent.** Always visible, not dismissible.
- **LLM generates search strings, not facts.** Results go through normal ingest pipeline with real source provenance. Worst case: a bad query returns irrelevant results, which the pipeline filters out.
- **Clear differentiator.** No competing system surfaces its own knowledge gaps and suggests how to fill them.

### 11.6 Crate Configuration

```toml
# engram-ui/Cargo.toml

[package]
name = "engram-ui"
version = "1.1.0"
edition = "2024"

[dependencies]
leptos = { version = "0.7", features = ["csr"] }
leptos_router = { version = "0.7", features = ["csr"] }
leptos_meta = { version = "0.7", features = ["csr"] }
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["HtmlElement", "EventSource", "MessageEvent"] }
js-sys = "0.3"
gloo-net = { version = "0.6", features = ["http", "eventsource"] }
gloo-timers = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
console_error_panic_hook = "0.1"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
```

```toml
# engram-ui/Trunk.toml

[build]
target = "index.html"
dist = "../frontend/dist"

[watch]
watch = ["src", "index.html"]
```

```html
<!-- engram-ui/index.html (Trunk entry point) -->
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>engram - Knowledge Graph</title>
  <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css">
  <link data-trunk rel="css" href="css/style.css">
  <script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
</head>
<body>
  <!-- Leptos mounts here -->
</body>
</html>
```

**Build:** `cd engram-ui && trunk build --release`

**Dev:** `trunk serve` (hot-reload, proxies API requests to engram backend)

The browser receives only:
- `engram-ui_bg.wasm` -- compiled application binary (~1-2 MB gzipped)
- `engram-ui.js` -- auto-generated glue (Trunk/wasm-bindgen output)
- `style.css` -- stylesheet
- Font Awesome + vis.js from CDN

Zero readable application logic. All routing, state management, API calls, and UI rendering happen inside the WASM binary.

---

## 12. Crate Structure

### 12.1 New Crates

```
engram/
  crates/
    engram-core/       existing: graph, storage, index, events (enhanced)
    engram-compute/    existing: GPU, NPU, SIMD
    engram-inference/  existing: rule engine
    engram-learning/   existing: reinforcement, decay, co-occurrence
    engram-mesh/       existing: peer sync, gossip, trust
    engram-a2a/        existing: agent-to-agent protocol
    engram-api/        existing: HTTP, MCP, gRPC (enhanced)
    engram-intel/      existing: WASM geopolitical engine
    engram-ingest/     NEW: ETL pipeline, NER, sources, resolution
    engram-action/     NEW: event bus, rule engine, effects
    engram-reason/     NEW: black area detection, gap analysis
    engram-ui/         NEW: Full Leptos WASM frontend (excluded from workspace, built via trunk)
```

### 12.2 Dependency Graph

```
engram-core          (knows nothing about new crates, emits events)
    ^
    |
    +--- engram-ingest   (reads graph for resolution, writes facts)
    |        ^
    |        |
    +--- engram-action   (reads graph for conditions, subscribes to events)
    |        |                can trigger engram-ingest pipelines
    |        |
    +--- engram-reason   (reads graph for analysis, emits BlackArea events)
    |
    +--- engram-api      (wires everything together, exposes HTTP endpoints)
             |
             +--- depends on: core, ingest, action, reason
```

Core never depends on new crates. New crates depend on core. API depends on everything.

### 12.3 Workspace Configuration

```toml
# Cargo.toml (workspace root)

[workspace]
members = [
    "crates/engram-core",
    "crates/engram-compute",
    "crates/engram-inference",
    "crates/engram-learning",
    "crates/engram-mesh",
    "crates/engram-a2a",
    "crates/engram-api",
    "crates/engram-ingest",
    "crates/engram-action",
    "crates/engram-reason",
]
exclude = [
    "crates/engram-intel",
    "crates/engram-ui",
]

[features]
default = []
ingest = ["engram-api/ingest", "engram-ingest"]
actions = ["engram-api/actions", "engram-action"]
reason = ["engram-api/reason", "engram-reason"]
full = ["ingest", "actions", "reason", "mesh", "llm", "onnx", "grpc"]
```

All v1.1.0 features are opt-in via feature flags. The base binary stays lean.

---

## 13. Build Order

### Phase 7: Foundation (event system + bulk endpoint)

| Step | Task | Effort | Dependencies |
|------|------|--------|-------------|
| 7.1 | Add `GraphEvent` enum and bounded channel to `engram-core` | Small | None |
| 7.2 | Emit events from all graph mutation methods (store, relate, update_confidence, etc.) | Small | 7.1 |
| 7.3 | Upgrade `POST /batch` with NDJSON streaming | Medium | None |
| 7.4 | Add chunked write locking to batch | Small | 7.3 |
| 7.5 | Add upsert mode to batch | Small | 7.4 |

### Phase 8: Ingest Pipeline

| Step | Task | Effort | Dependencies |
|------|------|--------|-------------|
| 8.1 | Create `engram-ingest` crate skeleton (traits, pipeline executor) | Medium | None |
| 8.2 | Implement pipeline stages: Parse, Transform, Load | Medium | 8.1 |
| 8.3 | Language detection integration (lingua-rs) | Small | 8.1 |
| 8.4 | Graph gazetteer (dynamic, self-updating) | Medium | 8.1 |
| 8.5 | Rule-based NER (regex patterns, per-language rule files) | Medium | 8.3 |
| 8.6 | NER chain (cascade/merge strategies) | Medium | 8.4, 8.5 |
| 8.7 | Anno backend (feature-gated, `anno_backend.rs`, GLiNER2 + coreference) | Medium | 8.6 |
| 8.8 | SpaCy HTTP sidecar integration | Small | 8.6 |
| 8.9 | LLM fallback NER (with restrictions) | Small | 8.6 |
| 8.10 | Entity resolution (conservative, progressive 4-step: filter/weight/schedule/match) | Large | 8.6 |
| 8.11 | Deduplication (content hash + semantic) | Small | 8.10 |
| 8.12 | Conflict detection and resolution | Medium | 8.11 |
| 8.13 | Confidence calculation (learned trust * extraction confidence, author > source > baseline) | Small | 8.12 |
| 8.14 | Multi-threaded pipeline executor (rayon + tokio) | Medium | 8.2 |
| 8.15 | Pipeline shortcuts (`?skip=ner,resolve` query params) | Small | 8.14 |
| 8.16 | Wire into API: `POST /ingest`, `POST /ingest/file`, `POST /ingest/configure` | Small | 8.14 |
| 8.17 | Source trait with capabilities + usage endpoint | Medium | 8.1 |
| 8.18 | File source (notify crate, watch mode, poll fallback, format auto-detect) | Medium | 8.17 |
| 8.19 | Search ledger (`.brain.ledger`, temporal cursors, content hash dedup) | Medium | 8.17 |
| 8.20 | Query subsumption (substring check, configurable window) | Small | 8.19 |
| 8.21 | Adaptive frequency scheduler (min/max bounds, yield-based adjustment) | Medium | 8.19 |
| 8.22 | Source usage endpoint integration (pre-fetch budget check, soft/hard limits) | Medium | 8.17 |
| 8.23 | Mesh fast path (skip NER, resolve locally, peer trust multiplier) | Small | 8.10, 8.15 |
| 8.24 | Learned patterns from graph co-occurrence | Medium | 8.6 |
| 8.25 | NER correction feedback loop | Small | 8.24 |
| 8.26 | Source health monitoring (success/failure rate, latency, auth status) | Small | 8.17 |
| 8.27 | Wire source APIs: `GET /sources`, `GET /sources/{name}/usage`, `GET /sources/{name}/ledger` | Small | 8.22, 8.19 |
| 8.28 | Learned trust: create Source/Author nodes on first encounter, `from_source`/`authored_by` edges | Medium | 8.13, 8.17 |
| 8.29 | Learned trust: auto-adjust via corroboration/correction propagation, per-source author scoping | Medium | 8.28 |

### Phase 9: Action Engine

| Step | Task | Effort | Dependencies |
|------|------|--------|-------------|
| 9.1 | Create `engram-action` crate skeleton | Small | 7.1 |
| 9.2 | Event subscriber (consumes GraphEvent channel) | Small | 9.1 |
| 9.3 | Rule parser (TOML rule definitions) | Medium | 9.2 |
| 9.4 | Condition evaluator (pattern matching against events) | Medium | 9.3 |
| 9.5 | Internal effects (confidence cascade, edge creation, tier change) | Medium | 9.4 |
| 9.6 | External effects (webhook, API call, message notification) | Medium | 9.4 |
| 9.7 | `CreateIngestJob` effect (dynamic jobs, `QueryTemplate`, `ReconcileStrategy`) | Medium | 9.4, 8.14 |
| 9.8 | Safety constraints (cooldown, chain depth, effect budget) | Small | 9.5, 9.6, 9.7 |
| 9.9 | Timer-based triggers (scheduled rules) | Small | 9.4 |
| 9.10 | Wire into API: rule management endpoints | Small | 9.8 |
| 9.11 | Dry run mode | Small | 9.4 |

### Phase 10: Black Area Detection & Enrichment

| Step | Task | Effort | Dependencies |
|------|------|--------|-------------|
| 10.1 | Create `engram-reason` crate skeleton | Small | None |
| 10.2 | Frontier node detection | Small | 10.1 |
| 10.3 | Structural hole detection | Medium | 10.1 |
| 10.4 | Asymmetric cluster analysis | Medium | 10.1 |
| 10.5 | Temporal gap detection | Small | 10.1 |
| 10.6 | Confidence desert detection | Small | 10.1 |
| 10.6b | Coordinated cluster detection (dense internal, sparse external, low author trust, temporal sync) | Medium | 10.4 |
| 10.7 | Severity scoring and ranking | Small | 10.2-10.6 |
| 10.8 | Suggested query generation (mechanical, from graph topology) | Medium | 10.7 |
| 10.9 | LLM-suggested queries (optional, via existing LLM endpoint) | Small | 10.8 |
| 10.10 | Mesh knowledge profile auto-derivation (cluster analysis -> DomainCoverage) | Medium | 10.4 |
| 10.11 | Mesh profile gossip broadcast + ProfileQuery message type | Medium | 10.10 |
| 10.12 | Mesh federated query protocol (FederatedQuery/FederatedResult) | Medium | 10.11 |
| 10.13 | Mesh discovery API (`/mesh/profiles`, `/mesh/discover`) | Small | 10.12 |
| 10.14 | 3-tier enrichment dispatcher (mesh > free external > paid external) | Medium | 10.12, 8.17 |
| 10.15 | Query-triggered enrichment (eager + await modes) | Medium | 10.14 |
| 10.16 | Mesh-level black area detection (uncovered areas across all peers) | Small | 10.10, 10.7 |
| 10.17 | Wire into API: `/reason/gaps`, `/query?enrich=`, `/mesh/query` | Small | 10.15, 10.13 |

### Phase 11: Streaming

| Step | Task | Effort | Dependencies |
|------|------|--------|-------------|
| 11.1 | `EventBus` (tokio broadcast channel) in `engram-core` | Small | 7.1 |
| 11.2 | SSE event subscription endpoint (`GET /events/stream`) | Medium | 11.1 |
| 11.3 | Webhook receiver endpoint (`POST /ingest/webhook/{id}`) | Medium | 8.13 |
| 11.4 | WebSocket ingest endpoint (`WS /ingest/ws/{id}`) | Medium | 8.13 |
| 11.5 | SSE response streaming for enrichment (`?enrich=await`) | Medium | 10.11 |
| 11.6 | SSE ingest progress streaming (`GET /batch/jobs/{id}/stream`) | Small | 7.3 |
| 11.7 | MCP tools: `engram_gaps`, `engram_enrich`, `engram_sources` | Medium | 10.17, 8.27 |
| 11.8 | MCP tools: `engram_mesh_discover`, `engram_mesh_query` | Small | 10.13 |
| 11.9 | MCP restricted tools: `engram_ingest`, `engram_create_rule` (opt-in config) | Small | 8.16, 9.10 |
| 11.10 | A2A skills: `ingest_text`, `enrich_query`, `analyze_gaps` | Medium | 8.16, 10.17 |
| 11.11 | A2A skills: `federated_search`, `suggest_investigations` | Small | 10.13, 10.9 |
| 11.12 | A2A streaming task support for long-running operations | Medium | 11.5 |
| 11.13 | gRPC proto definitions (`proto/engram_v110.proto`) | Medium | 8.27, 10.17 |
| 11.14 | gRPC server-streaming RPCs (ingest progress, enrichment, events) | Medium | 11.13, 11.2 |
| 11.15 | gRPC client-streaming RPC (bulk ingest) | Small | 11.13, 8.16 |

### Phase 12: Frontend (Leptos WASM)

| Step | Task | Effort | Dependencies |
|------|------|--------|-------------|
| 12.1 | Create `engram-ui` Leptos crate (Trunk.toml, main.rs, app.rs, router) | Medium | None |
| 12.2 | Shared components: nav, toast, modal, settings, table, stat_card | Medium | 12.1 |
| 12.3 | API client module (gloo-net, context provider, error handling) | Small | 12.1 |
| 12.4 | SSE listener component (EventSource -> Leptos signals) | Medium | 12.1 |
| 12.5 | vis.js interop (wasm-bindgen extern, GraphCanvas component) | Medium | 12.1 |
| 12.6 | Page: Dashboard (stats, health, system overview) | Medium | 12.2, 12.3 |
| 12.7 | Page: Graph (vis.js visualization, node inspector, filtering) | Large | 12.5, 12.3 |
| 12.8 | Page: Search (BM25 + semantic, result list, property filters) | Medium | 12.3 |
| 12.9 | Page: Natural Language (/tell, /ask, conversation view) | Medium | 12.3 |
| 12.10 | Page: Import (JSON-LD upload/download, preview) | Small | 12.3 |
| 12.11 | Page: Learning (confidence scores, reinforcement, decay timeline) | Medium | 12.3 |
| 12.12 | Page: Ingest (pipeline config, live progress via SSE) | Large | 12.2, 12.4, 8.16 |
| 12.13 | Page: Sources (health dashboard, usage, ledger view) | Medium | 12.3, 8.27 |
| 12.14 | Page: Actions (rule editor, dry run, event log) | Medium | 12.3, 9.10 |
| 12.15 | Page: Gaps (black area map, severity table, LLM suggestions with warning) | Large | 12.5, 12.3, 10.17 |
| 12.16 | Page: Mesh (peer topology, profiles, federated query, trust controls) | Large | 12.5, 12.3, 10.13 |
| 12.17 | CSS migration (adapt existing style.css for Leptos class bindings) | Medium | 12.6-12.16 |
| 12.18 | Trunk release build integration (output to frontend/dist, served by engram) | Small | 12.17 |

---

## 14. Testing Strategy

### 14.1 Test Categories

| Category | What | How | Target |
|----------|------|-----|--------|
| **Unit** | Individual stages, traits, algorithms | `#[cfg(test)]` in each module | 100% of public APIs |
| **Integration** | Pipeline end-to-end, API endpoints | `tests/` directory, real graph instance | Every pipeline stage combination |
| **Property** | NER correctness, confidence math | `proptest` / `quickcheck` | Confidence never >1.0, never <0.0 |
| **Benchmark** | Pipeline throughput, NER latency | `criterion` benchmarks | Regression detection |
| **Scenario** | Full use cases, realistic data | Dedicated test scenarios with fixture data | Each use case in section 15 |

### 14.2 Key Test Modules

#### engram-ingest tests

```
tests/
  ingest_pipeline.rs       End-to-end pipeline execution
  pipeline_shortcuts.rs    Skip NER/resolve/conflict via query params
  ner_gazetteer.rs         Graph gazetteer builds correctly, refreshes, multilingual
  ner_rules.rs             Rule matching, per-language rules, no false positives
  ner_chain.rs             Cascade/merge strategies, fallback behavior
  ner_anno.rs              Anno backend integration (feature-gated test)
  ner_learned.rs           Pattern learning from graph, threshold enforcement
  ner_correction.rs        Correction loop updates gazetteer and patterns
  entity_resolution.rs     Conservative ER: progressive 4-step, maybe_same_as edges
  dedup.rs                 Content hash, semantic dedup, threshold tuning
  conflict.rs              All conflict cases: supersede, contest, dispute, temporal
  confidence.rs            Learned trust x extraction confidence math
  language_detect.rs       Mixed-language document handling
  streaming.rs             NDJSON stream, backpressure, partial failure
  source_capabilities.rs   Source trait, capabilities declaration, usage endpoint
  file_source.rs           File watch mode, mtime cursor, format auto-detect
  search_ledger.rs         Temporal cursors, content hash, subsumption, adaptive frequency
  mesh_fast_path.rs        Skip NER, local resolve, peer trust multiplier
  source_health.rs         Success/failure rate tracking, auth status
```

#### engram-action tests

```
tests/
  event_bus.rs             Event emission, subscription, bounded overflow
  rule_parser.rs           TOML rule parsing, condition validation
  rule_eval.rs             Condition matching against events
  effects.rs               Internal effects (confidence, edges, tiers)
  dynamic_ingest.rs        CreateIngestJob effect, QueryTemplate, ReconcileStrategy
  webhooks.rs              External webhook delivery, retry, timeout
  safety.rs                Cooldown enforcement, chain depth, budget limits
  dry_run.rs               Dry run produces correct output without side effects
```

#### engram-reason tests

```
tests/
  frontier.rs              Frontier node detection accuracy
  structural_holes.rs      Missing link detection
  asymmetric.rs            Cluster size comparison
  temporal_gaps.rs         Stale entity detection
  confidence_desert.rs     Low-confidence cluster detection
  severity_scoring.rs      Ranking algorithm
  enrichment_trigger.rs    Gap -> enrichment action chain
  mesh_profiles.rs         Profile auto-derivation, gossip broadcast, discovery
  federated_query.rs       Cross-node search, result merging, ACL filtering
  mesh_black_areas.rs      Mesh-level uncovered area detection
  tiered_enrichment.rs     Mesh > free > paid ordering, tier fallback
  llm_suggestions.rs       LLM query generation from black areas (no auto-execute)
```

### 14.3 Test Data Fixtures

Standardized test datasets for reproducible testing:

```
tests/fixtures/
  small_graph.brain          50 nodes, 100 edges (unit tests)
  medium_graph.brain         5,000 nodes, 20,000 edges (integration)
  multilingual_docs/         Documents in en, de, zh, ru, ar
  ner_golden/                Known-good NER extractions for validation
  conflict_scenarios/        Predefined conflict cases with expected resolutions
  rules/                     Test rule definitions
```

### 14.4 Benchmarks

```
benches/
  ingest_throughput.rs       Facts/second through full pipeline
  ingest_shortcuts.rs        Throughput with skip params vs full pipeline
  ner_latency.rs             Per-document NER processing time
  gazetteer_lookup.rs        Lookup speed at various graph sizes
  entity_resolution.rs       Resolution speed vs graph size
  search_ledger.rs           Ledger lookup + hash check at various sizes
  event_bus.rs               Events/second through bus + subscribers
  gap_detection.rs           Full scan time at various graph sizes
  federated_query.rs         Cross-node query latency (loopback)
  mesh_fast_path.rs          Mesh ingest throughput vs full pipeline
```

**Target metrics:**
- Ingest throughput: >10,000 facts/second (full pipeline, 8 workers)
- NER latency: <50ms per document (gazetteer + rules)
- Gazetteer lookup: <1us per entity (from cache)
- Event bus: >100,000 events/second
- Gap detection: <5 seconds for 100K node graph

---

## 15. Use Cases

### 15.1 Use Case 14: Real-Time News Intelligence Pipeline

**Demonstrates:** Ingest pipeline, NER, confidence model, action engine, streaming

**Scenario:** Continuous monitoring of geopolitical news via GDELT API. Articles are ingested, entities extracted, relationships discovered, and alerts triggered when relevant patterns emerge.

```
GDELT webhook -> engram webhook receiver
  -> Parse (JSON articles)
  -> Language detect (multilingual news)
  -> NER (extract people, orgs, locations, events)
  -> Entity resolve (match against existing graph)
  -> Conflict check ("sanctions lifted" vs existing "sanctions active")
  -> Confidence tag (source: news, trust: 0.20)
  -> Store
  -> Action: "sanctions-alert" fires, webhook to Slack
  -> Dashboard updates via SSE event stream
```

**Tests:**
- Ingest 1000 GDELT articles, verify NER extraction accuracy
- Verify conflict detection between new and existing facts
- Verify action fires within configured cooldown
- Verify SSE stream delivers events to dashboard

### 15.2 Use Case 15: Enterprise Document Knowledge Extraction

**Demonstrates:** Ingest pipeline, NER (multilingual), entity resolution, graph learning

**Scenario:** A company ingests its internal documentation (Confluence, wikis, PDFs) into engram. NER extracts entities (people, projects, technologies, decisions). Entity resolution prevents duplicates. The graph gazetteer improves over time.

```
File watcher on /docs directory
  -> Parse (PDF, HTML, Markdown)
  -> Language detect (English + German mixed docs)
  -> NER chain:
     1. Graph gazetteer (known employees, projects)
     2. Custom rules (project codes: PRJ-xxxx)
     3. SpaCy (general entities)
  -> Entity resolve (fuzzy: "J. Smith" = "John Smith")
  -> Dedup (same fact from meeting notes + wiki)
  -> Store with source: "internal-docs", trust: 0.80
```

**Tests:**
- Ingest sample multi-format document set
- Verify graph gazetteer improves NER over iterations
- Verify entity resolution merges duplicates correctly
- Verify dedup prevents redundant storage

### 15.3 Use Case 16: Knowledge Gap Discovery and Auto-Enrichment

**Demonstrates:** Black area detection, spearhead search, query-triggered enrichment, streaming

**Scenario:** An analyst queries engram about "titanium supply chain risks." Engram has limited data. The gap detector identifies this as a black area. Enrichment fans out to web search, academic databases, and government sources. Results stream back to the analyst.

```
Analyst: GET /query?q=titanium+supply+chain+risks&enrich=await

  Local results: 2 facts, avg confidence 0.25
  -> QueryGap event emitted
  -> Black area: asymmetric cluster (Russia/sanctions: 40 facts,
                                     Russia/titanium: 2 facts)
  -> Spearhead search:
     - Brave search: "titanium supply chain sanctions"
     - Semantic Scholar: "titanium trade disruption"
     - Gov DB: "OFAC sanctions titanium"
  -> Each result through ingest pipeline
  -> SSE stream to analyst:
     event: local     (2 results)
     event: enriched  (brave: 5 new facts)
     event: enriched  (scholar: 3 new facts)
     event: conflict  (1 contradiction found)
     event: complete  (10 new facts total)
```

**Tests:**
- Verify gap detection identifies asymmetric clusters
- Verify enrichment fans out to multiple sources concurrently
- Verify SSE stream delivers progressive results
- Verify enrichment results pass through full pipeline (NER, conflict, confidence)
- Verify cooldown prevents repeated enrichment for same query

### 15.4 Use Case 17: Automated Fact Verification and Conflict Resolution

**Demonstrates:** Conflict resolution, confidence model, action engine, correction loop

**Scenario:** A news article claims "Company X CEO is Alice." Engram already stores "Company X CEO is Bob." The conflict detection catches this, flags it for review, and when an analyst confirms Alice is the new CEO, the temporal succession is recorded and NER learns the updated association.

```
Ingest: "Alice appointed CEO of Company X"
  -> NER: PERSON="Alice", ORG="Company X"
  -> Entity resolve: "Company X" matches existing node
  -> Conflict: existing edge (Bob --[ceo_of]--> Company X)
     contradicts new edge (Alice --[ceo_of]--> Company X)
  -> Resolution: temporal_succession (Bob valid_until=now)
  -> Action: "conflict-review" webhook fires
  -> Analyst confirms: correction stored
  -> NER correction: "Alice" + "Company X" + "CEO" -> PERSON pattern +1
  -> Gazetteer updated: Alice(PERSON) added
```

**Tests:**
- Verify temporal succession creates proper audit trail
- Verify both facts retained with correct `valid_until` timestamps
- Verify correction feedback updates NER patterns
- Verify gazetteer refresh includes newly confirmed entity

### 15.5 Use Case 18: Multi-Source Intelligence Dashboard (Streaming)

**Demonstrates:** Event streaming, SSE, WASM frontend, action engine

**Scenario:** A live dashboard subscribes to engram's event stream. It shows real-time ingestion, alerts, knowledge gaps, and conflict resolution -- all updating without polling.

```
Dashboard connects: GET /events/stream?topics=*

  event: fact_stored     -> counter increments, graph updates
  event: conflict        -> alert panel highlights
  event: action_fired    -> action log updates
  event: black_area      -> gap map highlights new gap
  event: enriched        -> enrichment panel shows progress
```

**Tests:**
- Verify SSE connection stability over extended period
- Verify event filtering works correctly
- Verify dashboard receives events within 100ms of graph change
- Verify reconnection handles gracefully after network interruption

### 15.6 Use Case 19: Dependency Monitoring and Version Reconciliation

**Demonstrates:** Scheduled source, event-triggered dynamic ingest, `ReconcileStrategy::VersionSupersede`, action engine

**Scenario:** A development team monitors critical dependencies (React, PostgreSQL, OpenSSL) via GitHub Releases API. When a new version is detected, engram automatically fetches changelogs, identifies breaking changes, and reconciles against existing knowledge.

```
Scheduled source: GitHub Releases API for facebook/react
  -> Polls every 6h, temporal cursor = last seen tag
  -> New release detected: v19.1.0

Ingest stores: React:v19.1.0 --[supersedes]--> React:v19.0.0

Action engine fires "new-version-detected" rule:
  -> CreateIngestJob:
     queries: ["React 19.1 changelog", "React 19.1 breaking changes",
               "React 19.1 migration guide", "React 19.1 security fixes"]
     sources: [github-api, web-search]
     reconcile: VersionSupersede { old_version: "React:v19.0.0" }

Ingest results:
  -> "useFormStatus hook deprecated" -> marks v19.0.0 fact as superseded
  -> "CVE-2026-1234 fixed" -> flags old vulnerability as resolved
  -> "new useOptimistic API" -> stored as new knowledge, linked to v19.1.0

Dashboard shows:
  -> 3 superseded facts, 1 security fix resolved, 2 new APIs discovered
  -> Diff view: v19.0.0 vs v19.1.0 knowledge delta
```

**Tests:**
- Verify temporal cursor correctly skips already-seen releases
- Verify VersionSupersede marks old facts as superseded (not deleted)
- Verify security CVE facts are flagged as resolved when fix detected
- Verify dynamic ingest job expires after all queries complete
- Verify action rule fires only once per new version (cooldown)

### 15.7 Use Case 20: Document Dropzone with Continuous Learning

**Demonstrates:** File source, watch mode, NER chain with graph learning, entity resolution

**Scenario:** An analyst drops intelligence reports (PDF, text, markdown) into a watched folder. Engram automatically ingests them, extracts entities, resolves against the existing graph, and improves NER accuracy over time as the graph grows.

```
Config: FileSource watching /data/reports/
  -> notify crate detects new file: "iran_titanium_report_march.pdf"

Pipeline:
  -> Parse (PDF extraction)
  -> Language detect: English (confidence 0.98)
  -> NER chain:
     1. Graph gazetteer: "Iran Titanium Corp" (exact match, 0.99)
        "Rostec" (exact match, 0.97)
     2. Rules: "OFAC-2026-0147" matches sanctions pattern (0.95)
     3. Anno (GLiNER2): "Deputy Minister Rezaei" (PERSON, 0.88)
        "Bandar Abbas port" (LOCATION, 0.85)
  -> Entity resolve:
     "Iran Titanium Corp" -> matched to existing node (hash index hit)
     "Rostec" -> matched (hash index hit)
     "Deputy Minister Rezaei" -> ambiguous (0.72):
       maybe_same_as "Ali Rezaei" (existing) OR new entity?
       -> creates new entity + maybe_same_as edge for human review
     "Bandar Abbas port" -> new entity (no match above 0.60)
  -> Conflict check: report claims "shipment date: March 2026"
     existing fact: "embargo effective: January 2026"
     -> no contradiction (different predicates), both stored
  -> Store with source: "file:/data/reports/iran_titanium_report_march.pdf"
     trust: 0.60 (internal document)

Graph learning after 50+ reports:
  -> Gazetteer grows: "Rezaei" now resolves unambiguously (accumulated evidence)
  -> Learned pattern: "OFAC-YYYY-NNNN" -> SANCTIONS_ID (auto-promoted from rules)
  -> NER accuracy improves: fewer ambiguous entities per report

File change detected: analyst updates the PDF
  -> Content hash differs from stored hash -> re-ingest
  -> New facts reconciled against existing (dedup + conflict check)
  -> Changed facts flagged with updated provenance
```

**Tests:**
- Verify file watch triggers ingest on new file
- Verify file change detection (mtime + content hash)
- Verify PDF parsing extracts text correctly
- Verify graph gazetteer improves entity resolution accuracy over iterations
- Verify ambiguous entities create `maybe_same_as` edges (conservative ER)
- Verify learned patterns promote from rules after threshold evidence
- Verify re-ingest of changed file reconciles correctly (no duplicates)

### 15.8 Use Case 21: Mesh Federated Intelligence (Multi-Analyst)

**Demonstrates:** Mesh federated query, knowledge profiles, 3-tier enrichment, peer access monitoring

**Scenario:** Three analysts cover different domains. When one analyst's query crosses into another's domain, engram federates the query to the right peer -- avoiding paid API calls and getting pre-verified facts.

```
Setup:
  iran-desk:   50K facts about Iran (sanctions, nuclear, oil)
  russia-desk: 80K facts about Russia (military, sanctions, energy)
  china-desk:  35K facts about China (trade, tech, maritime)

Each node auto-derives knowledge profile and broadcasts via gossip:
  iran-desk announces:  ["Iran", "sanctions", "nuclear", "oil"] (depth: 0.9)
  russia-desk announces: ["Russia", "military", "sanctions", "energy"] (depth: 0.85)
  china-desk announces:  ["China", "trade", "tech", "maritime"] (depth: 0.7)

Query on russia-desk: "Iran Russia arms deal titanium"
  -> Tier 1: Local search -> 3 facts (Russia side only)
  -> Tier 1: Mesh profile check -> iran-desk covers "Iran" (depth 0.9)
     -> Federated query to iran-desk
     -> iran-desk searches its graph: 12 matching facts
     -> Results returned with source="mesh:iran-desk"
     -> ACL filter applied: only "public" + "internal" facts
        (russia-desk has sensitivity_ceiling = "internal" for iran-desk)
  -> Merge: 3 local + 12 federated = 15 facts
  -> Sufficient! Skip Tier 2 (free external) and Tier 3 (paid)

Cost saved: 0 API calls. All knowledge came from mesh.

Peer access monitor on iran-desk shows:
  russia-desk: 47 queries/24h, 312 results returned, trust 0.92
  -> Normal pattern, no alerts

Later: unknown-node-7 peers with iran-desk, starts querying:
  -> 847 queries/24h, probing "nuclear program", "enrichment facilities"
  -> Action engine fires alert: unusual volume + sensitive topics
  -> Admin reviews in frontend, clicks [Detrust] -> peer drops to public-only
```

**Tests:**
- Verify knowledge profile auto-derivation from graph clusters
- Verify profile gossip broadcasts on significant graph change
- Verify federated query returns correct results from peer
- Verify ACL filtering respects sensitivity ceiling per peer
- Verify 3-tier ordering (mesh first, then free, then paid)
- Verify cost savings: no external API calls when mesh provides sufficient results
- Verify peer access metrics tracking (query count, results, timestamps)
- Verify anomaly alert fires on unusual query patterns
- Verify detrust immediately revokes elevated access

### 15.9 Use Case 22: Incident Handling and Event Correlation

**Demonstrates:** Webhook ingest, NER, co-occurrence learning, action engine, mesh federated query, black area detection, event-triggered dynamic ingest

**Scenario:** An operations team uses engram as an incident knowledge base. Monitoring tools push alerts via webhooks. Incidents are modeled as graph entities with edges to services, people, root causes, and resolutions. Over time, engram learns failure patterns and surfaces knowledge gaps in infrastructure documentation.

```
Sources:
  - PagerDuty webhooks (alerts, incidents, escalations)
  - Grafana webhooks (metric threshold breaches)
  - Datadog webhooks (APM traces, error spikes)
  - Jira API (incident tickets, postmortems)
  - Git webhooks (deploys, rollbacks)

Webhook: PagerDuty fires "CRITICAL: payment-api response time > 5s"

Ingest pipeline:
  -> Parse (JSON webhook payload)
  -> NER chain:
     1. Gazetteer: "payment-api" matches existing service node (0.99)
     2. Rules: "CRITICAL" -> SEVERITY entity, "5s" -> METRIC_VALUE
     3. Anno: "response time" -> METRIC_TYPE (0.88)
  -> Entity resolve: "payment-api" = existing Service:payment-api
  -> Store:
     Alert:2026-0315-14:28 --[affects]--> Service:payment-api
     Alert:2026-0315-14:28 --[severity]--> critical
     Alert:2026-0315-14:28 --[metric]--> response_time:5s

Action engine fires:
  Rule "critical-service-alert":
    -> Create incident entity: Incident:2026-0315-001
    -> Link: Incident:2026-0315-001 --[triggered_by]--> Alert:2026-0315-14:28
    -> Webhook to Slack: "#incidents: CRITICAL on payment-api"
    -> Enrich: "payment-api recent deploys" (query git deploy source)
    -> Enrich: "payment-api dependencies" (federated query to dev-team mesh node)

Subsequent webhooks arrive:
  Grafana: "CPU spike on payment-api-pod-3 at 14:28"
  Datadog: "Error rate 45% on POST /api/checkout at 14:29"
  Git webhook: "Deploy v2.3.1 to payment-api at 14:15"

Ingest resolves all to existing Incident:2026-0315-001:
  -> Incident:2026-0315-001 --[symptom]--> CPU_spike_pod3
  -> Incident:2026-0315-001 --[symptom]--> error_rate_45pct
  -> Incident:2026-0315-001 --[preceded_by]--> Deploy:v2.3.1 (14:15, 13min before)

Co-occurrence learning detects:
  "deploy_v2.x followed by cpu_spike within 30min: 3/3 times"
  -> Evidence surfaced on next deploy query

Resolution recorded:
  Incident:2026-0315-001 --[resolved_by]--> Action:rollback_v2.3.0
  Incident:2026-0315-001 --[root_cause]--> Bug:memory_leak_v2.3.1
  Incident:2026-0315-001 --[postmortem]--> Doc:PM-2026-0315
  Incident:2026-0315-001 --[duration]--> 45min

Mesh federated query (ops-desk -> dev-desk):
  "What changed in payment-api v2.3.1?"
  -> dev-desk returns: 3 commits, 1 dependency upgrade, 2 API changes
  -> Linked to incident as contributing factors

Black area detection:
  "payment-api has 12 incidents but 0 facts about its database dependencies"
  -> Severity 0.85, structural hole
  -> Suggested investigation: "payment-api database architecture"

Over time (after 50+ incidents):
  Graph learns patterns:
  - "deploy on Friday after 15:00 -> incident within 2h" (co-occurrence: 4/5)
  - "payment-api + high-traffic-event -> CPU spike" (3/3)
  - "rollback resolves 80% of deploy-related incidents"
  These are evidence, not rules. Surfaced as context on future queries.

Next deploy: "payment-api v2.4.0 deploying to production"
  -> Query: "payment-api deploy risks"
  -> Engram returns:
     - 3 recent incidents linked to deploys
     - Co-occurrence: "deploy -> incident within 2h" (80% observed)
     - Black area: "no load test data for v2.4.0"
     - Suggested: run load test before deploy
```

**Tests:**
- Verify webhook ingest processes PagerDuty/Grafana/Datadog payloads correctly
- Verify NER extracts service names, severity levels, metric values from alert text
- Verify entity resolution links multiple alerts to same incident
- Verify action engine creates incident entity and fires Slack webhook
- Verify co-occurrence learning detects deploy-to-incident patterns after N observations
- Verify mesh federated query retrieves code change context from dev-team node
- Verify black area detection identifies missing infrastructure documentation
- Verify temporal correlation: events within time window linked to same incident
- Verify resolution/postmortem recording updates incident entity correctly

### 15.10 Use Case 23: Influence Network Detection and Source Quality Analysis

**Demonstrates:** Author trust (auto-adjusting), co-occurrence learning, asymmetric cluster detection, correction propagation, graph topology analysis, frontend visualization

**Scenario:** An analyst team monitors social media and news sources for coordinated influence operations targeting elections. Engram ingests content from multiple platforms, tracks author trust over time, and surfaces coordinated networks through graph topology analysis.

```
Sources:
  - Twitter/X API (posts, retweets, author metadata)
  - Telegram channel monitor (messages, forwards)
  - RSS feeds (news sites, blogs)
  - GDELT (global news aggregation)
  - Fact-checking APIs (Snopes, PolitiFact)

Phase 1: Ingest and author tracking

  Twitter ingest: 500 tweets/day mentioning "election" + country
  Each tweet creates:
    -> Fact node: "Claim:NATO_will_collapse_2026"
    -> Author node: "Author:twitter:@freedom_eagle_88" (trust: 0.10, X baseline)
    -> Edge: Claim --authored_by--> Author:twitter:@freedom_eagle_88
    -> NER extracts: PERSON, ORG, LOCATION, EVENT entities
    -> Entity resolution links to existing graph entities

Phase 2: Corroboration and correction (automatic)

  Fact-checker API returns: "NATO collapse claim rated FALSE by PolitiFact"
  -> Correction applied to Claim:NATO_will_collapse_2026
  -> Correction propagates to author via authored_by edge
  -> Author:twitter:@freedom_eagle_88 trust: 0.10 -> 0.08

  Meanwhile, Author:twitter:@osint_verified posts confirmed fact about
  troop movements, corroborated by Reuters:
  -> Author:twitter:@osint_verified trust: 0.10 -> 0.15 -> 0.22

  Over weeks, author trust landscape sharpens:
    Reliable authors: 0.30 - 0.65 (consistently corroborated)
    Neutral authors: 0.08 - 0.12 (mixed or unverified)
    Unreliable authors: 0.01 - 0.05 (repeatedly corrected)

Phase 3: Network detection (graph topology)

  Co-occurrence tracker detects:
    "@freedom_eagle_88, @truth_patriot_99, @news_watcher_44"
    all post identical claims within 30-minute windows
    -> Co-occurrence count: 47 overlapping claims in 2 weeks

  Asymmetric cluster analysis finds:
    Cluster A: 23 authors, 340 internal edges (retweet/quote each other)
                4 external edges (almost never cite independent sources)
    Cluster B: 8 authors, 15 internal edges
                89 external edges (cite Reuters, AP, government sources)

    Cluster A is a dense, isolated subgraph = coordinated network signal
    Cluster B is a normal citation pattern

  Black area detection:
    "Cluster A has 340 claims but 0 corroborations from independent sources"
    -> Severity: 0.95 (extreme asymmetry)

  Temporal analysis:
    Cluster A publishing pattern: synchronized bursts at 09:00 Moscow time
    13 of 23 accounts created within same 48-hour window
    Activity spikes correlate with election debate schedule

Phase 4: Frontend visualization

  Gaps page shows:
  +--------------------------------------------------------------+
  |  NETWORK ANOMALIES                                           |
  +--------------------------------------------------------------+
  | Severity | Pattern           | Authors | Claims | Corr. rate |
  |----------|-------------------|---------|--------|------------|
  | 0.95     | Coordinated burst | 23      | 340    | 0%         |
  | 0.72     | Echo chamber      | 8       | 89     | 12%        |
  | 0.45     | Low-trust cluster | 5       | 23     | 35%        |
  +--------------------------------------------------------------+

  Graph page shows:
  - Cluster A highlighted in red (dense internal, sparse external)
  - Author trust displayed as node color gradient (green=high, red=low)
  - Edge thickness = co-occurrence strength
  - Timeline slider shows network formation over time

  Analyst clicks Cluster A:
  +--------------------------------------------------------------+
  |  CLUSTER ANALYSIS: "Coordinated Group Alpha"                  |
  +--------------------------------------------------------------+
  |  Authors: 23  |  Avg trust: 0.03  |  Corroboration: 0%       |
  |  Created: 2026-01-15 to 2026-01-17 (48h window)              |
  |  Active: 09:00-11:00 UTC daily                                |
  |  Top claims: "NATO collapse", "election fraud", "sanctions    |
  |              lifting" (all rated FALSE by fact-checkers)       |
  |                                                                |
  |  Shared by: 3 Telegram channels with same temporal pattern     |
  |                                                                |
  |  [Export network report] [Flag all authors] [Track cluster]    |
  +--------------------------------------------------------------+

Phase 5: Ongoing quality management

  Action rule fires on new ingest:
    IF author.trust < 0.05 AND author IN flagged_cluster:
      -> Automatically assign low confidence (0.01) to their facts
      -> Log but don't suppress (evidence of activity is still valuable)
      -> Notify analyst: "Known low-trust network active"

  New author appears posting same claims as Cluster A:
    -> Entity resolution: different account, same content fingerprint
    -> Co-occurrence links to existing cluster
    -> Inherits low trust via correction propagation from cluster
    -> Surfaced in frontend: "New account matches Cluster A pattern"

  Monthly report query: "author trust distribution by source"
    -> Shows platform-level quality: "X: 15% of authors above 0.30"
    -> Shows improvement over time as bad actors sink, good sources rise
```

**What engram provides that no other system does:**
- **Author trust as emergent property**: no manual blocklists, the graph learns
- **Network detection from topology**: not keyword matching, actual structural analysis
- **Correction propagation across networks**: one fact-check degrades entire cluster
- **Cross-platform correlation**: same narrative appearing on X, Telegram, blogs detected via entity resolution
- **Quality as a byproduct**: the same mechanism that finds influence networks also curates knowledge quality

**Tests:**
- Verify author nodes created with source baseline trust on first encounter
- Verify corroboration raises author trust, correction lowers it
- Verify co-occurrence detects synchronized posting patterns (>N overlaps in time window)
- Verify asymmetric cluster analysis flags dense-internal-sparse-external subgraphs
- Verify correction to one claim propagates trust reduction to author and co-authors
- Verify new account joining existing cluster inherits low trust via co-occurrence
- Verify action rule auto-assigns low confidence to facts from flagged cluster authors
- Verify temporal pattern analysis detects synchronized activity windows
- Verify cross-platform entity resolution links same narrative across sources

### 15.11 Use Case 24: Financial Market Signal Detection

**Scenario:** An analyst monitors financial instruments (stocks, crypto, commodities) by correlating news, filings, social sentiment, and market data. Engram detects emerging signals before they become consensus.

**Why engram, not a trading bot:**

Engram is a knowledge graph, not a prediction engine. It does not predict prices. What it does: correlate information across sources, detect narrative shifts, and surface gaps. The analyst decides what to trade. Engram tells them what they might be missing.

**Pipeline:**

```
Phase 1: Multi-Source Ingest
  - SEC filings (10-K, 10-Q, 8-K) via EDGAR API
  - Earnings call transcripts (structured text)
  - News feeds (Reuters, Bloomberg, financial RSS)
  - Social sentiment (Twitter/X financial accounts, Reddit/WSB)
  - Commodity prices (gold, oil, BTC via public APIs)
  - Central bank statements (Fed, ECB, BoJ)

Phase 2: Entity Extraction + Resolution
  - NER: companies, executives, ticker symbols, monetary values, dates
  - Resolution: "AAPL" = "Apple Inc." = "Apple" across all sources
  - Relation extraction: "CEO departure", "merger announced", "earnings beat"

Phase 3: Signal Correlation
  - Co-occurrence tracking: which entities appear together in recent ingests?
  - Temporal clustering: sudden spike in mentions = emerging narrative
  - Confidence weighting: SEC filing > analyst report > social media
  - Contradiction detection: bull vs bear narratives on same entity
  - Black area detection: "everyone is talking about X, but nobody mentions Y
    which is X's largest supplier" -> gap = potential blind spot

Phase 4: Enrichment Triggers
  - Action rule: when new entity appears in SEC filing with > 3 edges
    to monitored entities -> auto-ingest related filings
  - Gap detection: "Gold mentioned 50x this week but no central bank
    context ingested" -> suggest: "Fed gold reserve policy", "ECB gold
    purchases 2026"
  - Mesh: analyst A monitors tech, analyst B monitors commodities ->
    cross-domain correlation via mesh sync

Phase 5: Analyst Dashboard
  - Graph view: entity clusters with edge weights = correlation strength
  - Timeline: narrative evolution (when did "recession" narrative start?)
  - Confidence map: which claims are well-sourced vs. echo chamber?
  - Alert: new contradictions (bull/bear divergence on same ticker)
```

**What engram provides that Bloomberg Terminal doesn't:**
- Cross-source provenance (who said what, when, how reliable)
- Learned trust per source and author (not all analysts are equal)
- Gap detection (what is NOT being discussed that should be)
- Contradiction surfacing (conflicting narratives with confidence scores)
- Full audit trail (every fact traceable to original source)

**What engram does NOT do:**
- Price prediction (no ML models, no technical analysis)
- Trading execution (no broker integration)
- Real-time market data streaming (ingests on schedule, not tick-by-tick)
- Financial advice (it's a knowledge graph, not an advisor)

**Test plan:**
- Ingest sample earnings transcripts, verify entity extraction (company, revenue, guidance)
- Ingest contradictory analyst reports, verify conflict detection
- Verify co-occurrence spike detection on simulated news burst
- Verify gap detection finds missing supply chain context
- Verify learned trust differentiates SEC filings from social media over time

---

## 16. Security Considerations

### 16.1 IP Protection

- **Source code:** Never pushed to GitHub. Gitea at `192.168.178.26:3141` only.
- **Frontend logic:** WASM-compiled. Pipeline validation, NER testing, rule building -- all in compiled binary. No source inspection possible.
- **GitHub releases:** Binary-only distribution. No Rust source, no WASM source.

### 16.2 Ingest Security

- **Input validation:** All ingested data sanitized before graph insertion. No injection through entity labels or property values.
- **Rate limiting:** Per-source rate limits on enrichment queries. Prevents abuse of external APIs.
- **Source trust:** Learned from corroboration/correction history (Section 5.1). No hardcoded tiers -- all sources start at the same baseline.
- **LLM restrictions:** LLM-extracted facts cannot trigger actions, cannot supersede existing facts, always flagged.

### 16.3 Action Engine Safety

- **Cooldown enforcement:** Rules cannot fire faster than configured cooldown. No infinite loops.
- **Chain depth limit:** Default max 5 levels of rule chaining. Prevents cascade avalanches.
- **Effect budget:** Max external calls per minute. Prevents webhook storms.
- **Dry run first:** New rules should be tested in dry-run mode before enabling.
- **Audit log:** Every action execution logged with full context.

### 16.4 Enrichment Security

- **API key management:** Keys stored in environment variables, never in config files.
- **Network isolation:** Enrichment sources are whitelisted. No arbitrary URL fetching.
- **Result validation:** Enrichment results pass through the full ingest pipeline, including NER, resolution, and conflict checking. No raw data enters the graph.
- **Cooldown:** Same query (or semantically similar) within window returns cached results.

### 16.5 Mesh Federated Query Security

Federated queries expose the local graph to peers. A malicious or compromised peer could probe the graph by sending thousands of targeted queries to extract knowledge. Security is enforced at multiple levels, with **human oversight as the final authority**.

**Per-peer controls:**

| Control | Description | Default |
|---------|-------------|---------|
| **Sensitivity ceiling** | Maximum fact sensitivity a peer can see (`public`, `internal`, `confidential`, `restricted`) | `public` |
| **Query rate limit** | Max federated queries per minute from this peer | 10/min |
| **Result cap** | Max facts returned per query to this peer | 100 |
| **Topic restriction** | Peer can only query specific topics (optional) | All topics |
| **Trust level** | Peer trust score from v1.0.0 mesh trust model | Earned over time |

**Access metrics (visible in frontend):**

```
+--------------------------------------------------------------+
|  <i class="fa fa-shield-alt"></i> PEER ACCESS MONITOR                                     |
+--------------------------------------------------------------+
| Peer            | Trust | Queries | Results  | Last Query    |
|                 |       | (24h)   | Returned |               |
|-----------------|-------|---------|----------|---------------|
| iran-desk       | 0.92  | 47      | 312      | 5m ago        |
| china-desk      | 0.85  | 12      | 89       | 2h ago        |
| unknown-node-7  | 0.15  | 847     | 0        | 30s ago  (!)  |
+--------------------------------------------------------------+
|                                                               |
| <i class="fa fa-exclamation-triangle"></i> unknown-node-7: unusual query volume (847/24h)        |
|   Top queries: "classified weapons", "nuclear program",      |
|                "intelligence assets"                          |
|   [View full log] [Detrust] [Block]                          |
|                                                               |
+--------------------------------------------------------------+
```

**Trust/detrust is a human decision:**

- **Trust:** Explicit action in frontend. Admin reviews peer identity, query patterns, and purpose before granting access above `public`.
- **Detrust:** Immediate revocation. All active queries from this peer are terminated. Peer drops to `public` ceiling or is blocked entirely.
- **Auto-alert:** Unusual patterns trigger action engine events (high query volume, sensitive topic probing, queries outside declared knowledge domain). The system alerts the human -- it does not auto-block.
- **Audit trail:** Every federated query is logged with full context (peer, query, results returned, sensitivity levels accessed, timestamp).

**Configuration:**

```toml
# Per-peer overrides in mesh config
[[mesh.peers]]
id = "iran-desk"
name = "Iran Analysis Desk"
sensitivity_ceiling = "internal"    # can see public + internal facts
query_rate_limit = "30/min"         # trusted, higher limit
result_cap = 200
# topics = ["Iran", "sanctions"]   # optional topic restriction

[[mesh.peers]]
id = "external-partner"
name = "External Research Partner"
sensitivity_ceiling = "public"      # only public facts
query_rate_limit = "5/min"          # low trust, strict limit
result_cap = 50
topics = ["economic", "trade"]      # restricted to specific topics
```

**Key principle:** Engram surfaces anomalies and provides controls. It never makes trust decisions automatically. A human reviews the access patterns and decides to trust, detrust, or block.

---

## Appendix A: NER Backend Research (Updated 2026-03-10)

*Based on web research and crate analysis.*

### Rust-Native Options

| Crate | Description | Status | Notes |
|-------|-------------|--------|-------|
| **`anno`** | Multi-backend NER with GLiNER2. Zero-shot entity types, coreference, entity linking, KG export. | Production-ready | Best all-rounder. Falls back from GLiNER to heuristic. |
| **`gline-rs`** | GLiNER ONNX wrapper. Zero-shot NER -- define entity types at runtime. | Production-ready | NAACL 2024 paper. Pre-converted ONNX models on HuggingFace. |
| **`rust-bert`** | Port of HuggingFace transformers via `tch-rs` (libtorch). NER, classification, translation. | Mature | Heavy: ~500MB with libtorch. GPU support. |
| **`candle`** | HuggingFace pure Rust ML framework. Lighter than rust-bert. | Active | Smaller binaries, serverless-friendly. More manual work. |
| **`ort`** | ONNX Runtime bindings. Run any ONNX-exported model. | Production-ready, v2.0+ | Already in engram-core. Reuse for NER. |
| **`tokenizers`** | HuggingFace tokenizer library. 43x faster than Python. | Production-ready | 10.9M downloads. Required for any transformer NER. |
| **`lingua`** | Language detection. 75+ languages, Rust-native, N-gram + Naive Bayes. | Production-ready | Most accurate in Rust ecosystem. |
| **`jieba-rs`** | Chinese word segmentation. Pure Rust port of jieba. | Stable | Fast, dictionary-based. |
| **`lindera`** | CJK tokenization (Chinese, Japanese, Korean). Multiple dictionaries. | Active | cc-cedict (zh), ipadic-neologd (ja), ko-dic (ko). |
| **`language-tokenizer`** | Unified CJK tokenizer API wrapping lindera + regex fallback. | Active | Handles language routing automatically. |
| **`flashtext`** | Fast keyword search via Trie + Aho-Corasick. O(document_length). | Beta | Good for gazetteer matching. Not widely adopted in Rust. |
| **`chinese-ner-rs`** | CRF-based Chinese NER. | Stable | Chinese only. |

### Key Research Findings

1. **GLiNER (NAACL 2024)** is a breakthrough for NER: zero-shot entity type extraction without fine-tuning. Define entity types at runtime ("PERSON", "WEAPON", "SANCTION_TARGET") and the model extracts them. This aligns perfectly with engram's need for domain-specific entity types without retraining.

2. **No Rust entity resolution library exists.** Only 3 Rust projects on GitHub vs 194 Python. This is a **competitive advantage** for engram -- building embedded ER in Rust would be first-of-kind.

3. **Progressive Entity Resolution (2025 research):** Four-step framework: filter (reduce search space) -> weight (similarity scoring) -> schedule (prioritize execution) -> match (apply complex matching). Engram should implement this pattern.

4. **SpaCy integration is generally not worth it** for pure Rust applications. The IPC/HTTP overhead, Python dependency, and deployment complexity outweigh the accuracy gains. ONNX-exported models via `ort` provide comparable accuracy with native performance.

### Recommended Stack for Engram

| Layer | Crate | Why |
|-------|-------|-----|
| Language detection | `lingua` | Rust-native, 75+ languages, fast |
| Gazetteer | Custom (from graph) | No dependency, dynamic, self-updating |
| Fast keyword matching | `flashtext` or custom Aho-Corasick | O(n) lookup for known entities |
| Rule-based NER | Custom (regex + TOML rules) | Domain-specific, per-language |
| Zero-shot NER + coreference | `anno` (feature-gated) | GLiNER2, coreference, candle backend, multi-task extraction |
| Tokenization | `tokenizers` (HuggingFace) | 43x faster than Python, alignment tracking |
| CJK segmentation | `jieba-rs` (Chinese), `lindera` (Japanese) | Rust-native, fast |
| LLM fallback | Ollama/vLLM via existing `embed_api` | Last resort, existing infrastructure |

### Architecture Insights from Research

1. **ELT over ETL:** Modern KG systems favor loading data first, then transforming with graph context (entity resolution uses existing graph). Engram's pipeline should read the graph during resolution (ELT) not just write to it (ETL).

2. **Event Knowledge Graphs (EKG):** Emerging 2025 pattern where key events are graph nodes with temporal properties. Aligns with engram's bi-temporal timestamps and action engine.

3. **Black area detection as first-class feature does not exist in any production system.** Academic work focuses on knowledge graph completion (predicting missing links) but no system explicitly detects and signals its own gaps. This is engram's unique differentiator.

4. **WASM for IP protection is validated** by NIST research and industry adoption, but no existing admin UI examples found. Engram's WASM-compiled pipeline management UI would be novel.

5. **CDC (Change Data Capture) via event streams** is the industry standard for reactive graph updates. Engram's `GraphEvent` bus implements this pattern natively.

---

## Appendix B: Configuration Reference

### Complete Configuration Example

```toml
# engram.toml (v1.1.0 additions)
# All v1.1.0 sections are optional. Omit entirely for v1.0.0-only behavior.

# ═══════════════════════════════════════════════════════════════
# INGEST PIPELINE
# ═══════════════════════════════════════════════════════════════

[ingest]
enabled = true                      # master switch (default: false)
workers = 4                         # pipeline worker threads (leave cores for API)
batch_size = 1000                   # facts per write lock acquisition
batch_timeout_ms = 100              # flush batch after this delay even if not full
channel_buffer = 10000              # backpressure threshold
max_queue_depth = 10000             # reject new items if queue exceeds this

[ingest.parse]
supported_formats = ["json", "csv", "html", "plaintext", "pdf", "md"]

# ═══════════════════════════════════════════════════════════════
# NER (requires `--features ingest`)
# ═══════════════════════════════════════════════════════════════

[ner]
strategy = "cascade"                # "cascade", "merge_all", "cascade_threshold"
cascade_threshold = 3               # for cascade_threshold: proceed if < N entities
min_confidence = 0.3                # discard entities below this

[ner.gazetteer]
enabled = true
refresh_interval = "5m"             # rebuild from graph
min_entity_confidence = 0.6         # only entities above this enter gazetteer

[ner.rules]
files = ["rules/common.toml"]       # per-language rule files
# Additional files loaded by language: rules/finance_en.toml, rules/geo_de.toml

[ner.learned_patterns]
enabled = true
min_evidence = 50                   # observations before pattern activates
min_accuracy = 0.85                 # precision threshold
max_patterns = 1000                 # cap to prevent bloat

[ner.anno]                          # requires `--features anno`
enabled = true
model = "gliner-x-base"            # or "gliner-moe-multi", or custom path
entity_types = ["PERSON", "ORG", "LOCATION", "EVENT"]  # zero-shot types
coreference = true                  # pronoun-to-entity linking
# backend = "candle"               # default; or "onnx" for ort runtime

[ner.spacy]                         # SpaCy HTTP sidecar (optional)
enabled = false
endpoint = "http://localhost:8080/ner"
timeout_ms = 5000

[ner.llm_fallback]
enabled = false
provider = "ollama"
model = "llama3"
max_calls_per_minute = 10
can_trigger_actions = false         # LLM-extracted facts cannot fire rules
can_supersede = false               # LLM facts cannot overwrite existing facts

# ═══════════════════════════════════════════════════════════════
# SOURCES
# ═══════════════════════════════════════════════════════════════

# Each source is a [[sources]] entry. No source = no scheduled ingest.
# Engram never guesses costs -- budget enforcement via source's usage endpoint only.

[[sources]]
name = "gdelt-news"
type = "news_api"
endpoint = "https://api.gdeltproject.org/api/v2/doc/doc"
max_results = 50
timeout_ms = 10000
schedule = "30m"
min_interval = "5m"                 # adaptive frequency lower bound
max_interval = "12h"                # adaptive frequency upper bound
adaptive_frequency = true
# No auth -- GDELT is free
# No usage -- no usage endpoint

[[sources]]
name = "brave-search"
type = "web_search"
endpoint = "https://api.search.brave.com/res/v1/web/search"
max_results = 10
timeout_ms = 5000
schedule = "30m"
min_interval = "10m"
max_interval = "6h"
adaptive_frequency = true

[sources.auth]
type = "api_key"
header = "X-Subscription-Token"
key_env = "BRAVE_API_KEY"           # reads from environment variable

[sources.usage]
endpoint = "https://api.search.brave.com/res/v1/usage"
auth_env = "BRAVE_API_KEY"
check_before_fetch = true           # query usage endpoint before every fetch
soft_limit_pct = 80                 # emit warning event at 80%
hard_limit_pct = 95                 # stop fetching at 95%

[[sources]]
name = "twitter-osint"
type = "social"
schedule = "15m"
min_interval = "5m"
max_interval = "24h"
adaptive_frequency = true

[sources.auth]
type = "oauth2"
token_env = "TWITTER_BEARER_TOKEN"

[sources.usage]
endpoint = "https://api.twitter.com/2/usage/tweets"
auth_env = "TWITTER_BEARER_TOKEN"
check_before_fetch = true
soft_limit_pct = 80
hard_limit_pct = 95

[[sources]]
name = "recorded-future"
type = "paid_intel"
endpoint = "https://api.recordedfuture.com/v2/..."
timeout_ms = 15000
schedule = "6h"
min_interval = "1h"
max_interval = "24h"

[sources.auth]
type = "bearer"
token_env = "RF_API_KEY"

[sources.usage]
endpoint = "https://api.recordedfuture.com/v2/usage"
auth_env = "RF_API_KEY"
check_before_fetch = true
soft_limit_pct = 70
hard_limit_pct = 90

[[sources]]
name = "custom-rss"
type = "rss"
urls = ["https://feed1.xml", "https://feed2.xml"]
schedule = "10m"
min_interval = "5m"
max_interval = "24h"
# RSS uses ETag natively -- no auth, no usage endpoint

[[sources]]
name = "report-dropzone"
type = "file"
path = "/data/reports/"
watch = true                        # filesystem events via notify crate
poll_fallback = "30s"               # fallback for network drives
formats = ["txt", "md", "pdf", "json", "csv", "html"]
# trust is learned, not configured (Section 5.1)

# ═══════════════════════════════════════════════════════════════
# SEARCH LEDGER
# ═══════════════════════════════════════════════════════════════

[ledger]
storage = "{brain}.ledger"          # sidecar file alongside .brain
compact_on_startup = true           # compact append-only log on startup
subsumption_window = "auto"         # "auto" = match source schedule, or duration

# ═══════════════════════════════════════════════════════════════
# ACTION ENGINE
# ═══════════════════════════════════════════════════════════════

[actions]
enabled = true                      # master switch (default: false)
max_chain_depth = 5                 # rule A -> rule B -> ... max depth
max_effects_per_minute = 30         # global external effect rate limit
dry_run = false                     # evaluate rules, log, don't execute
rules_dir = "rules/"                # directory with rule TOML files

[actions.timer]
stale_check_interval = "6h"         # how often to run timer-based rules
gap_scan_interval = "1h"            # how often to trigger black area scan

# ═══════════════════════════════════════════════════════════════
# ENRICHMENT (query-triggered)
# ═══════════════════════════════════════════════════════════════

[enrichment]
enabled = true                      # master switch (default: false)
default_mode = "eager"              # "eager" (background), "await" (block), "none"
cooldown = "30m"                    # same query within window returns cached
max_concurrent_sources = 5          # parallel fan-out limit

# Tier ordering (mesh always first if available)
tier_order = ["mesh", "free", "paid"]

# ═══════════════════════════════════════════════════════════════
# BLACK AREA DETECTION
# ═══════════════════════════════════════════════════════════════

[reason]
enabled = true                      # master switch (default: false)
scan_interval = "1h"                # how often to run full gap detection
min_severity = 0.3                  # only report gaps above this severity
auto_enrich_above = 0.7             # auto-trigger enrichment for high-severity gaps

[reason.llm_suggestions]
enabled = false                     # LLM-generated investigation queries
provider = "ollama"                 # uses existing LLM endpoint
model = "llama3"
max_suggestions = 10                # per gap
# LLM suggestions are NEVER auto-executed. Frontend-only, user clicks [Run].

# ═══════════════════════════════════════════════════════════════
# MESH (v1.1.0 additions to existing mesh config)
# ═══════════════════════════════════════════════════════════════

[mesh.profiles]
enabled = true                      # auto-derive and broadcast knowledge profile
refresh_interval = "15m"            # recalculate profile from graph clusters
max_domains = 20                    # top-N topic clusters to announce

[mesh.federated_query]
enabled = true                      # allow peers to search this node's graph
max_results = 100                   # per federated query
timeout_ms = 5000                   # per-peer query timeout
respect_acl = true                  # filter results by sensitivity clearance

[mesh.sync]
mode = "selective"                  # "selective" (default) or "full"

[[mesh.sync.subscriptions]]         # optional: sync specific topics from peers
peer = "iran-desk"
topics = ["Iran-Russia", "sanctions"]
min_confidence = 0.7

[mesh.ingest]                       # mesh fast path settings
skip_ner = true                     # always skip NER for mesh-synced facts
skip_conflict_above_trust = 0.80    # skip conflict check if peer trust > 0.80
always_resolve = true               # always resolve against local graph
trust_multiplier = true             # apply peer trust to confidence

# ═══════════════════════════════════════════════════════════════
# STREAMING
# ═══════════════════════════════════════════════════════════════

[streaming]
sse_buffer = 1000                   # max buffered events per SSE connection
ws_max_connections = 50             # WebSocket ingest connections
webhook_secret_env = "ENGRAM_WEBHOOK_SECRET"  # HMAC validation for incoming webhooks
```

---

## Appendix C: Glossary

| Term | Definition |
|------|-----------|
| **Adaptive frequency** | Scheduler adjusts fetch interval based on result yield (slow down on no new results, speed up on many) |
| **Black area** | A gap or blind spot in the knowledge graph where knowledge is missing or insufficient |
| **BYOM** | Bring Your Own Model: user downloads and installs NER model, engram ships none |
| **Confidence** | Float 0.0-1.0 representing certainty of a fact. Based on learned trust (author > source > baseline) and extraction method |
| **Conservative ER** | Entity resolution strategy that never auto-merges borderline matches. Creates `maybe_same_as` edges instead |
| **Corroboration** | Confidence boost when multiple independent sources agree on a fact |
| **Direct store** | Bypassing the ingest pipeline entirely (`POST /store`, `/batch`). For pre-structured data |
| **Dynamic ingest job** | Ingest job created by the action engine from graph intelligence, not user-configured |
| **Enrichment** | Fetching external data to fill knowledge gaps, triggered by queries or rules |
| **Federated query** | Searching across mesh peer graphs without copying facts. Results merged, provenance preserved |
| **Frontier node** | An entity with few connections, at the edge of known knowledge |
| **Gazetteer** | Dictionary of known entities, dynamically generated from the graph |
| **Knowledge profile** | Auto-derived summary of what a mesh node covers (topics, depth, freshness). Broadcast via gossip |
| **Mesh fast path** | Optimized ingest for mesh-synced facts: skip NER, resolve locally, apply peer trust multiplier |
| **NER** | Named Entity Recognition: extracting entities (people, orgs, locations) from text |
| **Pipeline shortcut** | Skipping pipeline stages via `?skip=` params when data is already partially structured |
| **Provenance** | Complete record of where a fact came from, who/what created it, and how |
| **Query subsumption** | Skipping a narrow query when a broader query recently ran on the same source |
| **Search ledger** | Tracks every query-source combination to minimize redundant API calls. Stored in `.brain.ledger` |
| **Spearhead search** | Parallel fan-out to multiple sources simultaneously for enrichment |
| **Structural hole** | A missing connection between entities that should logically be connected |
| **Supersede** | When a higher-confidence fact replaces a lower-confidence contradicting fact |
| **Temporal cursor** | Source-specific marker (date, since_id, ETag) to fetch only new results since last run |
| **Three-tier enrichment** | Mesh peers (free, fast) > external free APIs > external paid APIs. Cost-optimized ordering |
| **Usage endpoint** | Source's own billing/quota API. Engram queries it, never calculates costs itself |

---

*This document is internal to the engram project. Do not distribute outside of Gitea.*
*Repository: `192.168.178.26:3141/admin/engram`*
