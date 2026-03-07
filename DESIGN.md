# Engram — AI Memory Engine

## Vision

Engram is a high-performance knowledge graph engine purpose-built as persistent memory for AI systems. It combines graph storage, semantic search, logical reasoning, and continuous learning into a single binary with a single `.brain` file. No external dependencies, no vendor lock-in, no cloud required.

The name "engram" refers to the hypothetical physical trace of memory in the brain — a unit of stored knowledge.

**Core principle:** LLMs are the interface layer, not intelligence. Engram is the brain — structured, verifiable, learning knowledge that any AI interface can use.

---

## Problem Statement

Current AI systems have no real memory or reasoning:

- **LLMs** predict text patterns, hallucinate freely, forget everything between sessions
- **Vector databases** find similar text but have no concept of relationships, causality, or truth
- **RAG** retrieves documents but doesn't reason over them
- **Graph databases** (Neo4j, etc.) are enterprise-heavy, proprietary, and have no AI integration
- **Chat histories** are flat text with no structure, no learning, no verification

There is no open-source system that provides structured, queryable, learning memory for AI with built-in reasoning and confidence tracking.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                    API Layer                         │
│  HTTP/gRPC server + LLM tool-calling interface      │
│  Natural language queries via integrated embeddings  │
├─────────────────────────────────────────────────────┤
│                 Query Planner                        │
│  Parses queries, selects execution path              │
│  Routes operations to CPU / NPU / GPU               │
├──────────────┬───────────────┬───────────────────────┤
│     CPU      │     NPU       │        GPU            │
│              │               │                       │
│  • Query     │  • Embedding  │  • Mass traversal     │
│    parsing   │    generation │  • Large-scale         │
│  • Logic     │  • Small model│    similarity search  │
│    rules     │    inference  │  • Learning updates   │
│  • I/O       │  • Classify   │  • Rule evaluation    │
│  • Planning  │  • Confidence │  • Co-occurrence scan │
│  • SIMD      │    scoring    │                       │
│    fallback  │               │                       │
├──────────────┴───────────────┴───────────────────────┤
│                 Inference Engine                      │
│  Rule-based reasoning, forward/backward chaining     │
│  Contradiction detection, confidence propagation     │
├─────────────────────────────────────────────────────┤
│                 Learning Engine                      │
│  Reinforcement, decay, co-occurrence tracking        │
│  Evidence accumulation, never invents knowledge      │
├─────────────────────────────────────────────────────┤
│                 Memory Manager                       │
│  RAM <-> NPU cache <-> VRAM <-> Disk                │
│  Hot subgraph pinning, LRU eviction                  │
├─────────────────────────────────────────────────────┤
│                 Storage Engine                       │
│  Custom binary format, mmap, WAL                    │
│  Single .brain file, portable                       │
└─────────────────────────────────────────────────────┘
```

---

## Storage Engine

### Design Goals

- No external dependencies (no SQLite, no RocksDB)
- Single `.brain` file — copy = backup, move = migrate
- Memory-mapped I/O for near-RAM speed on hot data
- Optimized for graph traversal, not relational queries
- ACID transactions via write-ahead log

### File Layout

```
.brain file structure:

┌──────────────────────────────────────┐
│ Header (4 KB)                        │
│  • Magic bytes: "ENGRAM\0\0"         │
│  • Version: u32                      │
│  • Node count: u64                   │
│  • Edge count: u64                   │
│  • Free list pointer: u64            │
│  • Index region offset: u64          │
│  • WAL offset: u64                   │
│  • Checksum: u64                     │
├──────────────────────────────────────┤
│ Node Region                          │
│  Fixed-size node slots (256 bytes)   │
│  Direct access by ID: O(1)          │
├──────────────────────────────────────┤
│ Edge Region                          │
│  Packed edge lists per node          │
│  Outgoing + incoming adjacency      │
├──────────────────────────────────────┤
│ Property Region                      │
│  Variable-length key-value data      │
│  Referenced by pointer from nodes    │
├──────────────────────────────────────┤
│ Embedding Region                     │
│  Dense float32/float16 vectors       │
│  Aligned for SIMD/GPU loading        │
├──────────────────────────────────────┤
│ Index Region                         │
│  • Hash index (node lookup by key)   │
│  • HNSW index (embedding similarity) │
│  • B+tree (temporal queries)         │
│  • Type index (nodes by type)        │
├──────────────────────────────────────┤
│ WAL Region                           │
│  Append-only write-ahead log         │
│  Truncated after checkpoint          │
└──────────────────────────────────────┘
```

### Node Structure (256 bytes, fixed)

```rust
#[repr(C, align(64))]
struct Node {
    id:             u64,          // Unique node ID
    node_type:      u32,          // Type registry index
    flags:          u32,          // Active, deleted, locked, etc.
    created_at:     i64,          // Unix timestamp (nanos) — when ingested
    updated_at:     i64,          // Last modification
    event_time:     i64,          // When the event actually occurred (bi-temporal)
    confidence:     f32,          // 0.0 - 1.0
    access_count:   u32,          // For LRU and reinforcement
    last_accessed:  i64,          // For decay calculations
    memory_tier:    u8,           // 0=core (always in context), 1=active, 2=archival
    sensitivity:    u8,           // 0=public, 1=internal, 2=confidential, 3=restricted
    source_id:      u64,          // Provenance — who/what created this
    edge_out_ptr:   u64,          // Pointer to outgoing edge list
    edge_out_count: u32,          // Number of outgoing edges
    edge_in_ptr:    u64,          // Pointer to incoming edge list
    edge_in_count:  u32,          // Number of incoming edges
    prop_ptr:       u64,          // Pointer to property block
    prop_size:      u32,          // Property data size in bytes
    embed_ptr:      u64,          // Pointer to embedding vector
    embed_dim:      u16,          // Embedding dimensions (e.g. 384, 768)
    label_hash:     u64,          // Hash of primary label for fast lookup
    _padding:       [u8; 62],     // Reserved for future use, alignment
}
```

### Edge Structure (64 bytes, fixed)

```rust
#[repr(C, align(64))]
struct Edge {
    id:           u64,            // Unique edge ID
    edge_type:    u32,            // Relationship type registry index
    flags:        u32,            // Directed, bidirectional, etc.
    from_node:    u64,            // Source node ID
    to_node:      u64,            // Target node ID
    confidence:   f32,            // 0.0 - 1.0
    created_at:   i64,            // Unix timestamp
    source_id:    u64,            // Provenance
    weight:       f32,            // Optional relationship weight
    _padding:     [u8; 4],        // Alignment
}
```

### Memory-Mapped I/O

```
Physical layout in memory:

File on disk:  [.......node region.......edge region.......]
                    |                        |
                  mmap                     mmap
                    |                        |
Virtual memory: [node pages]           [edge pages]
                    |                        |
                OS page cache (transparent, managed by kernel)
                    |
              Physical RAM (hot pages stay resident)

Benefits:
- OS manages caching — hot data stays in RAM automatically
- No serialization/deserialization — structs are the file format
- File can exceed available RAM — OS pages in/out as needed
- Zero-copy reads — pointer to mmap region IS the data
```

### Write-Ahead Log (WAL)

```
WAL entry format:
┌────────┬────────┬──────────┬───────────┬──────────┐
│ SeqNo  │ OpType │ DataLen  │ Data      │ Checksum │
│ u64    │ u8     │ u32      │ [u8; N]   │ u32      │
└────────┴────────┴──────────┴───────────┴──────────┘

Operations:
  0x01  NodeCreate
  0x02  NodeUpdate
  0x03  NodeDelete
  0x04  EdgeCreate
  0x05  EdgeUpdate
  0x06  EdgeDelete
  0x07  PropertySet
  0x08  EmbeddingSet
  0x09  ConfidenceUpdate
  0x0A  Checkpoint (WAL can be truncated here)

Recovery:
  On startup, replay WAL entries after last checkpoint.
  All operations are idempotent — safe to replay.
```

---

## Index Structures

### Hash Index (Node Lookup)

- Open-addressing hash table stored in the index region
- Key: label hash or external ID -> Value: node slot offset
- O(1) lookup by name/key
- Resized (doubled) when load factor exceeds 0.7

### HNSW Index (Embedding Similarity)

- Hierarchical Navigable Small World graph for approximate nearest neighbor
- Stored in the index region, layers reference embedding pointers
- Parameters: M=16, ef_construction=200 (tunable)
- Supports incremental insertion (no full rebuild needed)
- GPU-accelerated distance computation for large-scale queries

### B+Tree Index (Temporal)

- Ordered by timestamp for time-range queries
- "What was known between T1 and T2?"
- "What changed in the last 24 hours?"
- Leaf nodes contain node IDs, internal nodes are routing

### Type Index

- Bitmap index per node type
- Fast filtering: "all nodes of type Person" without scanning
- Compact — 1 bit per node per type

### Full-Text Index (Keyword Search)

- Inverted index for exact and keyword-based retrieval (BM25 scoring)
- Complements HNSW semantic search — exact matches beat similarity for identifiers, CVE IDs, error codes, names
- Stored in the index region alongside other indexes
- Tokenized on insert, updated incrementally
- Combined query: full-text candidates UNION semantic candidates, re-ranked by combined score

```
Query routing:
  "CVE-2021-44228"              → full-text (exact match)
  "database connection timeout"  → semantic (meaning-based)
  "server-01 connection issues"  → hybrid (full-text for "server-01" + semantic for "connection issues")
```

---

## Compute Architecture

### Heterogeneous Execution

The query planner routes operations to the optimal compute unit:

```
Operation                    → Target    Reason
─────────────────────────────────────────────────────
Single node lookup           → CPU       Simple hash lookup
Parse natural language       → NPU       Small model inference
Generate embedding           → NPU       Matrix multiply, low power
Similarity top-k (small)     → NPU       < 100K vectors
Similarity top-k (large)     → GPU       > 100K vectors
Graph traversal (< 1K nodes) → CPU       Sequential, cache-friendly
Graph traversal (> 1K nodes) → GPU       Parallel BFS/DFS
Confidence propagation       → GPU       Update all nodes at once
User-defined rule evaluation → GPU       Parallel rule matching
Rule evaluation              → CPU       Logic, branching
Bulk learning updates        → GPU       Parallel weight adjustment
```

### GPU Compute (Vulkan)

Using Vulkan compute shaders via `ash` (raw Vulkan bindings for Rust) or `vulkano` (safe wrapper).

**Why Vulkan, not CUDA:**
- Open standard — works on NVIDIA, AMD, Intel, any GPU
- No vendor lock-in
- SPIR-V shader bytecode is portable and pre-compiled
- Compute shaders are well-suited for graph algorithms

**Key GPU Kernels:**

```glsl
// 1. Parallel BFS Traversal
// Each workgroup processes one frontier wave
layout(local_size_x = 256) in;

