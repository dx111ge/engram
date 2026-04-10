# Engram Architecture (v1.1.2)

High-level overview of the Engram system design.

---

## Single Binary, Single File

Engram compiles to a single statically-linked binary with no runtime dependencies. All storage, indexing, search, reasoning, and networking are built in.

All knowledge is stored in a single `.brain` file. Copy the file to back up. Move the file to migrate. Delete the file to start fresh. No database servers, no configuration files, no cloud accounts.

---

## System Overview

```
+---------------------------------------------------+
|                    API Layer                        |
|  HTTP REST + MCP (stdio) + gRPC + A2A + LLM tools |
|  Natural language (/ask, /tell) + SSE streaming    |
+---------------------------------------------------+
|             Application Layer                       |
|  Chat system (47 tools, 8 clusters)               |
|  Multi-agent debate (7 modes, War Room)            |
|  Assessment engine (Bayesian, living assessments)  |
+---------------------------------------------------+
|             Intelligence Layer                      |
|  Ingest pipeline (NER, entity resolution, dedup)   |
|  Document pipeline (PDF, HTML, tables, provenance) |
|  Action engine (event-driven rules + effects)      |
|  Gap detection (domain-based, LLM queries)         |
|  Contradiction detection + conflict resolution     |
|  Temporal facts (valid_from / valid_to)            |
+---------------------------------------------------+
|                 Query Engine                        |
|  BM25 full-text + HNSW vector + bitmap filters    |
|  Boolean operators (AND, OR, NOT)                  |
|  Property, confidence, type, and tier filters      |
+---------------------------------------------------+
|              Reasoning + Learning                   |
|  Forward/backward chaining, rule evaluation        |
|  Confidence lifecycle (reinforce, decay, correct)  |
|  Co-occurrence tracking, contradiction detection   |
|  NER category learning (3-tier self-improving)     |
+---------------------------------------------------+
|               Knowledge Graph                       |
|  Typed nodes with properties and provenance        |
|  Directed edges with confidence + temporal bounds  |
|  Memory tiers (core, active, archival)             |
|  Domain taxonomy with auto-classification          |
+---------------------------------------------------+
|              Compute Layer                          |
|  SIMD (AVX2, FMA, NEON) for similarity             |
|  GPU compute (wgpu) for large-scale operations     |
|  GLiNER2 ONNX (DirectML / CUDA / CoreML)          |
|  NPU routing for low-power inference               |
+---------------------------------------------------+
|               Storage Engine                        |
|  Single .brain file, crash recovery via WAL        |
|  Sidecar files for properties, vectors, types      |
|  Document store (.brain.docs.N + .brain.docs.idx)  |
|  Source registry (.brain.sources)                  |
+---------------------------------------------------+
```

---

## Knowledge Graph

The graph stores **nodes** (entities) and **edges** (relationships).

**Nodes** have:
- A label (unique identifier)
- An optional type (e.g., `person`, `server`, `concept`)
- Arbitrary key-value properties
- A confidence score (0.0 to 1.0)
- Provenance tracking (who created it, when)
- A memory tier (`core`, `active`, or `archival`)

**Edges** are directed relationships between nodes:
- From node -> relationship -> to node
- Confidence score
- Temporal metadata (creation time, event time)

---

## Hybrid Search

Engram combines multiple search strategies:

- **BM25 full-text** -- keyword search with TF-IDF scoring across all node labels and properties
- **HNSW vector similarity** -- semantic search using embeddings from an external API (Ollama, OpenAI, vLLM, or any OpenAI-compatible endpoint)
- **Bitmap filtering** -- fast pre-filtering by type, tier, confidence range, and properties
- **Boolean queries** -- AND, OR, NOT operators for combining search conditions

When both BM25 and vector results are available, they are fused using reciprocal rank fusion for optimal relevance.

---

## Confidence Lifecycle

Every node and edge has a confidence score that evolves over time:

