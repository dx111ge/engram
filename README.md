# Engram

**AI Memory Engine** -- knowledge graph + semantic search + reasoning + learning in a single binary.

---

## What is Engram?

Engram is a high-performance knowledge graph engine built as persistent memory for AI systems. It combines graph storage, semantic search, logical reasoning, and continuous learning into a single binary with a single `.brain` file.

- **Single binary** -- no runtime dependencies, no Docker, no cloud
- **Single file** -- one `.brain` file is your entire knowledge base. Copy = backup, move = migrate
- **No external database** -- everything is built in
- **Hybrid search** -- BM25 full-text + HNSW vector similarity + bitmap filtering
- **Confidence lifecycle** -- knowledge strengthens with confirmation, weakens with time, corrects on contradiction
- **Inference engine** -- forward/backward chaining, rule evaluation, transitive reasoning
- **Knowledge mesh** -- peer-to-peer sync between engram instances with ed25519 identity and trust scoring
- **Built-in web UI** -- visual graph explorer, search, natural language queries, learning dashboard
- **Multiple APIs** -- HTTP REST, MCP (stdio), gRPC, LLM tool-calling, natural language queries

## Quick Start

### 1. Download

Download the latest binary from [Releases](https://github.com/dx111ge/engram/releases).

### 2. Create a Knowledge Base

```bash
engram create my.brain
```

### 3. Store Knowledge

```bash
engram store "PostgreSQL" my.brain
engram store "Redis" my.brain
engram relate "PostgreSQL" "caches_with" "Redis" my.brain
```

### 4. Query

```bash
engram query "PostgreSQL" 2 my.brain
```

### 5. Search

```bash
engram search "database" my.brain
engram search "confidence>0.8" my.brain
engram search "type:server AND confidence>0.5" my.brain
```

### 6. Start the API Server

```bash
engram serve my.brain
# HTTP API on http://localhost:3030
# Web UI: open http://localhost:3030 in your browser
# Health check: curl http://localhost:3030/health
```

The built-in web UI provides a visual graph explorer, search interface, natural language query panel, and learning operations dashboard. No additional setup required -- just open the server URL in your browser.

### 7. Use as MCP Server

```bash
engram mcp my.brain
```

Add to your Claude Code `.mcp.json`:
```json
{
  "mcpServers": {
    "engram": {
      "command": "engram",
      "args": ["mcp", "/path/to/my.brain"]
    }
  }
}
```

---

## CLI Reference

| Command | Description |
|---------|-------------|
| `engram create [path]` | Create a new `.brain` file |
| `engram stats [path]` | Show node and edge counts |
| `engram store <label> [path]` | Store a node |
| `engram set <label> <key> <value> [path]` | Set a property on a node |
| `engram relate <from> <rel> <to> [path]` | Create a relationship |
| `engram query <label> [depth] [path]` | Query a node and traverse edges |
| `engram search <query> [path]` | Search (BM25, filters, boolean) |
| `engram delete <label> [path]` | Soft-delete a node |
| `engram serve [path] [addr]` | Start HTTP + gRPC server (default: `0.0.0.0:3030`) |
| `engram mcp [path]` | Start MCP server (JSON-RPC over stdio) |
| `engram reindex [path]` | Re-embed all nodes after model change |

### Search Syntax

```
engram search "postgresql"                      Full-text keyword search
engram search "confidence>0.8"                  Filter by confidence
engram search "prop:role=database"              Filter by property
engram search "tier:active"                     Filter by memory tier
engram search "type:server AND confidence>0.5"  Boolean queries
engram search "postgresql OR mysql"             Boolean OR
engram search "database NOT mysql"              Boolean NOT
```

---

## HTTP API

Full reference: [docs/http-api.md](docs/http-api.md)

### Core Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/store` | Store a node with type, properties, confidence |
| POST | `/relate` | Create a directed edge between nodes |
| POST | `/batch` | Bulk store entities and relationships |
| POST | `/query` | BFS traversal from a start node |
| POST | `/search` | BM25 full-text search |
| POST | `/similar` | Semantic similarity search (vector) |
| POST | `/ask` | Natural language query |
| POST | `/tell` | Natural language fact assertion |
| GET | `/node/{label}` | Get node with edges and properties |
| DELETE | `/node/{label}` | Soft-delete a node |

### Learning Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/learn/reinforce` | Boost confidence (access or confirmation) |
| POST | `/learn/correct` | Mark fact as wrong, propagate distrust |
| POST | `/learn/decay` | Apply time-based decay |
| POST | `/learn/derive` | Run inference rules (forward chaining) |

### Additional Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/rules` | Load push-based inference rules |
| GET | `/rules` | List loaded rules |
| DELETE | `/rules` | Clear all rules |
| GET | `/export/jsonld` | Export graph as JSON-LD |
| POST | `/import/jsonld` | Import JSON-LD data |
| POST | `/quantize` | Enable/disable int8 vector quantization |
| GET | `/health` | Health check |
| GET | `/stats` | Node and edge counts |
| GET | `/compute` | Hardware and embedder info |
| GET | `/explain/{label}` | Full provenance and co-occurrences |
| GET | `/tools` | LLM tool definitions (OpenAI format) |

---

## MCP Server

Native integration with Claude, Cursor, Windsurf, and any MCP-compatible AI tool.

Full reference: [docs/mcp-server.md](docs/mcp-server.md)

**Available tools:** `engram_store`, `engram_relate`, `engram_query`, `engram_search`, `engram_prove`, `engram_explain`

---

## Use Cases

Twelve end-to-end walkthroughs with real commands and expected output:

| # | Use Case | Description |
|---|----------|-------------|
| 1 | [Wikipedia Import](docs/usecases/01-wikipedia-import/) | Build a knowledge graph from Wikipedia summaries |
| 2 | [Document Import](docs/usecases/02-document-import/) | Ingest markdown/text with metadata and entity extraction |
| 3 | [Inference & Reasoning](docs/usecases/03-inference-reasoning/) | Vulnerability propagation and SLA mismatch detection |
| 4 | [Support Knowledge Base](docs/usecases/04-support-knowledge-base/) | IT support error/cause/solution graphs |
| 5 | [Threat Intelligence](docs/usecases/05-threat-intelligence/) | Threat actor, malware, CVE, and TTP graphs |
| 6 | [Learning Lifecycle](docs/usecases/06-learning-lifecycle/) | Full lifecycle: store, reinforce, correct, decay, archive |
| 7 | [OSINT](docs/usecases/07-osint/) | Open Source Intelligence with multi-source correlation |
| 8 | [Fact Checker](docs/usecases/08-fact-checker/) | Multi-source claim verification |
| 9 | [Web Search Import](docs/usecases/09-web-search-import/) | Progressive knowledge building from web search |
| 10 | [NER Entity Extraction](docs/usecases/10-ner-entity-extraction/) | spaCy NER pipeline for entity extraction |
| 11 | [Semantic Web](docs/usecases/11-semantic-web/) | JSON-LD import/export for linked data |
| 12 | [Codebase Understanding](docs/usecases/12-codebase-understanding/) | AST analysis for codebase knowledge graphs |

---

## Architecture

Engram is a single statically-linked binary with no runtime dependencies.

See [ARCHITECTURE.md](ARCHITECTURE.md) for a high-level overview of the system design.

**Key components:**
- **Graph storage** -- typed nodes, directed edges, properties, provenance tracking
- **Hybrid search** -- BM25 full-text index + HNSW vector index + bitmap filters
- **Confidence lifecycle** -- reinforcement, time-based decay, correction with propagation, contradiction detection
- **Inference engine** -- forward and backward chaining, rule evaluation, transitive reasoning
- **Knowledge mesh** -- ed25519 identity, peer-to-peer sync, trust model, conflict resolution
- **Compute acceleration** -- SIMD (AVX2+FMA), GPU compute (wgpu), NPU routing
- **Crash recovery** -- write-ahead log ensures no data loss on unexpected shutdown

---

## Embeddings

Engram supports external embedding APIs for vector search. Configure an OpenAI-compatible endpoint:

```bash
# Using Ollama
ENGRAM_EMBED_ENDPOINT=http://localhost:11434/v1 \
ENGRAM_EMBED_MODEL=nomic-embed-text-v2-moe:latest \
engram serve my.brain

# Using OpenAI
ENGRAM_EMBED_ENDPOINT=https://api.openai.com/v1 \
ENGRAM_EMBED_MODEL=text-embedding-3-small \
ENGRAM_EMBED_API_KEY=sk-... \
engram serve my.brain
```

### Local ONNX Embedder (no network required)

For fully offline semantic search, engram can load an ONNX embedding model directly. Place the model files as sidecars next to your `.brain` file:

```bash
# 1. Export the model (one-time setup, requires Python)
pip install optimum[onnxruntime]
optimum-cli export onnx --model intfloat/multilingual-e5-small ./e5-small-onnx/

# 2. Copy sidecar files next to your .brain file
cp ./e5-small-onnx/model.onnx ./my.brain.model.onnx
cp ./e5-small-onnx/tokenizer.json ./my.brain.tokenizer.json

# 3. Start engram -- ONNX embedder is detected automatically
engram serve my.brain
```

**Recommended model:** `intfloat/multilingual-e5-small` (~120 MB, 384 dimensions, 100+ languages).

The ONNX embedder requires no external API, no network, and no running services. Engram detects the sidecar files automatically on startup.

### No embedder

Without an embedder configured, engram uses BM25 full-text search only. With an embedder (API or ONNX), `/similar` and `/ask` provide semantic similarity search.

---

## Python Integration

See [docs/python-examples.md](docs/python-examples.md) for a comprehensive guide covering:
- EngramClient wrapper class
- Bulk import from CSV/JSON
- LangChain tool integration
- Subprocess scripting

---

## License

Engram is free for personal use, research, education, and non-profit organizations.

Commercial use requires a paid license. Contact **sven.andreas@gmail.com** for commercial licensing.

See [LICENSE](LICENSE) for full terms.