buffer Nodes    { Node nodes[];    };
buffer Edges    { Edge edges[];    };
buffer Frontier { uint frontier[]; };
buffer Next     { uint next[];     };
buffer Results  { uint results[];  };

uniform float min_confidence;
uniform uint  max_depth;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= frontier_size) return;

    uint node_id = frontier[idx];
    Node node = nodes[node_id];

    for (uint e = node.edge_out_ptr; e < node.edge_out_ptr + node.edge_out_count; e++) {
        Edge edge = edges[e];
        if (edge.confidence >= min_confidence) {
            uint pos = atomicAdd(next_count, 1);
            next[pos] = edge.to_node;
        }
    }
}

// 2. Parallel Cosine Similarity
// Compare query embedding against all stored embeddings
layout(local_size_x = 256) in;

buffer QueryEmbed   { float query[];    };
buffer AllEmbeds    { float embeds[];   };
buffer Scores       { float scores[];   };

uniform uint embed_dim;
uniform uint embed_count;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= embed_count) return;

    float dot = 0.0, norm_a = 0.0, norm_b = 0.0;
    uint offset = idx * embed_dim;

    for (uint d = 0; d < embed_dim; d++) {
        float a = query[d];
        float b = embeds[offset + d];
        dot += a * b;
        norm_a += a * a;
        norm_b += b * b;
    }

    scores[idx] = dot / (sqrt(norm_a) * sqrt(norm_b) + 1e-8);
}

// 3. Confidence Propagation
// When a node's confidence changes, propagate to neighbors
layout(local_size_x = 256) in;

buffer Nodes      { Node nodes[];        };
buffer Edges      { Edge edges[];        };
buffer Updates    { uint updated_nodes[]; };

uniform float propagation_factor;  // e.g. 0.1

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= update_count) return;

    uint node_id = updated_nodes[idx];
    Node node = nodes[node_id];
    float delta = node.confidence_delta;

    for (uint e = node.edge_out_ptr; e < node.edge_out_ptr + node.edge_out_count; e++) {
        Edge edge = edges[e];
        uint target = edge.to_node;
        // Atomic float add to target's confidence
        atomicAdd(nodes[target].confidence, delta * propagation_factor * edge.confidence);
    }
}
```

**VRAM Budget (8GB GPU example):**

```
Graph structure (nodes + edges)     2 GB  →  ~8M nodes + 50M edges
Embedding vectors (hot)             3 GB  →  ~1.5M vectors (768d, f16)
HNSW index (GPU-side)               1 GB
Traversal working buffers           1 GB
Learning scratch space              1 GB
─────────────────────────────────────────
Total                               8 GB
```

For larger graphs, the memory manager pages subgraphs between RAM and VRAM based on access patterns.

### NPU Compute (ONNX Runtime)

Using ONNX Runtime with Intel OpenVINO execution provider for the NPU.

**NPU-accelerated operations:**

```
1. Embedding Generation
   - Small embedding model (e.g. all-MiniLM-L6-v2, 384 dimensions)
   - Exported as ONNX, runs on NPU at ~13 TOPS
   - Every node gets an embedding automatically on creation
   - Runs continuously in background, low power (~5W)

2. Intent Classification
   - "Is this a query, a store operation, or a learning update?"
   - Small classifier model on NPU
   - Parses natural language into structured operations

3. Contradiction Detection
   - Small model: given two facts, are they contradictory?
   - Runs on every new fact insertion
   - Flags conflicts for the reasoning engine

4. Confidence Scoring
   - Given source type and content, initial confidence estimate
   - "User observation" → 0.7, "Sensor data" → 0.95, "LLM output" → 0.3
```

**ONNX Runtime integration:**

```rust
use ort::{Session, Value, Environment};

struct NpuEngine {
    embedding_model: Session,    // all-MiniLM-L6-v2
    classifier: Session,         // intent classifier
    contradiction: Session,      // contradiction detector
}

impl NpuEngine {
    fn new() -> Self {
        let env = Environment::builder()
            .with_execution_providers([
                // Try NPU first, fall back to CPU
                ExecutionProvider::OpenVINO,
                ExecutionProvider::CPU,
            ])
            .build();

        // Load ONNX models — NPU handles inference automatically
        NpuEngine {
            embedding_model: Session::new(&env, "models/embedding.onnx"),
            classifier: Session::new(&env, "models/classifier.onnx"),
            contradiction: Session::new(&env, "models/contradiction.onnx"),
        }
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        // Tokenize + run on NPU → 384-dim vector
        let tokens = tokenize(text);
        let output = self.embedding_model.run(vec![Value::from(tokens)]);
        output[0].as_slice().to_vec()
    }
}
```

### CPU Fallback (SIMD)

When no GPU or NPU is available, CPU handles everything:

- AVX2/AVX-512 for vectorized similarity search
- NEON on ARM (Raspberry Pi, Mac, phones)
- Scalar fallback for any architecture
- Single-threaded traversal with prefetching for cache efficiency

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

unsafe fn cosine_similarity_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = _mm256_setzero_ps();
    let mut norm_a = _mm256_setzero_ps();
    let mut norm_b = _mm256_setzero_ps();

    for i in (0..a.len()).step_by(8) {
        let va = _mm256_loadu_ps(&a[i]);
        let vb = _mm256_loadu_ps(&b[i]);
        dot = _mm256_fmadd_ps(va, vb, dot);
        norm_a = _mm256_fmadd_ps(va, va, norm_a);
        norm_b = _mm256_fmadd_ps(vb, vb, norm_b);
    }

    let dot = hsum_avx2(dot);
    let norm = (hsum_avx2(norm_a) * hsum_avx2(norm_b)).sqrt();
    dot / (norm + 1e-8)
}
```

---

## Inference Engine

The inference engine derives new knowledge from existing facts using rules.

### Rule Types

```
1. Forward Chaining (data-driven)
   "When new fact matches pattern → derive conclusion"

   Example:
   IF (X causes Y) AND (Y observed)
   THEN (X is likely cause, confidence = edge.confidence * Y.confidence)

2. Backward Chaining (goal-driven)
   "To prove X, find evidence that supports it"

   Example:
   GOAL: Why is server slow?
   FIND: (? causes server_slow) → check each candidate

3. Transitive Inference
   IF (A is_a B) AND (B is_a C)
   THEN (A is_a C, confidence = min(conf_AB, conf_BC))

4. Contradiction Rules
   IF (A contradicts B) AND (A.confidence > B.confidence)
   THEN flag B for review, reduce B.confidence

5. Temporal Rules
   IF (A happened_before B) AND (B happened_before C) AND (A causes C)
   THEN (B may_mediate A→C)
```

### Rule Definition Format

```yaml
rules:
  - name: "causal_inference"
    when:
      - pattern: "(cause) -[causes]-> (effect)"
        condition: "effect.observed == true"
    then:
      - action: "flag_likely_cause"
        target: "cause"
        confidence: "edge.confidence * effect.confidence"

  - name: "transitive_type"
    when:
      - pattern: "(a) -[is_a]-> (b) -[is_a]-> (c)"
    then:
      - action: "create_edge"
        from: "a"
        to: "c"
        type: "is_a"
        confidence: "min(edge1.confidence, edge2.confidence)"

  - name: "staleness_decay"
    when:
      - pattern: "(node)"
        condition: "now() - node.last_confirmed > 30 days"
    then:
      - action: "reduce_confidence"
        target: "node"
        factor: 0.95  # per day past threshold
```

### Execution

```rust
struct InferenceEngine {
    rules: Vec<Rule>,
}

impl InferenceEngine {
    /// Run forward chaining until no new facts are derived
    fn forward_chain(&self, graph: &mut Graph) -> Vec<DerivedFact> {
        let mut derived = Vec::new();
        let mut changed = true;

        while changed {
            changed = false;
            for rule in &self.rules {
                // Pattern match against graph
                let matches = graph.match_pattern(&rule.when);
                for m in matches {
                    if rule.condition_met(&m) {
                        let fact = rule.apply(&m);
                        if graph.store_derived(fact) {
                            derived.push(fact);
                            changed = true;
                        }
                    }
                }
            }
        }
        derived
    }

    /// Backward chaining: prove or disprove a hypothesis
    fn prove(&self, graph: &Graph, hypothesis: &Query) -> ProofResult {
        // Find all paths of evidence supporting or contradicting
        let supporting = graph.find_evidence_for(hypothesis);
        let contradicting = graph.find_evidence_against(hypothesis);

        ProofResult {
            supported: !supporting.is_empty(),
            confidence: aggregate_confidence(&supporting),
            evidence_for: supporting,
            evidence_against: contradicting,
        }
    }
}
```

---

## Learning Engine

### Core Principle: No Hallucination

**Engram never invents knowledge.** It does not create edges from inferred patterns, does not
generalize from instances to type-level rules, and does not decide what constitutes a "pattern."

Engram's value is being a reliable, verifiable knowledge store. If it starts guessing and creating
phantom edges, it becomes the unreliable system it's trying to replace. When something is unclear,
the answer is to surface evidence and ask a human — never to guess.

The LLM is good at pattern recognition. It's bad at memory. Engram is good at memory. Play to
each system's strengths.

### What Engram Does Automatically (simple, reliable)

```
1. Reinforcement
   When a fact is accessed, used successfully, or confirmed:
   → Increase confidence (capped at source-type maximum)
   → Strengthen connected edges

2. Decay
   Unaccessed, unconfirmed knowledge fades over time:
   → Confidence decreases based on time since last access/confirmation
   → Below threshold (e.g. 0.1) → marked for garbage collection
   → Mimics human forgetting — use it or lose it

3. Contradiction Flagging
   When new knowledge conflicts with existing:
   → Both facts flagged as "disputed"
   → Evidence for both sides surfaced to the user/LLM
   → Engram does NOT pick a winner — the human decides
   → Once resolved, loser's confidence is reduced (not deleted)

4. Correction Propagation
   Explicit feedback: "This fact is wrong"
   → Reduce confidence to 0
   → Propagate distrust to facts that were derived from this one
   → Record correction as provenance (who corrected, when, why)

5. Co-occurrence Tracking (passive statistics)
   Simple counters, not pattern detection:
   → "migration was followed by missing-index 3 out of 3 times within 24h"
   → "servers with type=postgres had connection-timeout 4 out of 5 times"
   → These are raw statistics surfaced on query — engram does not interpret them
   → No edges created, no rules generated, no conclusions drawn
```

