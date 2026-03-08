# Use Case 1: Building a Knowledge Base from Wikipedia

### Overview

Wikipedia exposes a public REST API that returns article summaries in plain JSON -- no authentication, no rate-limit keys for casual use. Engram's HTTP API accepts JSON over standard HTTP. Together, they let you build a factual knowledge graph from Wikipedia content with a short Python script.

This use case includes two scripts:

1. **`import_wiki.py`** -- Quick start: imports 4 programming languages, their paradigms and creators. Produces ~24 nodes, ~22 edges in seconds. Good for verifying your setup works.

2. **`import_wiki_deep.py`** -- Full demo: imports 14 seed articles across programming languages, CS concepts, and organizations, follows up with 16 related articles, cross-links with natural language, then deeply enriches leaf nodes with creator biographies, paradigm interconnections, type system hierarchies, compiler phases, OS internals, memory safety chains, concurrency models, and more. Produces **638 nodes, 377 edges** from **974 API calls**.

**What this demonstrates:**

- The `/store` and `/relate` HTTP endpoints for structured ingestion
- The `/tell` natural-language endpoint parsing "X is a Y", "X uses Y", "X was developed at Y" patterns
- BM25 full-text search with boolean queries, property filters, and confidence filters
- Semantic vector search (with embedder) for conceptual queries
- Graph traversal with configurable depth and minimum confidence
- The `/ask` endpoint for natural-language queries over imported data
- JSON-LD export of the complete graph
- The web UI frontend for interactive graph exploration

### Performance

Measured on an Intel i7 + RTX 5070 with Ollama running locally:

| | Without Embedder | With Embedder (nomic-embed-text-v2-moe, 768D) |
|---|---|---|
| **Basic import** (4 articles, ~24 nodes) | ~2s | ~4s |
| **Deep import** (638 nodes, 377 edges) | ~19s | ~81s |
| **Overhead per node** | ~0ms | ~80ms (Ollama embedding call) |

Most of the time is Wikipedia API latency and Ollama embedding calls, not engram. The graph engine itself processes stores and relates in microseconds.

**Quality difference with embedder:**

| Query | Without Embedder (BM25 only) | With Embedder (BM25 + semantic) |
|---|---|---|
| "memory safe programming" | No results | memory safety, memory management |
| "graph databases and linked data" | graph database | linked data (11.0), graph database (4.9), knowledge graph (4.9) |
| "artificial intelligence applications" | No results | artificial intelligence (13.3) |
| "concurrent and parallel execution" | No results | concurrent programming (5.5) |

Graph traversal, `/ask`, `/tell`, property filters, and confidence filters work identically in both modes -- they don't use embeddings.

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed (`pip install requests`)
- Internet access for the Wikipedia API
- (Optional) An embedding model for semantic search -- see **Embedder Setup** below

### Embedder Setup (Optional)

Engram supports semantic search via any OpenAI-compatible embeddings API. The most common local option is **Ollama**.

#### Install Ollama and an embedding model

```bash
# Install Ollama (https://ollama.com/download)
# Then pull an embedding model:
ollama pull nomic-embed-text-v2-moe
```

Other models that work: `nomic-embed-text`, `mxbai-embed-large`, `all-minilm`, `snowflake-arctic-embed`.

#### Configure engram

Set environment variables before starting the server:

```bash
export ENGRAM_EMBED_ENDPOINT=http://localhost:11434/v1
export ENGRAM_EMBED_MODEL=nomic-embed-text-v2-moe:latest
```

Engram auto-detects the embedding dimension by sending a probe request at startup. You do **not** need to set `ENGRAM_EMBED_DIM` unless you want to override the default dimension (some Matryoshka models like nomic-embed-text-v2-moe support multiple dimensions: 256, 512, 768).

