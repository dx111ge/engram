# Changelog

All notable changes to Engram are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [1.1.2] - 2026-04-10

### Fixed
- Chat search tool now generates LLM summary after showing results
- Chat topic_map tool now generates LLM summary after topic mapping
- Conflicts list pagination (was hardcoded to 20, now paginated)
- Mesh endpoints return 200 with disabled status instead of 503
- Ingest page sends correct JSON format (items array)
- Onboarding wizard includes Serper.dev as search provider

### Added
- SearxNG setup guide (docs/searxng-setup.md)

## [1.1.1] - 2026-04-09

### Fixed
- Bundle frontend with binary in zip (no separate folder needed)
- Detect frontend in both dist/ and flat layout
- macOS native runners per architecture

## [1.1.0] - 2026-04-09

### Added

**Multi-Agent Debate Engine**
- 7 debate modes: Analyze, Red Team, Devil's Advocate, Scenario Planning, Delphi, Structured Analytic Techniques, War Game
- War Room: 3-panel live dashboard with SSE events, agent cards with gauges and sparklines, session recovery
- 3-layer synthesis pipeline (per-round, cross-round, final)
- Starter plate briefing, moderator agent, bias modeling per agent
- Configurable adaptive timeouts, context compression, memory cleanup
- `POST /debate/start`, `GET /debate/status`, `POST /debate/cancel`
- Mode-aware synthesis with activity feed, round summary, and synthesis result views

**Chat System** (47 tools, 8 clusters)
- Intent routing to 8 tool clusters: analysis, investigation, reporting, temporal, assessment, action, reasoning, general
- Analysis tools: compare, shortest path, LLM-augmented analysis, black area detection
- Investigation tools: network analysis, entity 360, entity gaps
- Reporting tools: briefing, export, dossier, topic map, graph stats
- Temporal tools: provenance, contradictions, situation snapshot
- Assessment tools: structured evidence, Bayesian calculation
- Action tools: rule create/list/fire, schedule
- Reasoning tools: what-if 2-hop cascade, multi-path influence
- Slash command autocomplete, tool cards, store-then-relate workflow

**Assessment Engine**
- Bayesian confidence calculation with structured evidence
- Living assessments: stale alerts, auto-detection, dynamic watches
- LLM-powered watch suggestions
- Assessment management: edit, search, filter, status lifecycle
- Inline expansion with evidence board
- `POST /assess`, `GET /assess`, `PUT /assess/{id}`

**Document Pipeline**
- Full PDF ingest with pdf-extract crate (keyword search + LLM read)
- Table-aware document processing (HTML + PDF table extraction)
- Source CRUD: `POST/GET/PUT/DELETE /sources` with test/run endpoints
- Background polling, folder watch (notify crate), per-document reprocess
- Translation support during ingest
- Document provenance chain: Entity -> Fact -> Document -> Publisher
- Documents vs facts separation (pending doc nodes, create_documents flag)
- `.brain.sources` persistence, `.brain.docs.N` + `.brain.docs.idx` storage

**Fact Extraction**
- SPO triple extraction with confidence cascade
- Compact fact review UI with pagination
- Fact confirmation workflow (review, accept, reject)
- Source view and graph status colors
- `GET /facts`, `POST /facts/review`, `POST /facts/confirm`

**Temporal Facts**
- `valid_from` / `valid_to` on edges via `relate_with_temporal()`
- 3-layer extraction: LLM prompt extension, quick NER/RE LLM pass, user manual fallback
- Temporal succession detection (distinguished from real conflicts)

**Contradictions & Conflicts**
- ConflictDetector wired into production pipeline
- `conflicts_with` graph edges with resolution metadata
- User-configurable singular properties (e.g., "capital_of" can have only one value)
- Contradiction UI with resolution actions (supersede, contest, dispute)
- `GET /conflicts`, API display with resolution workflow

**Intelligence Gaps**
- Domain-based asymmetric gap detection (replaces noisy type-based approach)
- Human-readable labels, filtered internal types
- LLM-generated suggested search queries with user edit
- Background quality enrichment endpoint
- Dismiss and persist gaps across sessions
- Gap enrichment actions: Enrich, Search & Ingest, Dismiss

**NER Category Learning**
- Three-tier self-improving label system (gazetteer > rules > learned > GLiNER > LLM)
- API for category management + UI in System page
- Learned categories from graph feed back into NER chain

**GLiNER2 In-Process ONNX**
- All NER/RE sidecars removed -- single-binary inference
- GPU acceleration: DirectML (Windows), CUDA (Linux), CoreML (macOS)
- CPU fallback when no GPU available

**Domain Taxonomy**
- User-defined domains in EngineConfig
- LLM suggests domains from graph content
- Auto-classification of entities during ingest and debate
- UI in System page + onboarding wizard