### What Engram Does NOT Do (unreliable, would compromise trust)

```
REMOVED: Pattern Extraction
  - Engram does not automatically detect patterns or create causal edges
  - Instead: co-occurrence statistics are surfaced as evidence when queried
  - The LLM or human sees the evidence and decides if it's a pattern
  - If they confirm: they explicitly store the rule via engram_tell

REMOVED: Generalization
  - Engram does not automatically create type-level rules from instances
  - Instead: when queried "what risks for server type X?", engram returns
    all instances and their outcomes as evidence
  - The human or LLM generalizes if appropriate and stores it explicitly

WHY: Every edge in engram has provenance — someone or something put it there
on purpose. Automatically generated edges would have "source: engram_guessed"
which is indistinguishable from hallucination. This undermines the entire trust
model.
```

### Evidence Surfacing (replaces automated pattern detection)

When a user or LLM queries engram, the response includes statistical evidence
from co-occurrence tracking. This enables pattern recognition without engram
making assumptions.

```
Query: "migration v2.8 is planned for payment-service, any risks?"

Engram responds (no interpretation, just evidence):
{
  "direct_knowledge": [],
  "co_occurrence_evidence": [
    {
      "observation": "migration followed by missing-index",
      "occurrences": 3,
      "total_migrations": 3,
      "frequency": 1.0,
      "time_window": "within 24 hours",
      "instances": ["incident-001", "incident-002", "incident-003"]
    }
  ],
  "related_facts": [
    "payment-service depends on postgresql-15.3 (confidence: 0.85)",
    "last migration was v2.7 on Feb 15 (confidence: 0.90)"
  ]
}

The LLM sees this and tells the user:
  "Based on 3 past migrations that all caused missing indexes,
   this is a high-risk pattern. Check index coverage after migration."

If the user agrees, they confirm:
  engram_tell("payment-service migrations frequently cause missing DB indexes",
              source: "user:sven", confidence: 0.85)

Now it's real knowledge with human provenance — not a guess.
```

### Confidence Model

```
confidence: f32  // 0.0 = unknown/disproven, 1.0 = certain

Sources and initial confidence:
  Sensor/measurement data    → 0.95
  Database/API response      → 0.90
  User explicit statement    → 0.80
  User observation           → 0.70
  Human-confirmed pattern    → 0.75
  LLM-generated content      → 0.30
  Unverified external source → 0.20

Confidence updates:
  Accessed/used:             += 0.02 (cap at source max)
  Confirmed by new evidence: += 0.10
  Contradicted:              -= 0.20
  Decay per day (unaccessed): *= 0.999 (slow fade)
  Explicit correction:       = 0.0
```

### Provenance Tracking

Every fact records its origin:

```rust
struct Provenance {
    source_type: SourceType,    // User, Sensor, LLM, API, Derived, Correction
    source_id: String,          // "user:sven", "sensor:cpu_monitor", "llm:qwen2.5"
    timestamp: i64,             // When was this knowledge acquired
    method: String,             // "direct_observation", "api_call", "inference_rule:causal"
    parent_facts: Vec<u64>,     // If derived, which facts led to this
}

enum SourceType {
    User,           // Human input
    Sensor,         // Automated measurement
    LLM,            // Language model output (low trust)
    API,            // External API response
    Derived,        // Inferred by reasoning engine
    Correction,     // Explicit correction of prior fact
}
```

---

## API Design

### Core API

```
REST + gRPC + MCP, designed for LLM tool-calling integration.
MCP (Model Context Protocol) for native Claude/Cursor/IDE integration.

POST   /store              Store a new fact (entity + properties)
POST   /relate             Create a relationship between entities
POST   /query              Graph query with traversal
POST   /similar            Semantic similarity search
POST   /ask                Natural language query → structured result
POST   /tell               Natural language input → stored facts
GET    /node/{id}          Get node with all edges and properties
DELETE /node/{id}          Soft-delete (confidence → 0, provenance recorded)

POST   /learn/reinforce    Increase confidence of a fact
POST   /learn/correct      Mark fact as wrong with evidence
POST   /learn/decay        Trigger decay cycle
POST   /learn/derive       Run inference rules

GET    /health             System status
GET    /stats              Graph statistics (nodes, edges, memory usage)
GET    /explain/{id}       Full provenance chain for a fact
```

### LLM Tool Interface

Designed to be called by any LLM via function/tool calling:

```json
{
  "tools": [
    {
      "name": "engram_store",
      "description": "Store a new fact or entity in the knowledge graph",
      "parameters": {
        "entity": "string — name/label of the entity",
        "type": "string — entity type (person, server, concept, event, ...)",
        "properties": "object — key-value properties",
        "source": "string — where this knowledge comes from",
        "confidence": "float — how certain (0.0-1.0), default based on source"
      }
    },
    {
      "name": "engram_relate",
      "description": "Create a relationship between two entities",
      "parameters": {
        "from": "string — source entity",
        "to": "string — target entity",
        "relationship": "string — type of relationship (causes, is_a, part_of, ...)",
        "confidence": "float — relationship confidence"
      }
    },
    {
      "name": "engram_query",
      "description": "Query the knowledge graph with traversal",
      "parameters": {
        "start": "string — starting entity",
        "pattern": "string — traversal pattern, e.g. '-[causes]->(?)'",
        "depth": "int — max traversal depth",
        "min_confidence": "float — minimum confidence threshold"
      }
    },
    {
      "name": "engram_ask",
      "description": "Ask a natural language question about stored knowledge",
      "parameters": {
        "question": "string — natural language question"
      }
    },
    {
      "name": "engram_tell",
      "description": "Tell engram something to remember",
      "parameters": {
        "statement": "string — natural language fact or observation",
        "source": "string — attribution"
      }
    },
    {
      "name": "engram_prove",
      "description": "Find evidence for or against a hypothesis",
      "parameters": {
        "hypothesis": "string — statement to prove or disprove"
      }
    },
    {
      "name": "engram_explain",
      "description": "Explain how a fact was derived, its confidence, and provenance",
      "parameters": {
        "entity": "string — entity or fact to explain"
      }
    }
  ]
}
```

### Query Language

Minimal, purpose-built graph pattern language (not Cypher, not SPARQL):

```
// Find direct relationships
server1 -[causes]-> ?

// Multi-hop with confidence filter
server1 -[*1..3, confidence > 0.7]-> ?

// Typed traversal
? -[is_a]-> database -[hosted_on]-> ?

// Temporal
? -[created_after: "2026-01-01"]-> ?

// Combined: semantic + structural
similar("high CPU usage") -[causes]-> ? -[affects]-> service

// Aggregation
(type: server) -[has_issue]-> ? | count, group_by(issue_type)

// Full-text keyword search
search("CVE-2021-44228")

// Hybrid: keyword + graph traversal
search("log4j") -[affected_by]-> ? -[runs_on]-> server
```

### MCP Server (Model Context Protocol)

Engram exposes itself as an MCP server for native integration with Claude, Cursor, and any MCP-compatible AI tool. MCP is JSON-RPC over stdio or HTTP — a thin wrapper over the existing tool interface.

```json
{
  "tools": [
    { "name": "engram_ask",     "description": "Query stored knowledge" },
    { "name": "engram_tell",    "description": "Store a new fact" },
    { "name": "engram_query",   "description": "Graph traversal query" },
    { "name": "engram_prove",   "description": "Prove or disprove a hypothesis" },
    { "name": "engram_explain", "description": "Explain provenance of a fact" },
    { "name": "engram_search",  "description": "Full-text keyword search" }
  ],
  "resources": [
    { "uri": "engram://stats",  "description": "Graph statistics" },
    { "uri": "engram://health", "description": "System health" }
  ]
}
```

### Multi-Tenant Access Control

For team use within a single engram instance, user-level permissions control who can read/write which topics or nodes.

```toml
[users.sven]
role = "admin"
topics = ["*"]

[users.dev-agent]
role = "write"
topics = ["code", "architecture", "deployments"]
deny_read = ["credentials", "hr"]

[users.readonly-dashboard]
role = "read"
topics = ["incidents", "monitoring"]
```

---

## Knowledge Mesh (Federation)

Engram instances form a decentralized mesh network for knowledge propagation. No master node, no central server — every instance is a peer. Proven pattern from homelabmon.

### Mesh Architecture

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│  Engram A   │◄───────►│  Engram B   │◄───────►│  Engram C   │
│  Personal   │         │  Team       │         │  Org-wide   │
│  laptop     │         │  server     │         │  datacenter │
│  .brain     │         │  .brain     │         │  .brain     │
└─────────────┘         └─────────────┘         └─────────────┘
       ▲                                               ▲
       │                                               │
       └───────────────────────────────────────────────┘
                    All peers equal
                    Gossip protocol
                    Selective sync
```

### Sync Model

Not full replication — selective knowledge propagation based on relevance and trust.

```
Sync strategies:

1. Push (broadcast)
   Node learns something new → pushes to interested peers
   Based on topic subscriptions: "I care about security, networking"

2. Pull (query)
   Node needs knowledge it doesn't have → asks peers
   "Does anyone know about CVE-2026-XXXX?"
   Peers respond with matching subgraphs

3. Gossip (protocol)
   Periodic heartbeat with knowledge digest (bloom filter)
   "I have 50K facts about topic X, last updated 5min ago"
   Peers request delta if their knowledge is stale
```

### Knowledge Propagation Rules

```rust
struct SyncPolicy {
    /// What to share with this peer
    share_filter: KnowledgeFilter,
    /// What to accept from this peer
    accept_filter: KnowledgeFilter,
    /// Minimum confidence to propagate
    min_confidence: f32,
    /// Trust level for this peer (affects accepted fact confidence)
    peer_trust: f32,
    /// Sync interval
    interval: Duration,
}