| Variable | Required | Default | Description |
|---|---|---|---|
| `ENGRAM_EMBED_ENDPOINT` | Yes | -- | Base URL of the embeddings API (must serve `/embeddings`) |
| `ENGRAM_EMBED_MODEL` | No | `multilingual-e5-small` | Model name passed in the API request |
| `ENGRAM_EMBED_DIM` | No | Auto-detected | Override embedding dimension (auto-probe if not set) |
| `ENGRAM_EMBED_API_KEY` | No | -- | API key for authenticated endpoints (OpenAI, Azure, etc.) |

**Compatible APIs**: Ollama, OpenAI, vLLM, LiteLLM, text-embeddings-inference, Azure OpenAI -- anything that serves the OpenAI `/v1/embeddings` format.

---

## Quick Start (Basic Import)

### Step 1: Start the engram server

Without embedder:
```bash
engram serve wiki.brain 127.0.0.1:3030
```

With embedder:
```bash
ENGRAM_EMBED_ENDPOINT=http://localhost:11434/v1 \
ENGRAM_EMBED_MODEL=nomic-embed-text-v2-moe:latest \
engram serve wiki.brain 127.0.0.1:3030
```

### Step 2: Run the basic import

```bash
python import_wiki.py
```

This imports Python, Rust, JavaScript, and Go with their paradigms and creators. ~24 nodes, ~22 edges.

### Step 3: Query

```bash
# Keyword search
engram search "functional programming" wiki.brain

# Property filter
engram search "prop:typing=static typing" wiki.brain

# Graph traversal
engram query Python 2 wiki.brain

# Natural language
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What does Rust connect to?"}'
```

---

## Deep Import (Recommended)

The deep import script builds a rich, interconnected knowledge graph that demonstrates engram's full capabilities -- especially graph traversal at depth 3-4 where relationships chain through creators, paradigms, institutions, and CS concepts.

### Step 1: Start the engram server

```bash
# Without embedder (keyword search only, faster ingestion)
engram serve wiki_deep.brain 127.0.0.1:3030

# With embedder (keyword + semantic search)
ENGRAM_EMBED_ENDPOINT=http://localhost:11434/v1 \
ENGRAM_EMBED_MODEL=nomic-embed-text-v2-moe:latest \
engram serve wiki_deep.brain 127.0.0.1:3030
```

### Step 2: Run the deep import

```bash
# Without embedder
python import_wiki_deep.py

# With embedder (also runs semantic search queries at the end)
python import_wiki_deep.py --with-embedder
```

The script runs in 4 phases:

**Phase 1: Seed articles** (14 articles) -- Programming languages (Python, Rust, JavaScript, Go, C, Haskell, TypeScript, C++), CS concepts (machine learning, knowledge graph, operating system, compiler), and organizations (Mozilla, Google). Each gets Wikipedia metadata, categories, paradigm links, creator links, influence chains, and extracted facts.

**Phase 2: Follow-up articles** (16 articles) -- Related topics discovered from Phase 1: artificial intelligence, neural network, deep learning, LLVM, Linux, graph database, semantic web, RDF, type system, garbage collection, memory safety, concurrency, lambda calculus, Turing machine, algorithm, data structure.

**Phase 3: Cross-linking** (28 statements) -- Natural language assertions that connect domains: "Rust was developed at Mozilla", "LLVM is used by Rust", "Python is popular for machine learning", "TypeScript is a superset of JavaScript", etc.

**Phase 4: Deep enrichment** (200+ relationships) -- This is what makes the graph interesting at depth 3-4:

- **Creators**: work history (Guido van Rossum -> Google, Dropbox, Microsoft), education (Dennis Ritchie -> Harvard), birthplaces, other creations (Anders Hejlsberg -> TypeScript, Turbo Pascal, C#)
- **Paradigm interconnections**: functional programming -> lambda calculus, supports immutability/higher-order functions/pattern matching; OOP -> inheritance/polymorphism/encapsulation; concurrent programming -> threads/message passing, challenges deadlock/race conditions
- **CS concept hierarchies**: machine learning -> deep learning -> neural network -> biological neuron; compiler -> lexical analysis/parsing/semantic analysis/code generation/optimization; LLVM -> Chris Lattner, University of Illinois
- **Operating system internals**: OS -> process management/memory management/file system; Linux -> Linus Torvalds, kernel, GPL
- **Type systems**: static typing -> used by Rust/Go/TypeScript/Haskell/C/C++; type inference -> Hindley-Milner
- **Memory safety chains**: memory safety -> prevents buffer overflow/use after free/null pointer dereference, enforced by borrow checker -> part of Rust
- **Concurrency models**: concurrency -> actor model (Erlang), CSP (Go, Tony Hoare), shared memory; threads -> managed by OS
- **Foundational CS**: lambda calculus -> Alonzo Church -> Princeton; Turing machine -> Alan Turing -> Bletchley Park
- **Organizations**: Google -> Go, TensorFlow, Chromium, Android, Larry Page, Sergey Brin; Microsoft -> TypeScript, Visual Studio Code, Azure, Bill Gates

### Expected output

```
Final graph: 638 nodes, 377 edges (974 total API calls)
```

### Step 3: Explore in the web UI

Open `http://127.0.0.1:3030/` in your browser. The Graph tab lets you explore interactively:

- Search for "Python" and set depth to 4 -- you'll see a rich network spanning from Python through Guido van Rossum to Google/Dropbox/Microsoft, through paradigms to lambda calculus and Turing machines, through typing to borrow checkers and garbage collection
- Search for "compiler" at depth 3 -- see phases (lexical analysis, parsing, code generation), LLVM connections to Rust/C++/Swift, Chris Lattner, University of Illinois
- Search for "machine learning" at depth 3 -- follow the hierarchy through deep learning, neural networks, down to biological neurons, and up through AI to NLP and computer vision
- Adjust the Min Confidence slider to filter out lower-confidence relationships

### Step 4: Query the graph

#### Keyword search
```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query": "functional programming", "limit": 5}'
```

#### Property filter -- find all statically typed languages
```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query": "prop:typing=static typing", "limit": 10}'
```

#### Boolean search
```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query": "type:programming_language AND confidence>0.85", "limit": 10}'
```

#### Graph traversal -- 2 hops from Rust with confidence filter
```bash
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "Rust", "depth": 2, "min_confidence": 0.7}'
```

Returns 57 nodes and 70 edges including Graydon Hoare, Mozilla, Apple, systems programming, memory management, concurrent programming, threads, message passing, and more.

#### Natural language questions
```bash
# Outgoing edges
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What does Python connect to?"}'

# Incoming edges (who uses this paradigm?)
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What connects to functional programming?"}'
# -> Python, Rust, JavaScript, Haskell, TypeScript
```

#### Semantic search (requires embedder)
```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query": "memory safe programming", "limit": 5}'
# -> memory safety, memory management, Memory safety
```

#### JSON-LD export
```bash
curl -s http://127.0.0.1:3030/export/jsonld | python -m json.tool | head -30
```

Exports all 638 entities as JSON-LD, consumable by any RDF-aware system.

---

### Key Takeaways

- **Engram stores what you tell it.** Wikipedia article content is not magically parsed; the import script does the entity extraction and relationship definition. Engram handles storage, indexing, traversal, and search.
- **Depth matters.** A flat graph (just languages and paradigms) looks unimpressive at depth 3-4. Adding creator biographies, concept hierarchies, and paradigm interconnections makes traversal genuinely useful.
- **Embedder is optional but valuable.** Without it, you get fast keyword search. With it, you get conceptual queries ("memory safe programming" finds Rust's memory safety features even without exact word matches). Ingestion takes ~2x longer due to embedding calls.
- **638 nodes, 377 edges in a single `.brain` file.** No external database, no configuration, no schema definition. The file is fully portable -- copy it to another machine and query it immediately.
- **Performance.** 974 API calls in 81 seconds with embedder, 19 seconds without. The bottleneck is network I/O (Wikipedia API + Ollama), not engram.