- **Reinforcement** -- confidence increases when knowledge is accessed or confirmed by independent sources
- **Time-based decay** -- unused knowledge gradually loses confidence (configurable rate)
- **Correction** -- marking a fact as wrong zeroes its confidence and propagates distrust to neighbors
- **Contradiction detection** -- conflicting facts are identified through co-occurrence analysis

Memory tiers track knowledge maturity:
- **Core** -- high-confidence, frequently accessed facts
- **Active** -- working knowledge at moderate confidence
- **Archival** -- low-confidence or rarely accessed facts, candidates for cleanup

---

## Inference Engine

Rule-based reasoning over the knowledge graph:

- **Forward chaining** -- apply rules to derive new facts, run to fixed point
- **Backward chaining** -- prove whether a relationship holds by finding evidence chains
- **Rule evaluation** -- pattern matching on edges, properties, and confidence thresholds
- **Derived facts** -- new edges created by rules carry computed confidence (min, product, or literal)

Rules support conditions on edges, properties, and confidence levels. Multiple conditions are AND-ed together.

---

## Application Layer

High-level analytical capabilities built on top of the intelligence and query layers.

### Chat System

Natural language interface with 47 tools organized in 8 clusters:

- **Analysis** -- entity comparison, shortest path, LLM-augmented analysis, black area detection
- **Investigation** -- network analysis, entity 360-degree view, entity gap analysis
- **Reporting** -- briefings, dossiers, topic maps, graph statistics, export
- **Temporal** -- provenance chains, contradiction timelines, situation snapshots
- **Assessment** -- structured evidence, Bayesian confidence, watch management
- **Action** -- rule creation, rule firing, scheduled triggers
- **Reasoning** -- what-if analysis, multi-path influence cascades
- **General** -- store, relate, search, query, explain

Intent routing dispatches user messages to the appropriate tool cluster. Tool results are rendered as structured cards in the UI.

### Multi-Agent Debate Engine

Structured multi-agent analysis with 7 modes:

- **Analyze** -- balanced multi-perspective analysis
- **Red Team** -- adversarial challenge of assumptions
- **Devil's Advocate** -- systematic counter-argumentation
- **Scenario Planning** -- future scenario exploration
- **Delphi** -- expert consensus building
- **Structured Analytic Techniques** -- intelligence community methods
- **War Game** -- competitive strategy simulation

Each debate runs multiple AI agents with configurable bias profiles. A moderator agent orchestrates rounds, and a 3-layer synthesis pipeline produces per-round summaries, cross-round analysis, and a final synthesis.

The **War Room** provides a live dashboard with SSE-driven agent cards (gauges, sparklines), activity feed, round summaries, and session recovery.

### Assessment Engine

Bayesian confidence calculation with structured evidence:

- **Living assessments** -- automatically detect when underlying evidence changes
- **Stale alerts** -- flag assessments whose supporting facts have been corrected or decayed
- **Dynamic watches** -- LLM-suggested monitoring triggers
- **Evidence board** -- visual evidence chain from facts to conclusions

---

## Ingest Pipeline

Automated knowledge acquisition from external sources:

- **NER processing** -- named entity recognition with language detection and per-language patterns
- **Entity resolution** -- conservative 4-step progressive matching against existing graph entities
- **Conflict detection** -- identifies contradictions between new and existing knowledge
- **Content deduplication** -- hash-based and semantic deduplication prevents duplicate facts
- **Source management** -- health monitoring, usage tracking, and adaptive scheduling
- **Multi-format input** -- text, structured data, webhooks, and WebSocket streams

---

## Document Pipeline

End-to-end document processing from ingestion to knowledge graph:

- **PDF extraction** -- text and table extraction via pdf-extract crate
- **HTML processing** -- table-aware extraction with structure preservation
- **Translation** -- automatic translation during ingest for multi-language sources
- **Provenance chain** -- Entity -> Fact -> Document -> Publisher tracking
- **Source CRUD** -- create, read, update, delete data sources with health monitoring
- **Folder watch** -- automatic ingestion of files added to watched directories
- **Reprocess** -- re-fetch and re-analyze existing documents with updated pipeline
- **Document store** -- dedicated storage (`.brain.docs.N` + `.brain.docs.idx`) separate from the graph