struct KnowledgeFilter {
    /// Node types to include/exclude
    types: Vec<TypeFilter>,
    /// Topic tags to match
    topics: Vec<String>,
    /// Only share facts above this confidence
    min_confidence: f32,
    /// Never share these (privacy)
    exclude_labels: Vec<String>,
    /// Max depth of subgraph to share
    max_depth: u32,
}
```

### Confidence in Federated Knowledge

```
When a fact arrives from a peer:

  local_confidence = fact.confidence * peer.trust * propagation_decay

  Where:
    fact.confidence    = confidence at the source
    peer.trust         = how much we trust this peer (0.0 - 1.0)
    propagation_decay  = 0.9 per hop (knowledge degrades with distance)

  Example:
    Fact confidence at origin:  0.90
    Peer trust:                 0.80
    2 hops away:                0.9^2 = 0.81
    Local confidence:           0.90 * 0.80 * 0.81 = 0.58

  This means:
    Direct observation:    0.90 (high)
    Trusted peer says so:  0.72 (medium)
    Friend of a friend:    0.58 (lower)
    3 hops away:           0.47 (getting weak)

  Just like real-world trust in information.
```

### Conflict Resolution Across Peers

```
When peer A says "X is true" and peer B says "X is false":

1. Both facts stored with provenance (peer A, peer B)
2. Confidence comparison (including peer trust weights)
3. Recency check — newer observations may override older ones
4. If unresolvable: both kept, flagged as "disputed"
5. Local inference engine can apply domain rules to resolve

No single peer can force consensus — each node decides locally
based on its own trust model. True decentralization.
```

### Mesh Protocol

```
Wire protocol: gRPC streams over mTLS

Messages:
  Heartbeat {
    node_id: UUID,
    knowledge_digest: BloomFilter,  // compact summary of what I know
    topic_subscriptions: Vec<String>,
    fact_count: u64,
    last_updated: Timestamp,
  }

  SyncRequest {
    topics: Vec<String>,
    since: Timestamp,           // delta sync
    max_facts: u32,             // limit response size
    min_confidence: f32,
  }

  SyncResponse {
    facts: Vec<Fact>,           // nodes + edges + provenance
    has_more: bool,             // pagination
    peer_chain: Vec<PeerID>,    // propagation path (prevent loops)
  }

  QueryBroadcast {
    query: String,              // "who knows about X?"
    ttl: u8,                    // max hops to forward
    origin: PeerID,
    request_id: UUID,
  }

  QueryResponse {
    request_id: UUID,
    results: Vec<Fact>,
    source_peer: PeerID,
    hops: u8,
  }

Loop prevention:
  - Each fact carries a peer_chain (list of peers it passed through)
  - If my node_id is already in peer_chain, drop it
  - TTL decrements per hop, dropped at 0
  - Same as homelabmon's heartbeat dedup, proven pattern
```

### Privacy & Access Control

```
Knowledge classification:

  Private    — never leaves this node (personal notes, credentials)
  Team       — shared within a defined peer group
  Public     — propagated to all peers
  Redacted   — structure shared, values hidden ("I know about X but can't share details")

Per-node ACL:
  Each node/edge can carry an access_level flag (2 bits in the flags field)
  Sync engine checks access_level before including in SyncResponse

Encryption:
  Peer-to-peer: mTLS with self-signed certs (like homelabmon)
  At-rest: optional .brain file encryption
  Shared secrets: never propagated, period
```

---

## Google A2A Protocol Integration

Google's Agent-to-Agent (A2A) protocol defines a standard for AI agents to discover, communicate, and collaborate. Engram implements A2A to become a knowledge service that any agent can use.

### What A2A Provides

```
A2A is to AI agents what HTTP is to web servers — a standard protocol for interoperability.

Key concepts:
  Agent Card    — JSON describing what an agent can do (like a business card)
  Task          — A unit of work one agent asks another to perform
  Message       — Communication within a task (text, data, artifacts)
  Artifact      — Structured output from a task (files, data, results)
  Streaming     — Server-sent events for long-running tasks
  Push Notify   — Webhook callbacks for async completion
```

### Engram as an A2A Agent

```
Engram exposes itself as an A2A-compatible agent:

GET /.well-known/agent.json → Agent Card

Any A2A-compatible agent (ChatGPT, Claude, Gemini, custom agents)
can discover engram and use it as a knowledge service.

No custom integration needed per agent — one standard, all agents.
```

### Agent Card

```json
{
  "name": "engram",
  "description": "High-performance AI memory engine. Store, query, and reason over knowledge graphs with GPU-accelerated traversal.",
  "url": "https://engram.local:9700",
  "version": "1.0.0",
  "protocol_version": "0.2",
  "capabilities": {
    "streaming": true,
    "pushNotifications": true,
    "stateTransitionHistory": true
  },
  "skills": [
    {
      "id": "store-knowledge",
      "name": "Store Knowledge",
      "description": "Store facts, entities, and relationships in the knowledge graph with confidence scoring and provenance tracking.",
      "tags": ["memory", "knowledge", "store", "facts"],
      "examples": [
        "Remember that server-01 runs PostgreSQL 15",
        "The CEO approved the budget on March 1st",
        "Python 3.12 introduced generic type syntax"
      ],
      "inputModes": ["text/plain", "application/json"],
      "outputModes": ["application/json"]
    },
    {
      "id": "query-knowledge",
      "name": "Query Knowledge",
      "description": "Query the knowledge graph with graph traversal, semantic similarity, or natural language. Returns facts with confidence scores and provenance.",
      "tags": ["memory", "knowledge", "query", "search", "recall"],
      "examples": [
        "What do we know about server-01?",
        "Find all causes of the outage last Tuesday",
        "What technologies does the payment team use?"
      ],
      "inputModes": ["text/plain", "application/json"],
      "outputModes": ["application/json"]
    },
    {
      "id": "reason",
      "name": "Reason & Prove",
      "description": "Use logical inference to derive new knowledge, prove hypotheses, or detect contradictions in stored knowledge.",
      "tags": ["reasoning", "inference", "proof", "logic"],
      "examples": [
        "Why might the database be slow?",
        "Is it true that all production servers have monitoring?",
        "Are there any contradictions about the release date?"
      ],
      "inputModes": ["text/plain"],
      "outputModes": ["application/json"]
    },
    {
      "id": "learn",
      "name": "Learn & Correct",
      "description": "Reinforce confirmed knowledge, correct wrong facts, or trigger knowledge decay. Continuous learning from feedback.",
      "tags": ["learning", "correction", "feedback", "memory"],
      "examples": [
        "That fact about the server IP was wrong, it's actually 10.0.0.5",
        "Confirm that the deployment succeeded",
        "Forget outdated information about the old API"
      ],
      "inputModes": ["text/plain", "application/json"],
      "outputModes": ["application/json"]
    },
    {
      "id": "explain",
      "name": "Explain Provenance",
      "description": "Explain how a fact was derived, its full provenance chain, confidence history, and supporting/contradicting evidence.",
      "tags": ["provenance", "explain", "trust", "audit"],
      "examples": [
        "How do we know that server-01 is in the EU datacenter?",
        "What's the evidence for this security recommendation?",
        "Why is the confidence for this fact so low?"
      ],
      "inputModes": ["text/plain"],
      "outputModes": ["application/json"]
    }
  ],
  "authentication": {
    "schemes": ["bearer", "mtls"]
  },
  "defaultInputModes": ["text/plain", "application/json"],
  "defaultOutputModes": ["application/json"]
}
```

### A2A Task Flow

```
External Agent                          Engram
     │                                    │
     │  POST /tasks/send                  │
     │  {                                 │
     │    "skill": "query-knowledge",     │
     │    "message": {                    │
     │      "text": "What caused the      │
     │               outage on March 5?"  │
     │    }                               │
     │  }                                 │
     │───────────────────────────────────►│
     │                                    │  1. Parse intent (NPU)
     │                                    │  2. Embed query (NPU)
     │                                    │  3. Similarity search (GPU)
     │                                    │  4. Graph traversal (GPU)
     │                                    │  5. Inference (CPU)
     │                                    │  6. Format response
     │  Response:                         │
     │  {                                 │
     │    "status": "completed",          │
     │    "artifacts": [{                 │
     │      "type": "application/json",   │
     │      "data": {                     │
     │        "facts": [...],             │
     │        "confidence": 0.87,         │
     │        "provenance": [...],        │
     │        "reasoning_chain": [...]    │
     │      }                             │
     │    }]                              │
     │  }                                 │
     │◄───────────────────────────────────│
```

### Multi-Agent Collaboration via A2A

```
Scenario: AI assistant investigating a production issue

User → ChatGPT: "Why is the payment service slow?"

ChatGPT (orchestrator):
  │
  ├─► engram (A2A): "What do we know about payment service?"
  │   └─ Returns: architecture, dependencies, recent changes, past incidents
  │
  ├─► monitoring-agent (A2A): "Current metrics for payment service?"
  │   └─ Returns: CPU 95%, memory 80%, DB latency 500ms
  │
  ├─► engram (A2A): "store: payment service DB latency is 500ms (source: monitoring)"
  │   └─ Stored with confidence 0.95, linked to payment service node
  │
  ├─► engram (A2A): "What caused high DB latency in the past?"
  │   └─ Returns: "3 previous incidents, all caused by missing index after migration"
  │   └─ Co-occurrence evidence: migration → missing-index (3/3 times)
  │
  └─► ChatGPT → User: "The payment service is slow due to high DB latency (500ms).
                         Based on 3 previous incidents, this is likely caused by a
                         missing index after a recent migration. Confidence: 82%.
                         Evidence: [provenance chain]"

Then:
  ├─► engram (A2A): "store: payment slowdown on March 7 caused by DB latency,
  │                   likely missing index (source: investigation, confidence: 0.82)"
  │   └─ Knowledge grows. Next time this happens, confidence will be higher.
```

### A2A + Knowledge Mesh Combined

```
The most powerful configuration:

