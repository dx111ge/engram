# Changelog

All notable changes to Engram are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [1.0.0] - 2026-03-08

### Added

**Storage Engine**
- Custom binary `.brain` file format -- single file for entire knowledge base
- Write-ahead log (WAL) for crash recovery and ACID durability
- Concurrent access with reader-writer locks
- Deferred checkpoint with background flush
- Sidecar files for properties, type registry, vectors, and co-occurrence data

**Knowledge Graph**
- Typed nodes with arbitrary key-value properties
- Directed edges with confidence scores
- Provenance tracking (source, timestamps)
- Bi-temporal metadata (event time and creation time)
- Soft-delete with confidence zeroing
- BFS graph traversal with configurable depth and direction

**Search**
- BM25 full-text index with TF-IDF scoring
- HNSW vector index for semantic similarity search
- Bitmap filtering by type, tier, confidence, and properties
- Boolean query operators (AND, OR, NOT)
- Hybrid search with reciprocal rank fusion
- Int8 scalar quantization for vector memory reduction

**Embeddings**
- External embedding API support (Ollama, OpenAI, vLLM, any OpenAI-compatible endpoint)
- Auto-dimension detection
- Reindex command for model changes

**Confidence Lifecycle**
- Reinforcement (access boost + source confirmation)
- Time-based decay with configurable rate
- Correction with neighbor distrust propagation
- Contradiction detection via co-occurrence analysis
- Memory tiers: core, active, archival

**Inference Engine**
- Forward chaining with fixed-point evaluation
- Backward chaining for relationship proving
- Rule language with edge, property, and confidence conditions
- Confidence computation: min, product, literal
- Push-based rules (load, list, clear)

**Knowledge Mesh**
- Ed25519 identity and keypair management
- Peer registry with trust scoring
- Delta sync with bloom filter digests
- Gossip protocol for peer discovery
- Conflict resolution (deterministic merge)
- Topic-level access control with sensitivity labels
- Full audit trail

**APIs**
- HTTP REST server with 25+ endpoints
- MCP server (JSON-RPC over stdio) for Claude, Cursor, Windsurf
- gRPC server (optional feature flag)
- LLM tool-calling interface (OpenAI-compatible definitions)
- Natural language query (`/ask`) and assertion (`/tell`)
- Batch endpoint for bulk operations
- JSON-LD export and import
- CORS support

**Compute**
- SIMD acceleration (AVX2, FMA, NEON)
- GPU compute via wgpu (DX12, Vulkan, Metal)
- NPU routing for low-power hardware
- Compute planner for automatic backend selection

**CLI**
- `create` -- create new `.brain` files
- `store` -- store nodes
- `set` -- set properties
- `relate` -- create relationships
- `query` -- graph traversal
- `search` -- full-text search with filters
- `delete` -- soft-delete nodes
- `serve` -- start HTTP + gRPC server
- `mcp` -- start MCP server
- `reindex` -- rebuild embedding index
- `stats` -- show graph statistics
