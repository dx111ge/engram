# Engram Architecture

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
|  HTTP REST + MCP (stdio) + gRPC + LLM tools       |
|  Natural language queries (/ask, /tell)            |
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
+---------------------------------------------------+
|               Knowledge Graph                       |
|  Typed nodes with properties and provenance        |
|  Directed edges with confidence scores             |
|  Memory tiers (core, active, archival)             |
+---------------------------------------------------+
|              Compute Layer                          |
|  SIMD (AVX2, FMA, NEON) for similarity             |
|  GPU compute (wgpu) for large-scale operations     |
|  NPU routing for low-power inference               |
+---------------------------------------------------+
|               Storage Engine                        |
|  Single .brain file, crash recovery via WAL        |
|  Sidecar files for properties, vectors, types      |
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

- **HTTP REST** -- 25+ endpoints for full graph manipulation, search, learning, and system operations
- **MCP (Model Context Protocol)** -- JSON-RPC over stdio for native AI tool integration (Claude, Cursor, Windsurf)
- **gRPC** -- high-performance RPC for service-to-service communication
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