┌──────────────────────────────────────────────────────────────┐
│                    A2A Protocol Layer                        │
│         Any agent can discover and use any engram            │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────┐      ┌─────────┐      ┌─────────┐             │
│  │Engram A │◄────►│Engram B │◄────►│Engram C │  Knowledge  │
│  │Dev team │ mesh │Ops team │ mesh │Security │  Mesh       │
│  │.brain   │      │.brain   │      │.brain   │             │
│  └────┬────┘      └────┬────┘      └────┬────┘             │
│       │                │                │                    │
├───────┼────────────────┼────────────────┼────────────────────┤
│       │                │                │                    │
│  ┌────▼────┐      ┌────▼────┐      ┌────▼────┐             │
│  │Claude   │      │Custom   │      │Gemini   │  AI Agents  │
│  │Code     │      │DevOps   │      │Security │  (via A2A)  │
│  │Agent    │      │Agent    │      │Scanner  │             │
│  └─────────┘      └─────────┘      └─────────┘             │
│                                                              │
└──────────────────────────────────────────────────────────────┘

- Each team has their own engram with domain knowledge
- Knowledge meshes between teams (with access control)
- Any AI agent talks to any engram via A2A
- Agents can query across the mesh: "ask all engrams about X"
- Knowledge learned by one agent benefits all others
- Privacy preserved: each engram controls what it shares
```

### A2A Implementation

```rust
// A2A server built on top of engram-api

struct A2AServer {
    engram: EngramCore,
    agent_card: AgentCard,
}

impl A2AServer {
    /// GET /.well-known/agent.json
    fn agent_card(&self) -> AgentCard {
        self.agent_card.clone()
    }

    /// POST /tasks/send
    async fn handle_task(&self, task: A2ATask) -> A2AResponse {
        match task.skill.as_str() {
            "store-knowledge" => {
                let facts = self.engram.tell(&task.message.text, &task.source());
                A2AResponse::completed(facts.into_artifact())
            }
            "query-knowledge" => {
                let results = self.engram.ask(&task.message.text);
                A2AResponse::completed(results.into_artifact())
            }
            "reason" => {
                let proof = self.engram.prove(&task.message.text);
                A2AResponse::completed(proof.into_artifact())
            }
            "learn" => {
                let update = self.engram.learn(&task.message);
                A2AResponse::completed(update.into_artifact())
            }
            "explain" => {
                let chain = self.engram.explain(&task.message.text);
                A2AResponse::completed(chain.into_artifact())
            }
            _ => A2AResponse::failed("Unknown skill"),
        }
    }

    /// POST /tasks/sendSubscribe (streaming)
    async fn handle_task_stream(&self, task: A2ATask) -> impl Stream<Item = A2AEvent> {
        // For long-running queries or large result sets
        // Stream partial results as they're found during traversal
    }
}
```

---

## Project Structure

```
engram/
├── Cargo.toml
├── LICENSE                    # Apache-2.0 or MIT (truly open)
├── README.md
├── DESIGN.md                  # This document
│
├── crates/
│   ├── engram-core/           # Storage engine, graph operations
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── storage/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── mmap.rs        # Memory-mapped file management
│   │   │   │   ├── node.rs        # Node structure and operations
│   │   │   │   ├── edge.rs        # Edge structure and operations
│   │   │   │   ├── property.rs    # Variable-length property storage
│   │   │   │   ├── wal.rs         # Write-ahead log
│   │   │   │   └── brain_file.rs  # .brain file format, header, regions
│   │   │   ├── index/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── hash.rs        # Hash index for node lookup
│   │   │   │   ├── hnsw.rs        # HNSW for embedding similarity
│   │   │   │   ├── btree.rs       # B+tree for temporal index
│   │   │   │   └── bitmap.rs      # Bitmap index for type filtering
│   │   │   ├── graph.rs           # High-level graph API
│   │   │   └── query.rs           # Query parser and executor
│   │   └── Cargo.toml
│   │
│   ├── engram-compute/        # GPU/NPU/CPU compute abstraction
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── planner.rs         # Route operations to best compute unit
│   │   │   ├── vulkan/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── device.rs      # Vulkan device setup
│   │   │   │   ├── traversal.rs   # BFS/DFS compute shaders
│   │   │   │   ├── similarity.rs  # Cosine similarity shader
│   │   │   │   └── learning.rs    # Confidence propagation shader
│   │   │   ├── npu/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── onnx.rs        # ONNX Runtime integration
│   │   │   │   ├── embedding.rs   # Embedding model runner
│   │   │   │   └── classify.rs    # Intent + contradiction classifiers
│   │   │   ├── cpu/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── simd.rs        # AVX2/NEON similarity
│   │   │   │   └── traversal.rs   # CPU graph traversal
│   │   │   └── memory.rs          # RAM <-> VRAM <-> disk sync
│   │   ├── shaders/
│   │   │   ├── traversal.comp     # GLSL compute shader
│   │   │   ├── similarity.comp
│   │   │   └── propagation.comp
│   │   └── Cargo.toml
│   │
│   ├── engram-inference/      # Reasoning engine
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── rules.rs           # Rule definition and parsing
│   │   │   ├── forward.rs         # Forward chaining
│   │   │   ├── backward.rs        # Backward chaining / proof
│   │   │   ├── contradiction.rs   # Contradiction detection
│   │   │   └── temporal.rs        # Time-based reasoning
│   │   └── Cargo.toml
│   │
│   ├── engram-learning/       # Learning engine
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── reinforce.rs       # Confidence reinforcement
│   │   │   ├── decay.rs           # Knowledge decay
│   │   │   ├── cooccurrence.rs     # Co-occurrence tracking (passive counters)
│   │   │   ├── evidence.rs        # Evidence surfacing for queries
│   │   │   └── correct.rs         # Correction handling
│   │   └── Cargo.toml
│   │
│   ├── engram-mesh/           # Knowledge mesh (federation)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── peer.rs            # Peer management and discovery
│   │   │   ├── gossip.rs          # Gossip protocol, bloom filter digest
│   │   │   ├── sync.rs            # Delta sync, push/pull/query broadcast
│   │   │   ├── conflict.rs        # Cross-peer conflict resolution
│   │   │   ├── policy.rs          # Sync policies, filters, access control
│   │   │   └── trust.rs           # Peer trust model, confidence propagation
│   │   └── Cargo.toml
│   │
│   ├── engram-a2a/            # Google A2A protocol implementation
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── agent_card.rs      # Agent Card generation and serving
│   │   │   ├── tasks.rs           # A2A task handling (send, subscribe)
│   │   │   ├── skills.rs          # Skill definitions and routing
│   │   │   ├── streaming.rs       # SSE streaming for long-running tasks
│   │   │   └── discovery.rs       # Discover other A2A agents
│   │   └── Cargo.toml
│   │
│   └── engram-api/            # HTTP/gRPC server + LLM tools
│       ├── src/
│       │   ├── lib.rs
│       │   ├── server.rs          # HTTP server (axum)
│       │   ├── grpc.rs            # gRPC server (tonic)
│       │   ├── a2a.rs             # A2A endpoints (/.well-known/agent.json, /tasks/*)
│       │   ├── llm_tools.rs       # Tool definitions for LLM integration
│       │   ├── natural.rs         # Natural language query handler
│       │   └── auth.rs            # API key auth
│       └── Cargo.toml
│
├── models/                    # Pre-trained ONNX models (small)
│   ├── embedding.onnx         # all-MiniLM-L6-v2 (~80MB)
│   └── classifier.onnx       # Intent classifier (~5MB)
│
├── src/
│   └── main.rs                # CLI entry point, single binary
│
├── tests/
│   ├── storage_tests.rs
│   ├── graph_tests.rs
│   ├── inference_tests.rs
│   ├── learning_tests.rs
│   └── benchmark.rs
│
└── docs/
    ├── query-language.md
    ├── rules-format.md
    └── deployment.md
```

---

## Performance Targets

```
Operation                        Target Latency
──────────────────────────────────────────────────
Single node lookup (by ID)       < 1 μs
Single node lookup (by label)    < 10 μs
1-hop traversal (100 edges)      < 50 μs
3-hop traversal (10K nodes)      < 1 ms
3-hop traversal (1M nodes, GPU)  < 5 ms
Embedding generation (NPU)      < 10 ms per text
Similarity top-10 (1M vectors)  < 5 ms (GPU)
Similarity top-10 (1M vectors)  < 50 ms (CPU/SIMD)
Store new fact                   < 100 μs
Full inference cycle (1K rules)  < 100 ms
Natural language query end-to-end < 50 ms (excl. LLM)
```

---

## Technology Choices

| Component | Choice | Reason |
|-----------|--------|--------|
| Language | Rust | Zero-cost abstractions, no GC, mmap-safe, single binary |
| GPU API | Vulkan (ash/vulkano) | Open standard, all vendors, no CUDA lock-in |
| NPU API | ONNX Runtime (ort) | Abstracts all NPU vendors, CPU fallback |
| HTTP | axum | Fast, async, Rust-native |
| gRPC | tonic | For high-performance programmatic access |
| Embedding model | all-MiniLM-L6-v2 | Small (80MB), good quality, 384 dimensions |
| Shader language | GLSL → SPIR-V | Standard, compiled, portable |
| License | AGPL-3.0 | Truly open, no AGPL restrictions |
| File format | Custom .brain | Purpose-built, no compromise |

---

## Build & Distribution

```
Single binary: engram (or engram.exe on Windows)
Single data file: knowledge.brain
Optional: models/ directory for ONNX models (can be embedded in binary)

Cross-compilation targets:
  - x86_64-unknown-linux-gnu
  - x86_64-pc-windows-msvc
  - aarch64-unknown-linux-gnu (ARM64 / Raspberry Pi / Mac)
  - aarch64-apple-darwin (Mac M-series)

Docker:
  FROM scratch
  COPY engram /engram
  COPY models/ /models/
  ENTRYPOINT ["/engram"]
  # ~100MB total image