Documents and facts are orthogonal entities: a document may produce multiple facts, and facts can exist independently of documents.

---

## Temporal Facts & Contradiction Detection

### Temporal Facts

Edges support `valid_from` and `valid_to` timestamps, enabling time-bounded knowledge:

- **3-layer extraction**: LLM prompt extension asks for dates, a quick NER/RE pass identifies temporal patterns, and users can set dates manually
- **Temporal succession**: when a fact replaces another (e.g., a new capital), the system creates a succession relationship rather than flagging a conflict

### Contradiction Detection

Automatic identification of conflicting knowledge:

- **ConflictDetector** runs during ingest and on-demand scans
- **Singular properties** (e.g., "capital_of" can have only one current value) are user-configurable
- **Conflict edges** (`conflicts_with`) link contradictory facts with metadata
- **Resolution workflow** -- supersede, contest, or dispute through the API or UI

---

## Action Engine

Event-driven automation triggered by graph changes:

- **Rule definitions** -- TOML-based rules with pattern matching on graph events
- **Internal effects** -- confidence cascade, edge creation, tier changes
- **External effects** -- webhook calls, API notifications
- **Safety constraints** -- cooldown periods, chain depth limits, effect budgets
- **Scheduled triggers** -- timer-based rules for periodic operations
- **Dry run mode** -- test rules against events without executing effects

---

## Reasoning Layer

Proactive knowledge gap analysis and investigation:

- **Gap detection** -- identifies frontier nodes, structural holes, temporal gaps, and confidence deserts
- **Severity scoring** -- ranks gaps by impact on graph completeness
- **Query generation** -- suggests search queries based on graph topology
- **Investigation suggestions** -- optional LLM-powered analysis recommendations
- **Mesh federation** -- query knowledge across peer instances without copying facts
- **Enrichment dispatch** -- 3-tier strategy (mesh > free sources > paid sources)

---

## Knowledge Mesh

Peer-to-peer synchronization between engram instances:

- **Identity** -- each instance has an ed25519 keypair for authentication
- **Peer registry** -- explicit mutual peering with trust scores
- **Delta sync** -- only changed facts are exchanged, using bloom filter digests
- **Conflict resolution** -- deterministic merge strategy for concurrent modifications
- **Access control** -- topic-level ACLs with sensitivity labels (`public`, `internal`, `confidential`, `restricted`)
- **Audit trail** -- all sync operations are logged for accountability

---

## APIs

Engram exposes multiple interfaces:

- **HTTP REST** -- 40+ endpoints for graph manipulation, search, learning, intelligence, and system operations
- **MCP (Model Context Protocol)** -- JSON-RPC over stdio with 12 tools for native AI integration (Claude, Cursor, Windsurf)
- **A2A (Agent-to-Agent)** -- Google's agent protocol with skill routing and streaming task support
- **gRPC** -- high-performance RPC with streaming services for events, enrichment, and bulk ingest
- **SSE streaming** -- real-time graph event stream and enrichment progress
- **LLM tool-calling** -- OpenAI-compatible tool definitions at `/tools`
- **Natural language** -- `/ask` for queries and `/tell` for assertions, powered by the embedding model

---

## Compute Acceleration

- **SIMD** -- AVX2 and FMA instructions for vectorized similarity computation on CPU
- **GPU** -- wgpu compute shaders for mass similarity search and large-scale graph operations
- **NPU** -- automatic routing to low-power neural processing units when available
- **Compute planner** -- automatically selects the optimal backend based on workload size and available hardware

---

## Storage

The `.brain` file is a custom binary format optimized for graph access patterns:

- **Crash recovery** -- write-ahead log (WAL) ensures ACID durability
- **Concurrent access** -- reader-writer locks allow multiple concurrent readers with exclusive writers
- **Deferred checkpoint** -- writes are immediately crash-safe via WAL, with periodic background flush for performance
- **Sidecar files** -- properties (`.brain.props`), type registry (`.brain.types`), vectors (`.brain.vectors`), and co-occurrence data (`.brain.cooccur`) are stored alongside the main file
