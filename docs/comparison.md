# Engram vs. the Field: A Technical Comparison

**Version**: engram v0.1.0 (pre-production)
**Date**: 2026-03-07
**Status**: Honest assessment — engram is early-stage software. This document does not overstate its capabilities.

---

## Table of Contents

1. [Summary Table](#1-summary-table)
2. [What Engram Is](#2-what-engram-is)
3. [Comparison Dimensions Explained](#3-comparison-dimensions-explained)
4. [Detailed Competitor Profiles](#4-detailed-competitor-profiles)
   - 4.1 [Neo4j](#41-neo4j)
   - 4.2 [Redis (with RedisGraph / FalkorDB)](#42-redis-with-redisgraph--falkordb)
   - 4.3 [Pinecone](#43-pinecone)
   - 4.4 [Qdrant](#44-qdrant)
   - 4.5 [Weaviate](#45-weaviate)
   - 4.6 [SQLite](#46-sqlite)
   - 4.7 [Apache Jena / RDF Stores](#47-apache-jena--rdf-stores)
   - 4.8 [Memgraph](#48-memgraph)
5. [Feature Matrix](#5-feature-matrix)
6. [Capacity and Scale](#6-capacity-and-scale)
7. [Training and Import](#7-training-and-import)
8. [Storage and Deployment Model](#8-storage-and-deployment-model)
9. [Search Capabilities Deep Dive](#9-search-capabilities-deep-dive)
10. [Learning and Memory Management](#10-learning-and-memory-management)
11. [AI and LLM Integration](#11-ai-and-llm-integration)
12. [Hardware Acceleration](#12-hardware-acceleration)
13. [Honest Limitations of Engram](#13-honest-limitations-of-engram)
14. [When to Use Engram](#14-when-to-use-engram)

---

## 1. Summary Table

The table below gives a quick orientation. Detailed breakdowns follow in each section.

| Dimension | Engram | Neo4j | Redis | Pinecone | Qdrant | Weaviate | SQLite | Apache Jena | Memgraph |
|---|---|---|---|---|---|---|---|---|---|
| **Version / maturity** | 0.1.0, pre-prod | 5.x, production | 7.x, production | Managed, production | 1.x, production | 1.x, production | 3.x, production | 4.x, production | 2.x, production |
| **Language** | Rust | Java | C | Proprietary (cloud) | Rust | Go | C | Java | C++ |
| **License** | AGPL-3.0 | GPL/commercial | BSD-3 | Proprietary SaaS | Apache 2.0 | BSD-3 | Public domain | Apache 2.0 | BSL 1.1 |
| **Architecture** | Embedded, single-process | Client-server | Client-server | Cloud SaaS | Client-server | Client-server | Embedded | Client-server | Client-server |
| **Storage** | Single `.brain` file + sidecar WAL | Directory tree | In-memory + AOF/RDB | Cloud-managed | Directory | Directory | Single `.db` file | TDB2 directory | In-memory + WAL |
| **Install / binary size** | ~2.1 MB (release, stripped) | ~500 MB (JRE + libs) | ~5 MB | N/A (SaaS) | ~20 MB | ~80 MB | ~1 MB | ~150 MB (JAR + deps) | ~30 MB |
| **Graph model** | Property graph | Property graph (Cypher) | Property graph (Cypher-like) | None | None | Object/class graph | None (relational) | RDF triple store | Property graph (Cypher) |
| **Full-text search** | BM25, boolean AND/OR | Lucene-backed | RediSearch module | No | Yes | Yes (BM25) | Lucene (via Fuseki) | SPARQL + text plugin | Limited |
| **Vector / semantic search** | HNSW (pure Rust) | Optional (vector index) | RediSearch module | Core feature | Core feature | Core feature | No | No | No |
| **Hybrid search (BM25 + vector)** | Yes (RRF fusion) | Partial | Partial | No | Yes | Yes | No | No | No |
| **Confidence scoring** | Yes (per-node, per-edge) | No | No | No | No | No (certainty is distance) | No | No | No |
| **Knowledge decay** | Yes (0.999/day exponential) | No | TTL only | No | No | No | No | No | No |
| **Inference engine** | Forward-chaining rules | No (application layer) | No | No | No | No | No | RDFS/OWL inference | No |
| **Memory tiers** | 3 tiers (core/active/archival) | No | No | No | No | No | No | No | No |
| **Provenance tracking** | Yes (source type + source ID) | Manual | No | No | No | No | No | Quad stores (named graphs) | No |
| **MCP server** | Yes (JSON-RPC stdio) | No | No | No | No | No | No | No | No |
| **A2A protocol** | Yes (agent-to-agent) | No | No | No | No | No | No | No | No |
| **REST API** | Yes (axum) | Yes (Bolt + REST) | Yes | Yes | Yes | Yes | No (library) | Yes (SPARQL endpoint) | Yes |
| **gRPC** | No (planned; not yet implemented) | No | No | Yes | Yes | Yes | No | No | Yes |
| **Clustering / replication** | No (single-node only) | Yes (Causal clustering) | Yes (Sentinel/Cluster) | Yes (cloud) | Yes | Yes | No | No | Yes |
| **ACID transactions** | No | Yes | Partial | N/A | No | No | Yes | No (SPARQL Update) | Yes |
| **Query language** | Custom DSL + natural language | Cypher | RESP / Cypher-like | gRPC filter DSL | Filter DSL | GraphQL | SQL | SPARQL | Cypher |
| **CPU SIMD** | Yes (AVX2+FMA, NEON) | JVM JIT | No explicit | N/A | Yes (AVX2) | No | No | No | No |
| **GPU acceleration** | Yes (wgpu: DX12/Vulkan/Metal) | No | No | N/A (cloud) | Partial | No | No | No | No |
| **Cross-platform** | Windows, macOS, Linux (x64 + arm64) | Windows, macOS, Linux | Windows, macOS, Linux | N/A | Windows, macOS, Linux | Windows, macOS, Linux | Windows, macOS, Linux | Windows, macOS, Linux | Linux primary |
| **Offline / air-gapped** | Yes | Yes | Yes | No | Yes | Yes | Yes | Yes | Yes |

---

## 2. What Engram Is

Engram is an **embedded AI memory engine** — a self-contained knowledge graph designed to give AI agents, LLM-backed applications, and local tools a structured, persistent, queryable memory. It stores everything in a single `.brain` file using a memory-mapped fixed-record layout with a write-ahead log for crash recovery.

Key design goals, directly reflected in the implementation:

- **Zero-dependency deployment**: one binary, one file. No JVM, no Python runtime, no separate server process, no network round-trips.
- **Learning, not just storage**: facts are not static. Confidence scores decay over time (0.999 per day), can be reinforced by repeated observation, and can be corrected when contradictions are detected.
- **Native AI integration**: the MCP server (JSON-RPC over stdio) and A2A protocol make engram a first-class citizen in LLM tool ecosystems without any adapter layer.
- **Tiered memory that maps to LLM context**: Core (tier 0) facts are always surfaced; Active (tier 1) facts are accessible; Archival (tier 2) facts are search-only. This mirrors how LLM context windows work.

Engram is an AI memory engine -- a knowledge graph with built-in learning, confidence scoring, and hardware-accelerated compute in a single 2.1 MB binary. It does not aim to replace enterprise graph databases or cloud-managed vector stores. Its value is the combination of features that no single competing product offers: graph structure + learning lifecycle + zero-pipeline import + embedded deployment + mesh federation. The products below each excel at one dimension; engram integrates all of them.

---

## 3. Comparison Dimensions Explained

Before the detailed sections, this glossary defines how each dimension is evaluated.

**Architecture (embedded vs. client-server vs. cloud)**
Embedded means the database runs in the same process (or at least on the same host without a network socket) as the application. Client-server means a separate daemon that clients connect to. Cloud means the data lives on a third-party managed service.

**Single-file storage**
Whether the entire database state (schema, data, indexes) fits in one file that can be copied, backed up, or sent over the wire atomically. Engram's `.brain` file plus its `.brain.wal` sidecar and optional `.brain.vectors` sidecar count as effectively single-file for backup purposes — all files share the same stem.

**Property graph vs. RDF**
A property graph model (Neo4j, engram) attaches arbitrary key-value properties to both nodes and edges. RDF (Apache Jena) uses subject-predicate-object triples with no native property-on-edge support. Engram's node structure carries inline label storage, typed properties, confidence, tier, sensitivity, provenance, timestamps, and embedding pointer — all in a 256-byte fixed record.

**Confidence scoring**
A numeric belief value (0.0–1.0) per fact, updated by the system based on reinforcement, correction, and temporal decay. No competitor in this list implements this natively.

**Inference engine**
Forward-chaining rule execution: given a set of `WHEN <pattern> THEN <action>` rules, the engine fires rules when graph patterns match and derives new edges. This is different from RDFS/OWL inference (which reasons over class hierarchies) and from application-layer Cypher queries.

---

## 4. Detailed Competitor Profiles

### 4.1 Neo4j

**Neo4j** is the market-leading property graph database. It uses the Cypher query language, runs as a client-server daemon backed by a Java process, and requires a JVM. The Community Edition is GPL-licensed; production clustering requires the commercial Enterprise Edition.

| Attribute | Neo4j | Engram |
|---|---|---|
| License | GPL (Community) / Commercial (Enterprise) | AGPL-3.0 |
| Runtime | JVM (Java 17+) | Native Rust binary |
| Install footprint | ~500 MB (JRE + Neo4j libs + conf) | ~2.1 MB |
| Startup time | 5–30 seconds (JVM warm-up) | Sub-100ms |
| Storage | Directory tree (data/, logs/, conf/) | Single `.brain` file |
| Query language | Cypher | Custom DSL + natural language |
| Full-text search | Lucene-backed full-text indexes | BM25 (built-in) |
| Vector search | Available (Neo4j 5.11+, HNSW) | HNSW (pure Rust) |
| Transactions | ACID | None |
| Clustering | Causal cluster (Enterprise) | None (single-node) |
| Confidence scoring | No | Yes |
| Knowledge decay | No | Yes |
| MCP / A2A | No | Yes |

**Where Neo4j wins**: Cypher is a mature, expressive query language with decades of tooling. Neo4j handles billion-node graphs, has enterprise clustering, ACID guarantees, role-based access control, and a large ecosystem of connectors. It is the correct choice for production graph analytics at scale.

**Where engram is different**: Neo4j has no concept of a fact becoming less certain over time. A relationship stored in Neo4j in 2020 carries the same weight as one stored yesterday unless the application explicitly manages this. Engram's decay model (`confidence *= 0.999 ^ days_since_last_access`) makes stale knowledge automatically less influential without application-level intervention. For an AI memory use case where recency matters, this is architecturally relevant.

**Engram's honest weaknesses vs. Neo4j**: No Cypher, no ACID transactions, no clustering, no enterprise security model, no change data capture, no billion-node scale. For anything beyond a single-machine AI agent workload, Neo4j is the appropriate choice.

---

### 4.2 Redis (with RedisGraph / FalkorDB)

**Redis** is an in-memory key-value store with optional modules. RedisGraph (now maintained as FalkorDB after Redis changed its licensing) added a property graph layer using sparse adjacency matrices and a Cypher-compatible query language. RediSearch adds full-text and vector search.

| Attribute | Redis + Modules | Engram |
|---|---|---|
| License | RSAL / SSPL (Redis 7.4+); FalkorDB: MIT | AGPL-3.0 |
| Runtime | C daemon + module .so files | Single Rust binary |
| Install footprint | ~5 MB (redis-server) + modules | ~2.1 MB |
| Storage | In-memory primary; AOF/RDB for persistence | mmap'd file (durable by design) |
| Memory model | All data in RAM | Memory-mapped file (OS manages pages) |
| Graph model | Property graph (FalkorDB) | Property graph |
| Full-text search | RediSearch module | Built-in BM25 |
| Vector search | RediSearch module | Built-in HNSW |
| Confidence scoring | No | Yes |
| Knowledge decay | TTL per key (coarse) | Per-node exponential decay |
| MCP / A2A | No | Yes |

**Where Redis wins**: Redis is extraordinarily fast for simple key-value operations (sub-millisecond at high throughput). Its pub/sub and stream primitives make it the backbone of many real-time systems. At scale with RediSearch, it handles billions of documents.

**Where engram is different**: Redis's persistence story is secondary — it is an in-memory store that can flush to disk, not a disk-primary store that maps pages into memory. For an AI agent that must survive restarts without data loss and without careful AOF/RDB configuration, engram's append-only WAL and mmap'd storage provide durability by default. Redis TTL is binary (key exists or it doesn't); engram's decay is continuous and probabilistic.

**Engram's honest weaknesses vs. Redis**: No pub/sub, no streams, no atomic multi-key transactions, no Redis Cluster scale-out. Redis with RediSearch has a vastly larger operational community. FalkorDB's Cypher support is more expressive than engram's query DSL.

---

### 4.3 Pinecone

**Pinecone** is a managed vector database. There is no on-premises deployment option; all data lives in Pinecone's cloud infrastructure.

| Attribute | Pinecone | Engram |
|---|---|---|
| License | Proprietary SaaS | AGPL-3.0 |
| Deployment | Cloud-only | Local / on-premises / embedded |
| Storage | Cloud-managed | Single `.brain` file |
| Data residency | Pinecone's servers | Your hardware |
| Internet required | Yes | No |
| Graph model | None (flat vector namespace) | Property graph |
| Full-text search | No | Yes (BM25) |
| Vector search | Core feature (ANN) | HNSW |
| Hybrid search | Sparse + dense (Pinecone Sparse) | BM25 + HNSW (RRF) |
| Confidence scoring | No | Yes |
| Knowledge decay | No | Yes |
| MCP / A2A | No | Yes |
| Pricing | Usage-based, can be significant at scale | Free (AGPL) |

**Where Pinecone wins**: Pinecone's ANN infrastructure scales to hundreds of millions of vectors with managed replication, zero operational burden, and low-latency retrieval across large corpora. It is the right choice when vector similarity is the primary retrieval mechanism and operational simplicity outweighs data sovereignty concerns.

**Where engram is different**: Pinecone cannot model relationships between entities. It stores vectors in namespaces with metadata filters, not a graph. It has no concept of one fact being derived from another, no confidence, no decay. It requires an internet connection — it cannot run air-gapped. For an AI agent running locally (on a developer laptop, on an edge device, in a secure facility), Pinecone is architecturally incompatible.

**Engram's honest weaknesses vs. Pinecone**: Engram's vector index (HNSW in pure Rust) is limited to what fits in host memory. There is no sharding, no managed replication, and no SLA. For large-scale retrieval over millions of vectors, Pinecone's infrastructure is far more capable.

---

### 4.4 Qdrant

**Qdrant** is an open-source vector search engine written in Rust. It runs as a client-server daemon, stores collections as directory trees, and focuses on approximate nearest neighbor search with rich filtering.

| Attribute | Qdrant | Engram |
|---|---|---|
| License | Apache 2.0 | AGPL-3.0 |
| Language | Rust | Rust |
| Architecture | Client-server | Embedded |
| Install footprint | ~20 MB | ~2.1 MB |
| Storage | Directory per collection | Single `.brain` file |
| Graph model | None (flat vector collections) | Property graph |
| Full-text search | Yes (built-in) | Yes (BM25) |
| Vector search | Core feature (HNSW) | HNSW |
| Hybrid search | Yes (sparse + dense) | Yes (BM25 + HNSW, RRF) |
| Quantization | Yes (scalar, product, binary) | No |
| Clustering / sharding | Yes | No |
| Confidence scoring | No (distance scores only) | Yes |
| Knowledge decay | No | Yes |
| MCP / A2A | No | Yes |
| REST + gRPC | Yes (full protobuf) | REST yes, gRPC not yet implemented |

**Where Qdrant wins**: Qdrant is a production-grade vector store with quantization (scalar, product, binary), multi-vector support, named vectors, payload filtering, and horizontal sharding. It is the appropriate choice for teams that need vector search as a primary workload at scale with proper gRPC binary protocol support.

**Where engram is different**: Qdrant has no graph model. Relationships between entities must be encoded in payload metadata and managed by the application. There is no inference engine, no confidence decay, and no native LLM protocol (MCP/A2A). Engram and Qdrant are closest in technical heritage (both Rust) but serve different primary use cases.

**Engram's honest weaknesses vs. Qdrant**: No quantization means engram's vector memory consumption is higher per vector. No sharding. Qdrant has full protobuf gRPC; engram has no gRPC implementation yet. Qdrant has a significantly larger user base and production track record.

---

### 4.5 Weaviate

**Weaviate** is an open-source vector database with a strong hybrid search story and a schema/class-based data model. Written in Go, it runs as a client-server daemon and exposes a GraphQL API alongside REST and gRPC.

| Attribute | Weaviate | Engram |
|---|---|---|
| License | BSD-3 | AGPL-3.0 |
| Language | Go | Rust |
| Architecture | Client-server | Embedded |
| Install footprint | ~80 MB | ~2.1 MB |
| Storage | Directory (segment files) | Single `.brain` file |
| Graph model | Object/class model (not a true property graph) | Property graph |
| Full-text search | BM25 (built-in) | BM25 (built-in) |
| Vector search | HNSW | HNSW |
| Hybrid search | Yes (BM25 + vector, RRF) | Yes (BM25 + HNSW, RRF) |
| Multi-tenancy | Yes | No |
| Clustering | Yes | No |
| Confidence scoring | No | Yes |
| Knowledge decay | No | Yes |
| MCP / A2A | No | Yes |
| LLM module integrations | Many (OpenAI, Cohere, etc.) | Embedder interface (bring your own) |

**Where Weaviate wins**: Weaviate has a mature hybrid search pipeline, multi-tenancy, clustering, and a rich ecosystem of vectorizer modules that integrate directly with major embedding providers. Its object model and GraphQL API make it accessible to developers familiar with REST/GraphQL patterns. It is well-suited for RAG (retrieval-augmented generation) infrastructure at scale.

**Where engram is different**: Weaviate's object model is closer to a document store with vectors than a true property graph. There are no typed, directed edges with confidence and provenance in Weaviate's schema. The LLM integration in Weaviate is retrieval-only: you get back documents. Engram's MCP server makes it a tool that an LLM can call to both read and write structured knowledge, and A2A support enables agent-to-agent knowledge sharing at the protocol level.

**Engram's honest weaknesses vs. Weaviate**: No multi-tenancy, no clustering, significantly smaller community. Weaviate's vectorizer ecosystem (OpenAI, Cohere, Hugging Face, etc.) is far richer than engram's current embedder interface. Weaviate is production-ready; engram is not.

---

### 4.6 SQLite

**SQLite** is an embedded relational database written in C, with a public domain license. It is one of the most widely deployed pieces of software in the world, present in virtually every smartphone, browser, and operating system.

| Attribute | SQLite | Engram |
|---|---|---|
| License | Public domain | AGPL-3.0 |
| Language | C | Rust |
| Architecture | Embedded (library) | Embedded (process or library) |
| Install footprint | ~1 MB | ~2.1 MB |
| Storage | Single `.db` file | Single `.brain` file |
| Data model | Relational (tables, rows, columns) | Property graph (nodes, edges, properties) |
| Query language | SQL | Custom DSL + natural language |
| Full-text search | FTS5 extension | BM25 (built-in) |
| Vector search | sqlite-vec (third-party extension) | Built-in HNSW |
| Graph traversal | Recursive CTEs (awkward) | Native BFS/DFS traversal |
| ACID transactions | Yes | No |
| Confidence scoring | No (application must manage) | Yes |
| Knowledge decay | No (application must manage) | Yes |
| Inference engine | No | Yes (forward-chaining rules) |
| MCP / A2A | No | Yes |

**Where SQLite wins**: SQLite is the most battle-tested embedded database on earth. It has ACID transactions, a full SQL engine, triggers, views, recursive CTEs, foreign keys, and decades of correctness testing under adversarial conditions. It is the correct default for any structured data storage need.

**Where engram is different**: Representing a knowledge graph in SQLite requires a `nodes` table, an `edges` table, a `properties` table, and then recursive CTEs or application-layer BFS to traverse relationships. This is functional but loses the semantic richness of a typed graph. More importantly, SQLite has no concept of epistemics: it cannot express that a fact has a 0.73 confidence score derived from an LLM source, that it was last accessed 47 days ago, and that it should therefore be decayed before being surfaced to an AI agent. Building that system on top of SQLite is exactly what engram replaces.

**Engram's honest weaknesses vs. SQLite**: No ACID transactions means engram relies on its WAL and mmap flush for durability, which is not equivalent to SQLite's transaction semantics. SQLite has forty years of correctness testing; engram is v0.1.0. For any use case that does not require graph semantics, confidence, or decay, SQLite with FTS5 is the more conservative and reliable choice.

---

### 4.7 Apache Jena / RDF Stores

**Apache Jena** is a Java framework for building semantic web applications. It includes TDB2 (a native triple store), an in-memory store, a SPARQL query engine, RDFS/OWL inference, and a SPARQL HTTP endpoint (Apache Fuseki). Other RDF stores in this category include Virtuoso, GraphDB, and Stardog.

| Attribute | Apache Jena / TDB2 | Engram |
|---|---|---|
| License | Apache 2.0 | AGPL-3.0 |
| Language | Java | Rust |
| Architecture | Client-server (Fuseki) or embedded (TDB2) | Embedded |
| Install footprint | ~150 MB (JRE + Jena JARs) | ~2.1 MB |
| Storage | TDB2 directory (segment files) | Single `.brain` file |
| Data model | RDF triples / quads | Property graph |
| Query language | SPARQL | Custom DSL + natural language |
| Ontology / reasoning | RDFS, OWL (rule-based and DL subset) | Forward-chaining rules (custom) |
| Full-text search | Lucene plugin (Jena Text) | Built-in BM25 |
| Vector search | No | Built-in HNSW |
| Confidence scoring | No (reification for metadata, awkward) | Yes |
| Knowledge decay | No | Yes |
| MCP / A2A | No | Yes |
| Standards compliance | W3C RDF, SPARQL, OWL | None (proprietary) |

**Where Apache Jena wins**: RDF and SPARQL are W3C standards. Knowledge represented in RDF is interoperable — it can be exported as Turtle, JSON-LD, or N-Quads and loaded into any compliant triple store. OWL ontology reasoning provides formal, provably correct inference over class hierarchies and property chains. For semantic web applications, linked data publishing, and interoperability with external ontologies (DBpedia, Wikidata, schema.org), RDF stores are the correct tool.

**Where engram is different**: RDF's property-on-edge limitation (reification is verbose and awkward) and SPARQL's steep learning curve are well-known friction points. Engram's property graph model is more natural for most developers. The inference engine in engram is not OWL-complete — it is a practical forward-chaining system where a human writes `WHEN edge(A, "is_a", B) AND edge(B, "is_a", C) THEN edge(A, "is_a", C)` and the engine fires it. This is less powerful than OWL reasoning but more predictable and controllable for AI agent use cases where rule explosion is a risk.

**Engram's honest weaknesses vs. Jena**: No standards compliance means engram data is not interoperable with the semantic web ecosystem. SPARQL is far more expressive than engram's query DSL. OWL reasoning handles more complex inference patterns than engram's forward-chaining engine. The JVM overhead of Jena is real, but Jena's correctness and ecosystem maturity are not.

---

### 4.8 Memgraph

**Memgraph** is an in-memory graph database written in C++, focused on real-time streaming graph analytics. It supports Cypher, has a MAGE (Memgraph Advanced Graph Extensions) library for graph algorithms, and includes a Kafka/Pulsar integration for streaming ingestion.

| Attribute | Memgraph | Engram |
|---|---|---|
| License | BSL 1.1 (source-available, not OSI open source) | AGPL-3.0 |
| Language | C++ | Rust |
| Architecture | Client-server | Embedded |
| Install footprint | ~30 MB | ~2.1 MB |
| Storage | In-memory primary + WAL on disk | mmap'd file (disk-primary) |
| Data model | Property graph | Property graph |
| Query language | Cypher (openCypher compatible) | Custom DSL + natural language |
| Graph algorithms | MAGE library (PageRank, community detection, etc.) | BFS/DFS traversal only |
| Full-text search | Limited (via text index) | BM25 (built-in) |
| Vector search | No | Built-in HNSW |
| Streaming ingestion | Yes (Kafka, Pulsar) | No |
| Confidence scoring | No | Yes |
| Knowledge decay | No | Yes |
| Clustering | Yes | No |
| MCP / A2A | No | Yes |

**Where Memgraph wins**: Memgraph is designed for real-time streaming graph analytics — fraud detection, network topology analysis, recommendation engines that need sub-millisecond Cypher query response times over continuously ingested data. Its MAGE library provides graph algorithms (PageRank, Louvain community detection, betweenness centrality) that engram does not have. Cypher on Memgraph is a mature, standards-compatible query language.

**Where engram is different**: Memgraph's BSL 1.1 license restricts competitive use. It has no vector search, no hybrid search, no confidence model, and no native LLM integration protocol. Its in-memory architecture means the working set must fit in RAM, and while it has WAL-based persistence, the cost model is memory-bound in a way that engram's mmap approach is not. For the AI agent memory use case, Memgraph offers no advantage over engram, while requiring network connectivity, a separate server process, and higher memory consumption.

**Engram's honest weaknesses vs. Memgraph**: No Cypher, no streaming ingestion, no graph algorithms library, no clustering. If your use case involves complex graph analytics over streaming data, Memgraph is the appropriate tool.

---

## 5. Feature Matrix

This matrix covers all eight competitors across the thirteen comparison dimensions defined at the start. A filled cell indicates native/built-in support. A partial indicator means the feature is present but incomplete, external, or requires additional modules.

### 5.1 Core Architecture and Storage

| | Engram | Neo4j | Redis | Pinecone | Qdrant | Weaviate | SQLite | Apache Jena | Memgraph |
|---|---|---|---|---|---|---|---|---|---|
| Embedded (no separate process) | Yes | No | No | No | No | No | Yes | Partial | No |
| Single-file storage | Yes | No | No | N/A | No | No | Yes | No | No |
| Air-gap / offline operation | Yes | Yes | Yes | **No** | Yes | Yes | Yes | Yes | Yes |
| Cross-platform (Win/Mac/Linux) | Yes | Yes | Yes | N/A | Yes | Yes | Yes | Yes | Linux primary |
| x86\_64 + aarch64 | Yes | Yes | Yes | N/A | Yes | Yes | Yes | Yes | x86\_64 primary |

### 5.2 Graph Model

| | Engram | Neo4j | Redis | Pinecone | Qdrant | Weaviate | SQLite | Apache Jena | Memgraph |
|---|---|---|---|---|---|---|---|---|---|
| Property graph (typed nodes + edges) | Yes | Yes | Partial | No | No | Partial | No | No | Yes |
| RDF triple store | No | No | No | No | No | No | No | Yes | No |
| Bi-temporal (event time + ingest time) | Yes | No | No | No | No | No | No | Partial | No |
| Provenance tracking | Yes | No | No | No | No | No | No | Partial (quads) | No |
| Sensitivity labels | Yes | No | No | No | No | No | No | No | No |

### 5.3 Search

| | Engram | Neo4j | Redis | Pinecone | Qdrant | Weaviate | SQLite | Apache Jena | Memgraph |
|---|---|---|---|---|---|---|---|---|---|
| Full-text (BM25) | Yes | Lucene | RediSearch | No | Yes | Yes | FTS5 | Lucene plugin | Partial |
| Boolean AND/OR queries | Yes | Cypher | RediSearch | No | Payload filter | GraphQL | SQL | SPARQL | Cypher |
| Property filters | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Temporal range queries | Yes | Yes | No | No | No | No | Yes | Partial | Yes |
| Confidence filters | Yes | No | No | No | No | No | No | No | No |
| Tier filters | Yes | No | No | No | No | No | No | No | No |
| Vector / ANN search (HNSW) | Yes | Yes (5.11+) | RediSearch | Yes | Yes | Yes | Via extension | No | No |
| Hybrid search (keyword + vector) | Yes | Partial | Partial | Partial | Yes | Yes | No | No | No |

### 5.4 Learning and Memory

| | Engram | Neo4j | Redis | Pinecone | Qdrant | Weaviate | SQLite | Apache Jena | Memgraph |
|---|---|---|---|---|---|---|---|---|---|
| Confidence scoring (per fact) | Yes | No | No | No | No | No | No | No | No |
| Source-based confidence caps | Yes | No | No | No | No | No | No | No | No |
| Reinforcement (access raises confidence) | Yes | No | No | No | No | No | No | No | No |
| Correction (contradiction detection) | Yes | No | No | No | No | No | No | No | No |
| Temporal decay (0.999/day) | Yes | No | TTL only | No | No | No | No | No | No |
| Memory tiers (core/active/archival) | Yes | No | No | No | No | No | No | No | No |
| Co-occurrence tracking | Yes | No | No | No | No | No | No | No | No |
| Inference rules (forward-chaining) | Yes | No | No | No | No | No | No | RDFS/OWL | No |

### 5.5 API and Integration

| | Engram | Neo4j | Redis | Pinecone | Qdrant | Weaviate | SQLite | Apache Jena | Memgraph |
|---|---|---|---|---|---|---|---|---|---|
| HTTP REST API | Yes | Yes | Yes | Yes | Yes | Yes | No | Yes (Fuseki) | Yes |
| gRPC (native protobuf) | No (planned) | No | No | Yes | Yes | Yes | No | No | Yes |
| MCP server (JSON-RPC stdio) | Yes | No | No | No | No | No | No | No | No |
| A2A protocol | Yes | No | No | No | No | No | No | No | No |
| Natural language interface | Yes (rule-based, ~15 patterns) | No | No | No | No | No | No | No | No |
| Tool manifest endpoint (`/tools`) | Yes | No | No | No | No | No | No | No | No |
| Language drivers / SDKs | None yet | Java, Python, JS, Go, .NET | Many | Python, JS, Go | Python, JS, Rust, Go | Python, JS, Go | Many | Java | Python, Go, Rust |

### 5.6 Hardware Acceleration

| | Engram | Neo4j | Redis | Pinecone | Qdrant | Weaviate | SQLite | Apache Jena | Memgraph |
|---|---|---|---|---|---|---|---|---|---|
| CPU SIMD (AVX2+FMA) | Yes | JVM JIT | No | N/A | Yes | No | No | No | Partial |
| CPU SIMD (NEON / aarch64) | Yes | JVM JIT | No | N/A | Yes | No | No | No | No |
| GPU (wgpu: DX12/Vulkan/Metal) | Yes | No | No | N/A | No | No | No | No | No |
| NPU detection | Yes (informational) | No | No | N/A | No | No | No | No | No |

---

## 6. Capacity and Scale

### Engram's storage model and practical limits

Engram's `.brain` file uses memory-mapped storage with auto-growing regions. When a region fills up, engram automatically doubles its capacity -- no manual intervention, no migration, no downtime. The file starts small and grows as needed.

**Default initial capacity**: 10,000 nodes and 40,000 edges (~5 MB file). Auto-grows by doubling: 10K -> 20K -> 40K -> 80K -> 160K -> ... with no upper limit beyond disk space.

**Custom initial capacity**: `Graph::create_with_capacity(path, nodes, edges)` sets the starting size. Useful for large imports where you know the approximate size upfront, avoiding repeated grow cycles.

**Record sizes**:

| Record type | Size |
|---|---|
| Node | 256 bytes (fixed) |
| Edge | 64 bytes (fixed) |

**Practical scale reference points** for a single `.brain` file:

| Nodes | Edges | Approximate file size | Feasibility |
|---|---|---|---|
| 1M | 4M | ~500 MB | Comfortable on any modern machine |
| 10M | 40M | ~5 GB | Works on workstations with 16 GB+ RAM (OS mmap pagination handles the rest) |
| 100M | — | ~25 GB | Theoretical; requires large disk and adequate RAM for indexes |

**Wikipedia as a reference point**: Wikipedia contains approximately 6.7 million articles. A full Wikipedia import would require roughly 1.7 GB for nodes alone (6.7M x 256 bytes), plus additional space for edges and sidecar files. The in-memory indexes (label hash, adjacency lists, BM25 fulltext, HNSW) would consume an additional 8–16 GB of RAM. This is feasible on a workstation but represents the upper bound of practical single-file use.

**In-memory indexes consume additional RAM beyond the mmap file**: the label hash index, adjacency lists, BM25 inverted index, and HNSW vector index all live in process memory. For large graphs with many embeddings, RAM becomes the binding constraint before disk does.

### Horizontal scaling: engram-mesh

Scale beyond a single `.brain` file is handled by the `engram-mesh` crate, which provides peer-to-peer federation between engram instances. Each instance holds a subset of the knowledge graph. Confidence propagates across peers using a trust-based weighting model — a fact reported by a peer with high trust contributes more to local confidence than one from an untrusted peer.

This is the horizontal scaling story for engram: not sharding a single file, but federating multiple independent instances that each operate at comfortable single-node capacity.

### Comparison with other systems

| System | Practical scale | Scaling approach |
|---|---|---|
| Engram (single file) | ~10M nodes on workstation | Auto-growing mmap; see engram-mesh for federation |
| Engram (mesh) | Multiple instances federated | P2P sync with trust-based confidence propagation |
| Neo4j | 34 billion nodes (theoretical, enterprise) | Causal clustering, multiple store files |
| Redis | Tens of millions of keys | Limited by available RAM; Redis Cluster for scale-out |
| Pinecone | Billions of vectors | Serverless, cloud-managed sharding |
| Qdrant | Billions of vectors | Sharding across cluster nodes |
| SQLite | 281 TB theoretical maximum | Single-writer; practical limit is tens of millions of rows |
| Weaviate | Billions of objects | Multi-tenant, horizontal cluster |

The honest summary: engram is not a large-scale distributed system. A single `.brain` file serves single-machine workloads up to the low tens of millions of nodes. For AI agent memory use cases — where the knowledge graph represents a bounded domain rather than a global corpus — this is sufficient. For internet-scale retrieval, use a system built for it.

---

## 7. Training and Import

This section covers a key differentiator: the time and complexity required to get a fact from "I know this" to "I can query it."

### The zero-pipeline import model

Engram is designed around the premise that structured facts should be immediately storable and immediately queryable, with no intermediate processing pipeline. This stands in direct contrast to every other system in this comparison when used for AI knowledge management.

**Store and query in sub-millisecond time**:

```bash
# CLI: store a fact
engram store "PostgreSQL" --type database

# HTTP API: store an entity
POST /store
{"label": "PostgreSQL", "type": "database"}

# HTTP API: natural language import
POST /tell
{"statement": "PostgreSQL is a relational database developed by UC Berkeley"}
```

After any of the above, the next query sees the result. No batch job, no reindex, no embedding computation required (unless vector search is needed for that specific node).

**Import volume**: 100,000 entities imported via the HTTP API takes seconds. Each `POST /store` is a sub-millisecond mmap write followed by an index update. There is no write buffer that needs flushing, no transaction log that needs committing in batches, and no embedding model that needs to run.

### Natural language import

The `/tell` endpoint accepts natural language statements and extracts entities and relationships using a rule-based parser that handles approximately 15 relationship patterns. Examples:

| Statement | Extracted |
|---|---|
| "PostgreSQL is a database" | node: PostgreSQL, node: database, edge: is_a |
| "Redis was created by Salvatore Sanfilippo" | node: Redis, node: Salvatore Sanfilippo, edge: created_by |
| "Django depends on Python" | node: Django, node: Python, edge: depends_on |
| "AWS is owned by Amazon" | node: AWS, node: Amazon, edge: owned_by |

The parser is deterministic and requires no model, no GPU, and no network access. It handles the patterns it was built for and falls back to a full-text store for anything it does not recognize.

### Incremental import — no rebuild required

Adding new knowledge does not require reindexing existing knowledge. The BM25 index updates incrementally on each store. The HNSW index inserts new vectors without rebuilding. Memory tier assignments and decay schedules apply immediately to new nodes.

This matters in practice: a system that requires periodic reindexing (vector databases that batch-process embeddings, graph databases that rebuild their indexes offline) introduces operational complexity that engram avoids entirely.

### Provenance and correction

Every fact stored in engram records:

- **Source type**: who or what provided it (sensor, API, user, LLM, derived inference)
- **Source ID**: an application-defined identifier for the specific source
- **Timestamp**: when it was stored and when it was last accessed

When a fact is wrong, `POST /learn/correct` immediately adjusts confidence and records the correction as a new provenance event. The corrected confidence takes effect on the next query with no reprocessing step.

### Comparison: time from fact to queryable

| System | Path from fact to queryable result |
|---|---|
| Engram | `POST /store` or `POST /tell` → sub-millisecond → immediately queryable |
| LLMs (GPT-4, Claude, Llama) | Fine-tuning: hours to days on GPU clusters. A single fact cannot be added without retraining the model. Prompt injection is not persistent storage. |
| Neo4j | `CREATE (n:Node {props})` via Cypher — fast for individual records, but bulk imports via LOAD CSV require transaction batching and can be slow for large datasets. No natural language interface. |
| Pinecone | Vectorize text with an embedding model (external, adds latency and cost) → upsert vector → queryable. No graph structure, no inference. |
| Qdrant | Same as Pinecone: embedding model required before upsert. |
| Weaviate | Vectorizer module auto-embeds on insert, but still requires an embedding model call per document. |
| SQLite | Fast inserts, but requires schema design upfront. No graph traversal, no confidence scoring. |
| RAG systems | Chunk documents + embed chunks + upsert to vector store = a multi-step pipeline. Each new document requires running the full pipeline. |

**The key differentiator**: engram is the fastest path from "I have a structured fact" to "I can query it." No embedding model, no GPU, no pipeline configuration, no batch processing window. Just store and query.

This advantage is bounded: for unstructured text at large scale, RAG with a vector database is more appropriate. Engram's `/tell` endpoint handles structured or semi-structured facts expressed in natural language sentences, not arbitrary document corpora.

### LLMs as a special case

LLMs (GPT-4, Claude, Llama, and similar) are often considered as an alternative to explicit knowledge storage. The comparison is worth making directly:

| Dimension | LLM (fine-tuned or prompted) | Engram |
|---|---|---|
| Add a single new fact | Requires retraining or fine-tuning | `POST /store` — sub-millisecond |
| Correct a wrong fact | RLHF or fine-tuning — hours to days | `POST /learn/correct` — immediate |
| Provenance of a fact | None. LLMs do not track where they learned something. | Recorded: source type, source ID, timestamp |
| Confidence in a fact | No native confidence; hallucination is undetectable from the model's own output | 0.0–1.0 per fact, source-capped |
| Offline operation | Requires model weights + compute | Single binary, single file, no network |
| Cost per query | GPU inference cost | CPU query, negligible |
| Determinism | Non-deterministic by design | Deterministic for non-vector queries |

Engram does not replace LLMs. It provides structured, auditable, correctable memory that LLMs can read and write through the MCP interface. The combination — LLM for reasoning, engram for memory — is the intended deployment pattern.

---

## 8. Storage and Deployment Model

### The `.brain` file format

Engram stores everything in a memory-mapped fixed-record file. The layout is:

```
[ Header: 4096 bytes ]
[ Node region: N * 256 bytes per node ]
[ Edge region: M * 128 bytes per edge ]
```

Adjacent sidecar files use the same stem:

- `graph.brain` — primary mmap'd store (nodes, edges)
- `graph.brain.wal` — write-ahead log for crash recovery
- `graph.brain.vectors` — serialized HNSW vector index
- `graph.brain.types` — edge type registry
- `graph.brain.props` — property store (overflow for large property values)
- `graph.brain.cooccur` — co-occurrence frequency table

A full backup is a filesystem copy of all files sharing the `.brain` stem. No dump/restore tooling is required. The WAL is replayed automatically on next open if a crash occurred before checkpoint.

### Deployment comparison

| Model | Example systems | Backup method | Network dependency |
|---|---|---|---|
| Embedded single-file | Engram, SQLite | File copy | None |
| Client-server, directory | Neo4j, Qdrant, Weaviate, Memgraph | Vendor backup tools or rsync | Localhost or network socket |
| Cloud managed | Pinecone | Vendor-managed | Always required |
| In-memory + persist | Redis, Memgraph | AOF/RDB or snapshot | Localhost or network socket |

Engram's model is deliberately aligned with SQLite's: an application includes or invokes the engram binary, opens the `.brain` file, and operates on it directly. There is no port to open, no firewall rule, no service account, and no network configuration.

---

## 9. Search Capabilities Deep Dive

### 9.1 BM25 Full-Text Search

Engram implements BM25 from scratch in `crates/engram-core/src/index/fulltext.rs` with tuning parameters K1=1.2, B=0.75 (industry standard defaults). The index is built in memory on file open and rebuilt from node labels and property values. It supports:

- Tokenized keyword search (whitespace + punctuation splitting)
- Scored results sorted by BM25 relevance
- Combined with boolean filters via the query DSL

The query DSL supports patterns directly comparable to what developers write in Lucene or Elasticsearch, but with engram-specific extensions:

```
# Full-text search
"postgresql"

# Exact label match
label:server-01

# Boolean AND
"web server" AND type:server

# Boolean OR with tier filter
tier:core OR tier:active

# Confidence filter
confidence>0.8

# Temporal range (ISO 8601 dates)
created:2024-01-01..2024-12-31

# Property filter
prop:role=database

# Combined
"database" AND tier:core AND confidence>0.7
```

No competitor in this list implements tier filters or confidence filters natively — these are engram-specific extensions that require no application-layer post-processing.

### 9.2 Vector Search (HNSW)

The HNSW index is implemented in pure Rust in `crates/engram-core/src/index/hnsw.rs` with tuning parameters M=16 (connections per layer), M_MAX0=32 (layer 0 connections), EF_CONSTRUCTION=200. It is persisted to the `.brain.vectors` sidecar on checkpoint and loaded on open.

Vector operations are accelerated:

- **x86\_64**: AVX2 + FMA (runtime detection via `is_x86_feature_detected!`)
- **aarch64**: NEON intrinsics
- **GPU**: wgpu WGSL compute shader (cosine similarity, batch parallel, 64-wide workgroup)

Embedding generation requires an external embedder implementing the `Embedder` trait. Engram does not bundle a model; this is intentional (models are large and change frequently). The application sets an embedder before the graph is used, and engram auto-embeds new labels when one is configured.

### 9.3 Hybrid Search (RRF)

Hybrid search in `crates/engram-core/src/index/hybrid.rs` uses Reciprocal Rank Fusion with the standard constant k=60. Both keyword and vector result lists are merged without needing score normalization:

```
RRF_score(d) = sum over lists L: 1 / (k + rank_L(d))
```

This is the same algorithm used by Weaviate and Qdrant for their hybrid search. Results carry a `sources` flag indicating whether a hit came from keyword, vector, or both.

---

## 10. Learning and Memory Management

This section covers capabilities unique to engram in this comparison. No other system in this document implements all of these natively.

### 10.1 Confidence Scoring

Every node and edge carries a `confidence: f32` value (0.0–1.0). Initial confidence is assigned at ingestion based on source type:

| Source type | Initial confidence | Maximum cap |
|---|---|---|
| Sensor | 0.95 | 0.99 |
| API | 0.90 | 0.95 |
| User | 0.80 | 0.95 |
| Correction | 0.90 | 0.95 |
| Derived (inference) | 0.50 | 0.80 |
| LLM | 0.30 | 0.70 |

The LLM cap of 0.70 reflects a deliberate epistemic choice: facts that originate from a language model are structurally less trustworthy than facts from sensors or human corrections, and the system enforces this regardless of how many times an LLM-sourced fact is reinforced.

### 10.2 Temporal Decay

Decay is applied per the formula:

```
confidence_new = confidence_current * 0.999 ^ days_since_last_access
```

Applied per-day. No decay within the first day. Below confidence 0.10, a node becomes an archival/GC candidate. At the 0.999/day rate:

- After 30 days (one month): ~3% reduction
- After 365 days (one year): ~30% reduction
- After ~2555 days (7 years): confidence of 0.80 decays below 0.10

This is intentional. Knowledge that has not been accessed or reinforced for years should not influence an AI agent's responses as if it were fresh.

### 10.3 Memory Tiers

Three tiers govern how knowledge is surfaced:

| Tier | Label | Use | Auto-promotion criteria |
|---|---|---|---|
| 0 | Core | Always in LLM context | confidence >= 0.90 AND access_count >= 10 |
| 1 | Active | Default; queryable | New nodes start here |
| 2 | Archival | Search-only | confidence < 0.20 OR inactive > 90 days |

Tier filters in search (`tier:core`, `tier:active`, `tier:archival`) let callers scope queries to only the knowledge appropriate for the current context. An AI agent building a short system prompt includes only Core-tier facts. A research query might include all tiers.

### 10.4 Inference Engine

The inference engine executes forward-chaining rules authored by humans. Rules match graph patterns and derive new edges. Example:

```
name: transitive_type
when:
  - edge(A, "is_a", B)
  - edge(B, "is_a", C)
then:
  - edge(A, "is_a", C, confidence = min(e1.confidence, e2.confidence))
```

Derived edges carry the `Derived` source type and are capped at 0.80 confidence. The rule engine does not invent rules — all rules are explicitly authored. This is a deliberate constraint that prevents runaway inference.

---

## 11. AI and LLM Integration

### 11.1 MCP Server

The MCP (Model Context Protocol) server in `crates/engram-api/src/mcp.rs` implements JSON-RPC 2.0 over stdio. It exposes the following tools to MCP-compatible clients (Claude, Cursor, VS Code with MCP extensions, etc.):

| Tool | Purpose |
|---|---|
| `engram_store` | Store a new fact or entity |
| `engram_query` | Query the graph with the full DSL |
| `engram_ask` | Natural language question answering |
| `engram_tell` | Natural language fact ingestion |
| `engram_prove` | Request proof of a derived fact |
| `engram_explain` | Explain a node's provenance and evidence |
| `engram_search` | Hybrid keyword + vector search |

Resources: `engram://stats`, `engram://health`.

No other system in this comparison implements MCP natively. Adding MCP support to Neo4j, Qdrant, or Weaviate requires an external adapter layer.

### 11.2 A2A Protocol

The A2A (Agent-to-Agent) module in `crates/engram-a2a/` implements the Google A2A protocol for agent discovery and task delegation. This includes agent cards, skill definitions, streaming responses, push notifications, and task lifecycle management.

For multi-agent systems where one AI agent needs to query another agent's memory, A2A provides a standard protocol layer without requiring custom API integration.

### 11.3 Natural Language Interface

The `/ask` and `/tell` endpoints use a rule-based parser (not an LLM) to interpret common English patterns. The parser handles approximately 15 relationship patterns for import (`/tell`) and approximately 8 question patterns for query (`/ask`):

**Import patterns (`/tell`)**:
- `"postgresql is a database"` → store + is_a edge
- `"Django depends on Python"` → store + depends_on edge
- `"Redis was created by Salvatore Sanfilippo"` → store + created_by edge

**Query patterns (`/ask`)**:
- `"What is postgresql?"` → node lookup
- `"What does postgresql connect to?"` → outgoing edge traversal
- `"How are postgresql and redis related?"` → path finding
- `"Find things like postgresql"` → vector similarity search

This is explicitly **not** an LLM. It is a deterministic pattern matcher. It is fast, predictable, and works offline. It only handles the patterns it was built to recognize; anything outside those patterns falls back to a full-text search.

An optional LLM backend is planned (configurable via the `ENGRAM_LLM_ENDPOINT` environment variable) for broader natural language coverage — arbitrary sentence structures, complex multi-hop questions, and paraphrase handling. When configured, the LLM backend handles patterns the rule-based parser does not recognize. The rule-based parser remains the default: zero dependencies, zero latency overhead, fully deterministic, and available in air-gapped environments.

---

## 12. Hardware Acceleration

Engram's `engram-compute` crate provides a unified acceleration abstraction with automatic backend selection.

### 12.1 CPU SIMD

Runtime CPU feature detection selects the best path:

```
x86_64: AVX2 + FMA → 8-wide f32 cosine similarity
aarch64: NEON    → 4-wide f32 cosine similarity
fallback: scalar
```

Operations accelerated: cosine similarity, dot product, batch cosine distance (for HNSW search).

### 12.2 GPU (wgpu)

The GPU backend dispatches WGSL compute shaders through wgpu, which selects the platform's best graphics API automatically:

| Platform | Backend |
|---|---|
| Windows | DX12 (primary), Vulkan (secondary) |
| macOS | Metal |
| Linux | Vulkan |

The batch cosine similarity shader uses a 64-wide workgroup (`@workgroup_size(64)`). For large ANN search over thousands of vectors, GPU batch distances can be significantly faster than CPU scalar or SIMD, especially on discrete GPUs with high memory bandwidth.

### 12.3 NPU

NPU detection is informational in v0.1.0. The system identifies Intel AI Boost, Apple Neural Engine, and Qualcomm Hexagon NPUs but routes compute through the GPU path (wgpu integrated adapters) rather than platform-specific NPU SDKs (CoreML, DirectML for NPU). Direct NPU dispatch is planned.

---

## 13. Honest Limitations of Engram

This section documents what engram cannot do as of v0.1.0. These are not hedges — they are current hard boundaries.

**No ACID transactions.** Engram uses a WAL for crash recovery but does not implement multi-operation transactions. A `store` followed by a `relate` is two separate operations. If the process crashes between them, the node exists but the edge does not. Applications that require atomic multi-step mutations should use a transactional database.

**No Cypher or SPARQL.** The query DSL covers full-text, property filters, tier filters, confidence filters, temporal ranges, and boolean combinations. It does not support path expressions, aggregations, subqueries, or the broader surface area of Cypher or SPARQL. Complex graph queries (shortest path across weighted edges, PageRank, community detection) are not supported.

**No clustering or replication.** Engram is strictly single-node, single-writer. There is no primary/replica setup, no distributed consensus, and no horizontal scaling within a single `.brain` file. The `.brain` file cannot be safely shared across multiple writers simultaneously. The `engram-mesh` crate provides federation across independent instances, but that is a different model from traditional database replication.

**No language drivers or SDKs.** As of v0.1.0, the only client interfaces are the HTTP REST API, the MCP stdio server, and direct Rust library use. There are no Python, JavaScript, Go, or .NET clients.

**No schema enforcement.** Node types and edge types are registered in a type registry, but the graph does not enforce that a node of type "server" must have a "hostname" property. This is a property graph, not a schema-enforced document store.

**HNSW index is in-memory.** The vector index is loaded entirely into RAM on open. For graphs with many high-dimensional embeddings, this will consume significant memory. There is no disk-backed paging of the vector index.

**No gRPC.** There is no gRPC implementation in v0.1.0. Some route groupings in the HTTP API use body-based parameters (sometimes described informally as "gRPC-style"), but these are plain HTTP/JSON endpoints — no protobuf serialization, no HTTP/2 framing, no generated stubs. gRPC with protobuf is a planned future addition.

**Natural language is rule-based and limited.** The `/ask` and `/tell` endpoints handle approximately 15 relationship import patterns and approximately 8 question patterns. Anything outside those patterns falls back to a full-text search rather than returning an error. An optional LLM backend is planned (via `ENGRAM_LLM_ENDPOINT`) for broader coverage. The rule-based parser will remain the default — it is fast, deterministic, and requires no external dependencies.

**v0.1.0 means APIs may change.** The REST API endpoints, MCP tool signatures, storage format, and query DSL are all subject to breaking changes. Do not build production systems on engram v0.1.0 without accepting migration work on future versions.

---

## 14. When to Use Engram

Given the limitations above, the following describes the conditions under which engram is a reasonable choice as of v0.1.0.

### Use engram when:

**You are building an AI agent or LLM-backed application that needs structured memory.**
Engram is the only system in this comparison that natively models the epistemic properties of AI-generated knowledge — confidence capped by source type, decay over time, tiered access for context window management, and contradiction detection. If your application currently stores agent memory as JSON files or unstructured text, engram provides immediate structural improvement.

**You need zero-infrastructure deployment.**
One binary, one file. No Docker container, no database server, no network configuration. If you are building a developer tool, desktop application, edge device application, or air-gapped system, engram's embedded model is directly appropriate.

**You are integrating with MCP-compatible AI tooling.**
If your workflow uses Claude, Cursor, or any other MCP-compatible client, engram is immediately usable as a structured memory tool without any adapter layer.

**Your graph is small to medium scale (hundreds of thousands to low millions of nodes) on a single machine.**
Engram's fixed-record mmap layout is efficient for this scale. It is not designed for billion-node graphs.

**You value data sovereignty.**
Engram runs entirely on your hardware. No data leaves the machine. This matters for regulated industries, personal data, and air-gapped environments where cloud services like Pinecone are not acceptable.

**You need facts to be immediately queryable with no processing pipeline.**
If your use case involves storing structured or semi-structured facts that must be retrievable in the same request cycle — no embedding model, no batch job, no reindex — engram's import model is designed for exactly this.

### Do not use engram when:

**You need ACID transactions.** Use PostgreSQL, SQLite, or Neo4j.

**You need production clustering or high availability.** Use Neo4j (Enterprise), Qdrant, or Weaviate.

**You need Cypher or SPARQL.** Use Neo4j or Apache Jena.

**You need internet-scale vector retrieval (billions of vectors, managed cloud infrastructure).** Consider Pinecone or Qdrant Cloud. Engram handles millions of vectors on a single workstation with GPU-accelerated similarity search. For larger needs, mesh federation enables a distributed knowledge model across multiple machines -- a laptop for daily work, a workstation for project knowledge with GPU compute, a home server for archival memory. Each instance holds what is relevant to its role and syncs with trust-based confidence propagation. This is not cloud sharding -- it is distributed personal/team knowledge infrastructure where each node operates independently and knowledge flows between them.

**You need real-time streaming graph mutations with push-based triggers.** Consider Memgraph today. Engram's inference rules (`learn/derive`) are currently pull-based (called on demand). Auto-triggering rules on every `store`/`relate` operation is a small addition -- the rule engine exists, it just needs to be invoked automatically. This is an open topic for a future iteration. For batch analytics and on-demand inference, engram works today.

**You need full W3C semantic web compliance today (OWL reasoning, SPARQL federation, RDF provenance).** Use Apache Jena or Virtuoso for production semantic web deployments today. However, engram's property graph maps naturally to RDF triples (node = subject, edge = predicate, target = object), and the agent ecosystem is moving toward semantic interoperability. Planned additions: JSON-LD export/import (enabling data exchange with Wikidata, DBpedia, schema.org) and a SPARQL query adapter. The goal is not to replace triple stores but to let engram participate in the linked data ecosystem -- agents sharing structured knowledge across systems using standard formats.

**You need a production system today.** Engram is v0.1.0. Evaluate it, prototype with it, contribute to it — but do not run it in a production system where data loss, API breakage, or correctness bugs would be unacceptable. The WAL and mmap-based storage are sound in design but have not accumulated the years of adversarial testing that SQLite, Neo4j, or PostgreSQL have.

### The honest positioning

Engram occupies a space that did not exist as a standalone product before LLM-backed agents became a common software pattern: **structured, learning, embedded memory for AI systems**. The closest thing to it previously was a developer hand-rolling a SQLite schema with confidence and decay columns and writing the decay sweep as a cron job. Engram makes that a primitive.

What makes engram different is not any single feature — it is the combination: knowledge graph + confidence scoring + learning lifecycle + hardware-accelerated compute + zero-pipeline import + single-file deployment + mesh federation. No other product in this comparison offers all of these in a 2.1 MB binary with no runtime dependencies.

Engram scales from a single-file embedded agent memory to a federated knowledge mesh across multiple instances. It is the fastest path from "I have a fact" to "I can query it, traverse it, reinforce it, correct it, and let it decay" — and that path runs in sub-millisecond time, offline, on any platform.