```

---

## Development Phases

### Phase 0: Storage Proof-of-Concept (GO / NO-GO GATE)

This is the project's viability test. If the custom storage engine doesn't deliver on zero-copy mmap performance, the project stops here. No other code should be written until this phase passes.

- [ ] .brain file format (header, region layout, magic bytes, versioning)
- [ ] mmap region management (create, open, grow) — cross-platform (Windows + Linux)
- [ ] Node struct: write to mmap, read back via pointer cast (zero-copy proof)
- [ ] Edge struct: same zero-copy proof
- [ ] WAL: append-only log, checkpoint, crash recovery with idempotent replay
- [ ] Single-writer / multiple-reader locking (LMDB model)
- [ ] Hash index for node lookup by label
- [ ] **Benchmark gate**: node lookup by ID < 1 μs, node lookup by label < 10 μs, store new node < 100 μs
- [ ] Crash test: kill process mid-write, verify recovery from WAL
- [ ] Cross-platform test: verify identical .brain file behavior on Windows and Linux

Simplifications for Phase 0 (deferred complexity):
- Append-only node allocation (no free-list recycling — compact offline later)
- No concurrent writes (single writer)
- No property region (inline a small fixed label for now)
- No embedding region yet

**EXIT CRITERIA**: benchmark passes, crash recovery works, cross-platform mmap behaves consistently. If yes → proceed. If no → stop project.

### Phase 1: Core Graph Engine
- [ ] Property storage (variable-length key-value region)
- [ ] Edge adjacency lists (outgoing + incoming per node)
- [ ] Multi-hop traversal (CPU, BFS/DFS)
- [ ] Basic graph API (store, relate, traverse, delete)
- [ ] Soft-delete with tombstones (confidence → 0)
- [ ] CLI with basic commands (store, query, stats)
- [ ] Unit tests for data integrity
- [ ] Provenance tracking on all mutations

### Phase 2: Search & Indexing
- [ ] HNSW embedding index (CPU, using user-provided ONNX model)
- [ ] ONNX Runtime integration for embedding generation
- [ ] Full-text inverted index (BM25 keyword search)
- [ ] Hybrid search (semantic + keyword, combined ranking)
- [ ] B+tree temporal index (bi-temporal: event_time + created_at)
- [ ] Type bitmap index
- [ ] Query language parser
- [ ] Benchmark suite

### Phase 3: Intelligence & Learning
- [ ] Confidence model (source-based initial scoring)
- [ ] Confidence reinforcement on access/confirmation
- [ ] Knowledge decay (time-based confidence reduction)
- [ ] Correction handling (explicit "this is wrong" with distrust propagation)
- [ ] Contradiction flagging (detect conflicts, surface both sides, never auto-resolve)
- [ ] Co-occurrence tracking (passive edge-type frequency counters)
- [ ] Evidence surfacing on queries (return statistical evidence alongside facts)
- [ ] Forward chaining inference engine (user-defined rules only)
- [ ] Backward chaining / proof engine
- [ ] Rule definition format and parser (humans create rules, engram executes them)
- [ ] Temporal reasoning
- [ ] Memory tier management (core/active/archival promotion/demotion)

### Phase 4: API & Integration
- [ ] HTTP server (axum)
- [ ] MCP server (JSON-RPC, stdio + HTTP transport)
- [ ] LLM tool definitions (OpenAI-compatible)
- [ ] Natural language query interface
- [ ] gRPC server
- [ ] Multi-tenant user authentication and topic-level ACLs
- [ ] `engram reindex` command (re-embed all nodes after model change)

### Phase 5: Compute Acceleration
- [ ] CPU SIMD fallback (AVX2/NEON) for similarity search
- [ ] Vulkan device setup and shader compilation
- [ ] GPU traversal kernel (parallel BFS)
- [ ] GPU similarity kernel (cosine similarity)
- [ ] GPU confidence propagation kernel
- [ ] NPU compute path (ONNX Runtime with OpenVINO EP)
- [ ] Compute planner (auto-select CPU/NPU/GPU based on data size)
- [ ] Memory manager (RAM <-> VRAM paging)

### Phase 6: Knowledge Mesh
- [ ] ed25519 identity generation on first start
- [ ] Peer registration (mutual approval by public key + endpoint)
- [ ] Topic-level ACLs and fact sensitivity enforcement
- [ ] mTLS transport derived from ed25519 keypair
- [ ] Gossip protocol with bloom filter knowledge digests
- [ ] Delta sync (push/pull)
- [ ] Query broadcast with TTL and loop prevention
- [ ] Trust model and confidence propagation across peers
- [ ] Conflict resolution across peers
- [ ] Audit trail for all received facts

### Phase 7: A2A Protocol
- [ ] Agent Card serving (/.well-known/agent.json)
- [ ] A2A task handling (send, subscribe)
- [ ] Skill routing (store, query, reason, learn, explain)
- [ ] SSE streaming for large result sets
- [ ] Agent discovery (find other A2A agents)
- [ ] Push notifications (webhook callbacks)

### Phase 8: Polish & Distribution
- [ ] Cross-compilation CI/CD
- [ ] Docker image
- [ ] Documentation
- [ ] Performance optimization
- [ ] Security audit

---

## Testing Strategy

### Test Pyramid

```
                    ┌─────────────┐
                    │  End-to-End  │   Few, slow, high-value
                    │  (CLI + API) │   Full scenarios through public interfaces
                ┌───┴─────────────┴───┐
                │   Integration Tests  │   Cross-crate, cross-layer
                │   (crate boundaries) │   Storage + index, API + engine, mesh + sync
            ┌───┴─────────────────────┴───┐
            │       Unit Tests             │   Fast, isolated, many
            │   (per function / module)    │   Every crate, every module
            └─────────────────────────────┘
```

### Test Categories

#### Unit Tests (per crate, run on every commit)

```
engram-core/
  storage/
    - mmap region create, open, grow, close
    - node read/write zero-copy roundtrip
    - edge read/write roundtrip
    - WAL append, checkpoint, replay
    - WAL crash recovery (simulate kill mid-write)
    - hash index insert, lookup, resize, collision handling
    - property storage variable-length read/write
    - free list management (when implemented)
  index/
    - HNSW insert, query top-k, incremental update
    - B+tree insert, range query, delete
    - bitmap index set, clear, filter
    - full-text index tokenize, insert, BM25 scoring
  graph/
    - store node, relate, traverse 1-hop, traverse n-hop
    - soft-delete, tombstone behavior
    - confidence update, decay calculation

engram-compute/
    - SIMD cosine similarity correctness (vs naive impl)
    - compute planner routing decisions
    - (Phase 5) Vulkan shader output vs CPU reference

engram-inference/
    - forward chaining with simple rules
    - backward chaining proof finding
    - contradiction flagging
    - rule parser

engram-learning/
    - reinforcement: access increments confidence
    - decay: time reduces confidence
    - correction: propagation to dependent facts
    - co-occurrence: counter increment and query

engram-mesh/
    - bloom filter digest create, merge, check
    - sync policy filtering
    - peer trust calculation
    - sensitivity label enforcement

engram-api/
    - request parsing and validation
    - response serialization
    - auth token/mTLS verification
```

#### Integration Tests (cross-crate, run on every PR)

```
Storage + Index:
  - Store 10K nodes, verify all indexes are consistent
  - Delete nodes, verify indexes updated
  - Reopen .brain file, verify all data intact

Storage + Learning:
  - Store facts, access them, verify confidence increases
  - Wait (simulated time), verify decay applied
  - Correct a fact, verify dependent confidence drops

API + Engine:
  - HTTP request → store → query → verify response
  - MCP tool call → store → query → verify
  - Concurrent readers while single writer active

Mesh + Sync:
  - Two instances, peer, sync facts
  - Verify sensitivity labels block restricted facts
  - Verify topic ACLs filter correctly
  - Conflict: both peers store contradicting facts
```

#### End-to-End Tests (full scenarios, run before release)

```
Scenario 1: Fresh start
  - Start engram with empty .brain
  - Store 100 facts via CLI
  - Query via API, verify results
  - Stop, restart, verify persistence

Scenario 2: Crash recovery
  - Store facts, kill process mid-WAL-write
  - Restart, verify WAL replay recovers all committed data
  - Verify no corruption in .brain file

Scenario 3: Cross-platform
  - Create .brain on Linux, open on Windows (and vice versa)
  - Verify identical query results

Scenario 4: Scale
  - Load 1M nodes, 5M edges
  - Benchmark all performance targets
  - Verify memory usage stays within bounds

Scenario 5: Mesh federation
  - 3 instances meshed with different ACLs
  - Store sensitive fact on instance A
  - Verify it reaches B (allowed) but not C (denied)
  - Query broadcast, verify responses from all peers

Scenario 6: LLM integration
  - Configure MCP server
  - LLM agent stores facts via engram_tell
  - LLM agent queries via engram_ask
  - Verify evidence surfacing includes co-occurrence data
```

### Benchmark Suite (run on every PR, block on regression)

```
Benchmark                          Target          Regression Gate
──────────────────────────────────────────────────────────────────
Node lookup by ID                  < 1 μs          > 2 μs = FAIL
Node lookup by label               < 10 μs         > 20 μs = FAIL
Store new node                     < 100 μs        > 200 μs = FAIL
1-hop traversal (100 edges)        < 50 μs         > 100 μs = FAIL
3-hop traversal (10K nodes)        < 1 ms          > 2 ms = FAIL
Similarity top-10 (1M, CPU/SIMD)   < 50 ms         > 100 ms = FAIL
Similarity top-10 (1M, GPU)        < 5 ms          > 10 ms = FAIL
Embedding generation (ONNX)        < 10 ms         > 20 ms = FAIL
Full inference cycle (1K rules)    < 100 ms        > 200 ms = FAIL
.brain file open (cold, 1M nodes)  < 500 ms        > 1 s = FAIL

Benchmarks use criterion.rs for statistical rigor (min 100 iterations,
confidence intervals, outlier detection).
```

### Test Infrastructure

```
Repository: http://192.168.178.26:3141/admin/engram.git
CI Pipeline (Gitea Actions):

  On every commit:
    cargo fmt --check
    cargo clippy -- -D warnings
    cargo test --workspace              # all unit tests
    cargo bench --workspace -- --test   # benchmark sanity (not full run)

  On every PR:
    All of the above, plus:
    cargo test --workspace --features integration  # integration tests
    cargo bench --workspace                        # full benchmarks
    Compare benchmarks to main branch → block PR if regression gate hit

  On release tag:
    All of the above, plus:
    End-to-end test suite
    Cross-platform build (Linux x86_64, Windows x86_64, macOS ARM64)
    Cross-platform .brain compatibility test
    cargo audit                         # dependency vulnerability check
    cargo deny check                    # license compliance (AGPL-3.0)

  Nightly (optional):
    Fuzz testing (cargo-fuzz) on storage engine:
      - Random node/edge writes + crash + recovery
      - Malformed .brain file handling
      - Oversized properties, unicode labels, edge cases