**System Prompt**
- Configurable `llm_system_prompt` per `.brain` file
- Used in all LLM calls (debate, chat, ingest, gap-closing)
- Auto-generated during onboarding based on domain and seed content

**Onboarding Wizard**
- 11-step guided setup: embedder, NER, relation extraction, LLM, research/domain, quantization, knowledge base sources, web search, seed enrichment, completion
- Seed enrichment: area-of-interest detection, entity review, relation review, commit
- All steps available via REST API for headless/CLI setup
- `GET /config/status`, `POST /config/wizard-complete`
- `POST /ingest/seed/start`, `/confirm-aoi`, `/confirm-entities`, `/confirm-relations`, `/commit`

**Tiered Web Search**
- Fallback chain: SearXNG -> Serper -> Brave -> DuckDuckGo
- Multi-language search per gap (queries generated in target language)
- Output language configuration
- Google CX removed (API discontinued Jan 2027)

**Source Management**
- Full CRUD: `POST/GET/PUT/DELETE /sources`
- Per-source test and run endpoints
- Background polling with adaptive frequency
- Folder watch for local file sources
- Gazetteer-driven source lifecycle with ledger dedup

### Changed

**Ingest Pipeline** (`--features ingest`)
- Full ELT pipeline with NER, entity resolution, and conflict detection
- Language detection, graph-aware gazetteer, rule-based NER with per-language patterns
- NER chain with cascade/merge strategies
- Conservative 4-step progressive entity resolution
- Content deduplication (hash + semantic)
- Multi-threaded pipeline executor
- Search ledger with temporal cursors and query subsumption
- Adaptive frequency scheduling
- Webhook and WebSocket real-time ingest endpoints

**Action Engine** (`--features actions`)
- Event-driven rule engine triggered by graph changes
- TOML rule definitions with pattern matching
- Internal effects (confidence cascade, edge creation, tier change)
- External effects (webhook, API call, notifications)
- Dynamic ingest job creation from rules
- Safety constraints (cooldown, chain depth, effect budget)
- Timer-based scheduled triggers
- Dry run mode for rule testing

**Reasoning Layer** (`--features reason`)
- Knowledge gap detection (frontier nodes, structural holes, temporal gaps, confidence deserts)
- Coordinated cluster detection (suspicious patterns)
- Severity scoring and ranking
- Suggested query generation from graph topology
- LLM-powered investigation suggestions (optional)
- Mesh knowledge profile auto-derivation
- Federated query protocol across mesh peers
- 3-tier enrichment dispatcher (mesh > free > paid)
- Mesh-level blind spot detection

**Relation Extraction Architecture**
- Co-occurrence is now discovery-only: finds entity pairs without assigning types
- SPARQL classifies pairs first (ground truth, 0.80 confidence)
- GLiNER2 classifies remaining pairs by reading actual text context
- 4-tier relation triage: Confirmed / Likely / Uncertain / NoRelation
- Ingest review mode (`POST /ingest` with `review: true`): analyze without committing
- Edge rename autocomplete from known relation types
- Fuzzy dedup warning on edge rename

**Streaming & Protocol Extensions**
- Real-time graph event stream via SSE (`GET /events/stream`)
- SSE enrichment, batch progress, and debate progress streaming
- 6 new MCP tools: `engram_gaps`, `engram_frontier`, `engram_mesh_discover`, `engram_mesh_query`, `engram_ingest`, `engram_create_rule`
- 3 new A2A skills: `analyze-gaps`, `federated-search`, `suggest-investigations`
- gRPC streaming service (server-streaming events/enrichment/progress, client-streaming bulk ingest)

**Frontend** (Leptos WASM)
- Complete rewrite in Leptos 0.8 (Rust to WebAssembly)
- 4-section navigation: Knowledge | Insights | Debate | System
- Knowledge: graph explorer with inline node editor, search, documents zone, facts, chat
- Insights: intelligence gaps, assessments, contradictions
- Debate: 7-mode selector, War Room live dashboard
- System: configuration, NER/RE settings, sources, domain taxonomy, onboarding wizard
- vis.js graph visualization with wasm-bindgen interop
- Legend checkboxes filter graph by node type
- SSE live update integration throughout

### Fixed
- Two-phase checkpoint: WAL under write lock, sidecars under read lock
- Graph lock hardening for concurrent debate sessions
- Context compression for long LLM conversations
- Adaptive timeouts for slow LLM providers
- War Room: UTF-8 crash, card update dedup, nav guard, session recovery
- SSE status format (PascalCase from `format!("{:?}")`)
- Document reprocess: memory safety, table awareness, re-fetch strategy
- Seed enrichment: broken SSE events, missing completion signal
- Temporal succession detection distinguished from real conflicts
- Person+person co-occurrence defaults to "works_with" (was "married_to")
- Person+location co-occurrence defaults to "based_in" (was "born_in")

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