```

### Code Quality Standards

```
Coverage:
  - Storage engine (engram-core):      minimum 90% line coverage
  - Inference engine:                  minimum 85% line coverage
  - Learning engine:                   minimum 85% line coverage
  - API layer:                         minimum 80% line coverage
  - Mesh/A2A:                          minimum 80% line coverage
  - Measured with cargo-llvm-cov, enforced in CI

Safety:
  - Zero unsafe blocks outside of mmap module (storage/mmap.rs)
  - All unsafe code documented with SAFETY comments
  - Miri testing for mmap-adjacent code where possible

Linting:
  - cargo clippy with -D warnings (all warnings are errors)
  - cargo fmt enforced (no style debates)
  - No unwrap() in library code — proper error handling with thiserror
```

---

## Project Tracking

### Issue Management

```
Gitea Issues with labels:

Priority:
  P0-critical     — blocks release, data loss, security vulnerability
  P1-high         — major feature or significant bug
  P2-medium       — improvement or minor bug
  P3-low          — nice to have, cosmetic

Type:
  type:bug        — something is broken
  type:feature    — new functionality
  type:perf       — performance improvement
  type:security   — security-related
  type:test       — test coverage or infrastructure
  type:docs       — documentation

Phase:
  phase:0         — storage proof-of-concept (GO/NO-GO)
  phase:1         — core graph engine
  phase:2         — search & indexing
  phase:3         — intelligence & learning
  phase:4         — API & integration
  phase:5         — compute acceleration
  phase:6         — knowledge mesh
  phase:7         — A2A protocol
  phase:8         — polish & distribution

Component:
  crate:core      — engram-core
  crate:compute   — engram-compute
  crate:inference — engram-inference
  crate:learning  — engram-learning
  crate:mesh      — engram-mesh
  crate:a2a       — engram-a2a
  crate:api       — engram-api
```

### Milestone Structure

```
Milestones map to phases:

  v0.1.0  — Phase 0: Storage POC passes GO/NO-GO gate
  v0.2.0  — Phase 1: Core graph engine (store, relate, traverse)
  v0.3.0  — Phase 2: Search & indexing (HNSW, BM25, temporal)
  v0.4.0  — Phase 3: Intelligence & learning (rules, confidence, evidence)
  v0.5.0  — Phase 4: API & integration (HTTP, MCP, gRPC)
  v0.6.0  — Phase 5: Compute acceleration (SIMD, Vulkan, NPU)
  v0.7.0  — Phase 6: Knowledge mesh (federation, sync, trust)
  v0.8.0  — Phase 7: A2A protocol
  v1.0.0  — Phase 8: Production-ready release
```

### Branch Strategy

```
main              — always passes CI, always releasable
dev               — integration branch for current phase
feature/*         — individual features (e.g. feature/wal-recovery)
fix/*             — bug fixes
bench/*           — performance experiments

Flow:
  feature/* → PR → dev (requires: CI green, benchmarks pass, code review)
  dev → PR → main (requires: all integration tests, all benchmarks, milestone checklist)
  main → tag → release build
```

### Definition of Done (per task)

```
A task is "done" when:
  1. Code written and compiles (cargo build --workspace)
  2. Unit tests written and passing
  3. Integration tests updated if cross-crate behavior changed
  4. Benchmarks added for performance-critical code
  5. No benchmark regressions vs main branch
  6. cargo clippy clean, cargo fmt clean
  7. Unsafe code documented with SAFETY comments
  8. PR reviewed (or self-reviewed for solo phases)
  9. CI pipeline green
```

---

## Resolved Design Decisions

1. **Embedding model: bring your own (no bundled model)**
   - Engram ships without a default embedding model -- users must provide their own ONNX model
   - Keeps the binary small and allows language-aware or domain-specific model selection
   - New/better models can be swapped in without waiting for an engram release
   - When the model changes, existing vectors are invalidated -- explicit `engram reindex` required
   - Configuration via `--embedding-model ./path/to/model.onnx` or config file

2. **Versioning: temporal edges with timestamps**
   - Every fact tracks its own history via timestamps, provenance, and `supersedes` relationships
   - No automatic snapshots -- keeps `.brain` file proportional to actual knowledge
   - Full graph-state reconstruction for audit is supported but computed on demand (expensive query, acceptable for non-daily audit operations)
   - Explicit `engram snapshot` command may be added later for deliberate checkpoints

3. **Schema: schema-free**
   - No enforced schema for node or edge types
   - AI agents can store any knowledge without upfront type definitions
   - Structure emerges organically from usage patterns

4. **Encryption at rest: deferred**
   - Not implemented in initial version
   - Can be added later at the storage layer without changing the data model
   - Key concern: mmap compatibility requires per-page encryption (non-trivial), and AES-NI makes performance acceptable, but implementation complexity is deferred

5. **A2A authentication: strict zero-trust model**
   - **Identity**: each engram instance generates an ed25519 keypair on first start; public key is the instance identity
   - **Peering**: explicit mutual approval required -- both sides must add each other by public key + endpoint; no auto-discovery
   - **Topic-level ACLs per peer**:
     ```toml
     [peer."engram-sec"]
     public_key = "ed25519:abc123..."
     endpoint = "https://sec-team.example.com:9090"
     share = ["dependencies", "vulnerabilities"]
     receive = ["vulnerabilities", "compliance"]
     deny = ["internal-architecture", "credentials"]
     ```
   - **Fact-level sensitivity labels**: `public`, `internal`, `confidential`, `restricted` -- mesh sync respects these; default is `internal` (never syncs unless policy allows)
   - **Transport**: mTLS derived from the ed25519 keypair; peers trust each other's keys directly (SSH `known_hosts` model, no CA needed)
   - **Audit trail**: every fact received from a peer records who sent it, when, which peer key, and which policy allowed it

6. **Frontend: separate project**
   - Engram stays a pure headless engine (CLI + API + LLM tool-calling)
   - Web UI / graph visualization is a separate repository that talks to the engram API
   - Keeps the core binary small and focused

---

## Training Examples — Showcasing Capabilities & Speed

These examples demonstrate how engram learns, reasons, and performs in real scenarios.

### Example 1: Building Knowledge from Scratch (DevOps Domain)

```
Session: Teaching engram about a production environment

> engram tell "server-01 is a Linux Ubuntu 24.04 server in rack A" --source user:admin
  Stored: node(server-01, type:server) with 3 properties
  Embedded: 384d vector in 8ms (NPU)
  Time: 12ms total

> engram tell "server-01 runs PostgreSQL 15.3" --source user:admin
  Stored: node(postgresql-15.3, type:service)
  Created: edge(server-01 -[runs]-> postgresql-15.3, confidence: 0.80)
  Time: 14ms total

> engram tell "server-02 is a Linux Ubuntu 24.04 server in rack A" --source user:admin
  Stored: node(server-02, type:server)
  Stored: property(rack: "A")
  Time: 15ms total

> engram tell "server-02 runs PostgreSQL 15.3 as replica of server-01" --source user:admin
  Stored: edge(server-02 -[runs]-> postgresql-15.3)
  Stored: edge(server-02 -[replica_of]-> server-01, confidence: 0.80)
  Time: 18ms total

After 50 similar statements (< 1 second total):

> engram query "server-01 -[*1..3]-> ?" --min-confidence 0.5
  Results: 47 connected nodes, 83 edges
  Traversal: 0.3ms (CPU, small graph)

  server-01 → runs → postgresql-15.3
  server-01 → runs → nginx-1.24
  server-01 → in_rack → rack-a
  server-01 → has_replica → server-02
  server-01 → serves → payment-service
  payment-service → depends_on → postgresql-15.3
  payment-service → used_by → checkout-api
  ...

> engram ask "What happens if server-01 goes down?"
  Reasoning chain:
    1. server-02 is replica_of server-01 → replica may promote (confidence: 0.70)
    2. payment-service depends_on postgresql-15.3 on server-01 → affected (confidence: 0.85)
    3. checkout-api used_by payment-service → affected (confidence: 0.77)
    4. nginx on server-01 → unreachable (confidence: 0.95)

  Impact: 4 services affected, 2 critical
  Mitigation: server-02 replica exists (confidence: 0.70)
  Time: 2ms (inference) + 8ms (embedding) = 10ms total
```

### Example 2: Learning from Incidents (Evidence Accumulation)

```
Day 1: First incident
> engram tell "payment-service latency spike at 14:00, caused by missing DB index after migration v2.3" --source user:oncall --confidence 0.90
  Stored: event(incident-001) with causal chain
  Created: edge(migration-v2.3 -[causes]-> missing-index -[causes]-> latency-spike)
  Co-occurrence tracked: migration → missing-index (1 occurrence)

Day 15: Second incident
> engram tell "payment-service latency spike at 09:30, caused by missing DB index after migration v2.5" --source user:oncall --confidence 0.90
  Stored: event(incident-002) with causal chain
  Co-occurrence updated: migration → missing-index (2 occurrences)

Day 30: Third incident
> engram tell "payment-service latency spike, after migration v2.7" --source monitoring:alert --confidence 0.95
  Stored: event(incident-003)
  Co-occurrence updated: migration → missing-index (3 occurrences)

  No automatic pattern creation. No edges invented. Just a counter.

Day 31: Risk assessment
> engram ask "migration v2.8 is planned for payment-service, any risks?"

  Response (evidence, not conclusions):
  {
    "direct_knowledge": [],
    "co_occurrence_evidence": [
      {
        "observation": "migration followed by missing-index",
        "occurrences": 3, "total": 3, "frequency": 1.0,
        "time_window": "within 24 hours",
        "instances": ["incident-001", "incident-002", "incident-003"]
      }
    ],
    "related_facts": [
      "payment-service depends on postgresql-15.3 (confidence: 0.85)"
    ]
  }

  The LLM sees this evidence and warns the user.
  The user confirms: "Yes, migrations do cause missing indexes."

> engram tell "payment-service migrations frequently cause missing DB indexes" --source user:sven --confidence 0.85
  Stored as explicit human-confirmed knowledge with full provenance.
  Now it's a real fact — not a guess.
```

### Example 3: Speed at Scale (Benchmark Scenario)

```
Setup: 5 million nodes, 25 million edges, loaded from enterprise CMDB dump

Loading:
  5M nodes:          45 seconds (110K nodes/sec)
  25M edges:         90 seconds (278K edges/sec)
  5M embeddings:     8 minutes (NPU, 10K embeddings/sec)
  Total .brain file: 12 GB on disk
  VRAM loaded:       3.2M hottest nodes pinned

Queries:

  Single node lookup by ID:
    0.4 μs (mmap direct access)

  Single node lookup by label "server-01":
    3 μs (hash index)

  "Find all servers in rack A":
    Type index scan → 12,000 results in 0.8ms

  "3-hop traversal from server-01, confidence > 0.5":
    CPU (< 10K results): 0.6ms
    GPU (> 10K results): 1.2ms for 180K nodes touched

  "Find 10 most similar nodes to 'database connection timeout'":
    NPU embed query: 8ms
    GPU HNSW search across 5M vectors: 3ms
    Total: 11ms

  "What causes 'connection timeout' across all known incidents?":
    Embed + similarity: 11ms
    Graph traversal (causal chains): 2ms
    Inference (rule evaluation): 5ms
    Total: 18ms

  Full inference cycle (500 rules across 5M nodes):
    GPU parallel rule evaluation: 85ms

  Knowledge decay cycle (update all confidence scores):
    GPU parallel update: 12ms for 5M nodes

Mesh sync:
  Delta sync (1000 new facts to peer): 45ms
  Full digest exchange (bloom filter): 2ms
  Query broadcast (ask all 5 peers): 15ms + network latency
```

### Example 4: Multi-Brain Mesh Learning (Distributed Team)

```
Setup:
  engram-dev (Dev team, 500K nodes)     ← knows code, architecture, deployments
  engram-ops (Ops team, 800K nodes)     ← knows infra, incidents, monitoring
  engram-sec (Security, 300K nodes)     ← knows vulnerabilities, compliance
  All meshed, topic-based sync

Dev team stores:
> engram-dev tell "service-X uses log4j 2.14" --source build-system --confidence 0.95

Security team's engram already knows:
  node(CVE-2021-44228, type:vulnerability)
  edge(log4j-2.14 -[affected_by]-> CVE-2021-44228, confidence: 0.99)

Mesh sync triggers (within 5 seconds):
  1. engram-dev pushes "service-X uses log4j 2.14" to mesh (topic: dependencies)
  2. engram-sec receives, user-defined rule fires:
     rule: "if X uses Y and Y affected_by Z, then X affected_by Z"
     edge(service-X -[uses]-> log4j-2.14 -[affected_by]-> CVE-2021-44228)
     source: rule:vulnerability_propagation (human-authored rule)
  3. engram-sec pushes alert back to mesh:
     "service-X is vulnerable to CVE-2021-44228" (confidence: 0.94)
  4. engram-ops receives, existing edges connect to deployment:
     "service-X running on server-05, exposed to internet"
     Escalation: CRITICAL — internet-facing service with known RCE

Total time from dev storing a dependency to ops getting a critical alert: < 10 seconds
Human-authored rules do the linking. Engram executes, never guesses.

Ops team queries:
> engram-ops ask "Which internet-facing services have critical vulnerabilities?"

  Results (federated query across mesh):
    service-X on server-05: CVE-2021-44228 (log4j RCE) — confidence: 0.94
    service-Y on server-12: CVE-2024-3094 (xz backdoor) — confidence: 0.88

  Provenance: security team knowledge, build system data, ops deployment records
  Time: 25ms local + 40ms mesh query
```

### Example 5: Personal AI Memory (Daily Use)

```
Over months of daily use, engram learns about you:

Day 1:
> tell "I prefer Python for scripts and Rust for systems programming"
> tell "My project deadline is March 30"
> tell "The API key for production is in vault, not env vars"

Day 30:
> tell "Python 3.12 pattern matching is great for the parser"
> tell "Moved deadline to April 15, approved by Sarah"
  Updated: node(deadline) with temporal history
  Previous: March 30 → April 15 (provenance: user, approved by Sarah)

Day 60:
> ask "What did Sarah approve?"
  Results: deadline extension to April 15 (confidence: 0.80, decaying — 30 days old)

> ask "What languages do I use?"
  Results: Python (scripts, confidence: 0.85), Rust (systems, confidence: 0.82)
  Reinforced by usage patterns: you've stored 40 Python-related facts, 25 Rust facts

> ask "Where are production secrets stored?"
  Results: Vault, NOT env vars (confidence: 0.80)
  Note: This is a fact with negative assertion — engram remembers what NOT to do

After 6 months:
  12,000 nodes, 35,000 edges
  .brain file: 25 MB
  Query time: < 5ms for any question
  The AI assistant that uses engram as memory never forgets, never hallucinates stored facts
```

---

## Use Case: LLM Using Engram as Persistent Memory

This is the primary use case — an LLM (Claude, GPT, Llama, etc.) using engram as its knowledge backend via tool calling.

### The Problem Without Engram

```
User: "What did we decide about the authentication approach last week?"

LLM (without engram):
  "I don't have access to previous conversations. Could you remind me?"

  Or worse (hallucination):
  "Last week we decided to use OAuth2 with PKCE flow."  ← Made up

User: "Deploy service-X to production"

LLM (without engram):
  Doesn't know:
  - Which server runs service-X
  - What config it needs
  - That last deployment caused an incident
  - That there's a change freeze on Fridays
```

### The Solution With Engram

```
System prompt for any LLM:
  "You have access to engram, a persistent knowledge graph.
   Use engram_ask before answering factual questions.
   Use engram_tell to store important decisions and facts.
   Use engram_prove to verify claims before stating them.
   Use engram_explain when users ask 'how do you know that?'
   NEVER state facts from your training data when engram has relevant knowledge.
   Engram facts have confidence scores — communicate uncertainty."
```

### Full Conversation Flow

```
User: "What did we decide about the authentication approach last week?"

LLM internal:
  → Tool call: engram_ask("authentication approach decision")

Engram responds:
  {
    "facts": [
      {
        "statement": "Team decided on OAuth2 with PKCE for the mobile app",
        "confidence": 0.85,
        "timestamp": "2026-03-01T14:30:00Z",
        "source": "user:sven, context:architecture-meeting",
        "related": [
          "JWT tokens for API auth (confidence: 0.80)",
          "Rejected: API keys — too simple for mobile (confidence: 0.75)"
        ]
      }
    ]
  }

LLM to user:
  "In the architecture meeting on March 1st, the team decided on OAuth2 with
   PKCE flow for the mobile app. JWT tokens for API authentication. API keys
   were considered but rejected as too simple for mobile use.
   (Confidence: 85%, source: your input from the meeting)"

  No hallucination. Verified facts. Provenance included.
```

### Continuous Learning During Conversation

```
User: "Actually, we changed that. We're going with Passkeys instead of OAuth2."

LLM internal:
  → Tool call: engram_tell(
      "Team changed auth approach from OAuth2+PKCE to Passkeys for mobile app",
      source: "user:sven",
      confidence: 0.85
    )

Engram internally:
  1. Stores new fact: "Passkeys for mobile auth"
  2. Reduces confidence of "OAuth2 with PKCE": 0.85 → 0.40
  3. Creates edge: passkeys -[replaces]-> oauth2-pkce
  4. Records provenance: corrected by user:sven on March 7
  5. Propagates to mesh peers if policy allows

LLM to user:
  "Updated. I've recorded that the team switched from OAuth2+PKCE to Passkeys
   for the mobile app. The previous OAuth2 decision is kept in history but
   marked as superseded."

Next week, different team member asks a different LLM agent:

User2: "What's our mobile auth strategy?"

LLM2 internal:
  → Tool call: engram_ask("mobile authentication strategy")

Engram responds:
  {
    "facts": [
      {
        "statement": "Passkeys for mobile app authentication",
        "confidence": 0.85,
        "timestamp": "2026-03-07",
        "supersedes": "OAuth2 with PKCE (decided March 1, changed March 7)"
      }
    ]
  }

LLM2 to User2:
  "The current mobile auth strategy is Passkeys. This was changed on March 7th
   from the original OAuth2+PKCE decision made on March 1st."

  Correct answer. Different user. Different LLM. Same knowledge.
```

### LLM + Engram for Code Development

```
User: "Add error handling to the payment module"

LLM internal:
  → engram_ask("payment module architecture, error handling patterns")

Engram responds:
  {
    "facts": [
      "payment module is in src/services/payment.rs (confidence: 0.90)",
      "project uses thiserror for error types (confidence: 0.85)",
      "custom PaymentError enum defined in src/errors.rs (confidence: 0.80)",
      "all API errors must include request_id for tracing (confidence: 0.90)",
      "previous bug: unhandled timeout in stripe API call caused data loss (confidence: 0.88)"
    ]
  }

LLM now knows:
  - Where the code is
  - What error library to use (project convention)
  - Existing error types to extend
  - A specific past bug to avoid repeating
  - A requirement about request_id tracing

The LLM writes code that follows project conventions, uses existing patterns,
and specifically handles the timeout case that caused a bug before.

After the code is merged:
  → engram_tell("Added timeout handling to Stripe API calls in payment module,
     prevents data loss from unhandled timeouts", source: "code:merged-pr-42")

Knowledge grows. Next developer benefits.
```

### LLM + Engram vs. Plain RAG

```
                        Plain RAG              LLM + Engram
──────────────────────────────────────────────────────────────
Storage             Vector DB (flat)        Knowledge graph
Retrieval           Similarity only         Similarity + traversal + inference
Relationships       None                    First-class (edges, types)
Confidence          None                    Per-fact confidence scores
Learning            None (static index)     Continuous (reinforce, decay, correct)
Contradiction       Returns both, confused  Detects and resolves
Provenance          Maybe a source field    Full chain (who, when, why, how)
Reasoning           None                    Forward/backward chaining
Multi-hop           Can't do                Native (3-hop in 1ms)
Time awareness      None                    Temporal index, versioning
Correction          Re-index everything     Update confidence, keep history
Multi-agent         Each has own index      Shared mesh, A2A protocol
Hallucination       Still possible          Facts are verifiable
```

---

## Inspirations

- **Memex** (Vannevar Bush, 1945) — the original vision of associative memory
- **Engram** (neuroscience) — physical memory trace in the brain
- **SQLite** — single-file, zero-config, everywhere — the distribution model to follow
- **llama.cpp** — one stubborn project that made AI accessible — the spirit to follow
- **LMDB** — mmap-based storage that proved the approach works at scale
