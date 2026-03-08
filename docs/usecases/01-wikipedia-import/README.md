# Use Case 1: Building a Knowledge Base from Wikipedia

### Overview

Wikipedia exposes a public REST API that returns article summaries in plain JSON — no authentication, no rate-limit keys for casual use. Engram's HTTP API accepts JSON over standard HTTP. Together, they let you build a factual knowledge graph from Wikipedia content in under fifty lines of Python.

This walkthrough imports four programming language articles (Python, Rust, JavaScript, Go), extracts "is a" facts from their summaries, relates the languages to their paradigms, and then traverses the resulting graph. You get a queryable, persistent graph in a single `.brain` file without any database server.

**What this demonstrates today (v0.1.0):**

- The `/tell` natural-language endpoint parsing "X is a Y" and "X uses Y" patterns
- The `/store` and `/relate` HTTP endpoints for bulk ingestion
- BM25 full-text search across imported entities
- Graph traversal with configurable depth and minimum confidence
- The `/ask` endpoint for natural-language queries over imported data

**What requires external tools:**

- Wikipedia API access requires Python + `requests` (or any HTTP client)
- Entity extraction from free text requires your own regex or NLP pipeline; engram stores what you give it and does not extract entities from raw paragraphs on its own

### Prerequisites

- `engram` binary on your PATH
- Python 3.9+ with `requests` installed (`pip install requests`)
- Internet access for the Wikipedia API
- (Optional) An embedding model for semantic search — see **Embedder Setup** below

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

That's it. Engram auto-detects the embedding dimension by sending a probe request at startup. You do **not** need to set `ENGRAM_EMBED_DIM` unless you want to override the default dimension (some Matryoshka models like nomic-embed-text-v2-moe support multiple dimensions: 256, 512, 768).

| Variable | Required | Default | Description |
|---|---|---|---|
| `ENGRAM_EMBED_ENDPOINT` | Yes | — | Base URL of the embeddings API (must serve `/embeddings`) |
| `ENGRAM_EMBED_MODEL` | No | `multilingual-e5-small` | Model name passed in the API request |
| `ENGRAM_EMBED_DIM` | No | Auto-detected | Override embedding dimension (auto-probe if not set) |
| `ENGRAM_EMBED_API_KEY` | No | — | API key for authenticated endpoints (OpenAI, Azure, etc.) |

**Compatible APIs**: Ollama, OpenAI, vLLM, LiteLLM, text-embeddings-inference, Azure OpenAI — anything that serves the OpenAI `/v1/embeddings` format.

### Step-by-Step Implementation

#### Step 1: Start the engram server

Without embedder (keyword search only):
```bash
engram serve wiki.brain 127.0.0.1:3030
```

With embedder (keyword + semantic search):
```bash
ENGRAM_EMBED_ENDPOINT=http://localhost:11434/v1 \
ENGRAM_EMBED_MODEL=nomic-embed-text-v2-moe:latest \
engram serve wiki.brain 127.0.0.1:3030
```

Expected output:

```
engram API listening on 127.0.0.1:3030
```

Leave this running. Open a second terminal for the following steps.

#### Step 2: Verify the server is healthy

```bash
curl -s http://127.0.0.1:3030/health
```

Expected output:

```json
{"status":"ok","version":"0.1.0"}
```

#### Step 3: Write the Wikipedia import script

Full script: [import_wiki.py](import_wiki.py)

Save the following as `import_wiki.py`:

```python
#!/usr/bin/env python3
"""
Import Wikipedia article summaries into engram.

Uses the Wikipedia REST API (no auth required):
  https://en.wikipedia.org/api/rest_v1/page/summary/{title}

For each article:
  1. Fetch the summary JSON
  2. Store the article title as a node with properties
  3. Use /tell to assert "X is a Y" facts found in the extract
  4. Create explicit relationships to paradigms and creators
"""

import re
import requests
import time

ENGRAM = "http://127.0.0.1:3030"
WIKI   = "https://en.wikipedia.org/api/rest_v1/page/summary"

# Articles to import with their known relationships
ARTICLES = [
    {
        "title": "Python_(programming_language)",
        "label": "Python",
        "paradigms": ["object-oriented programming", "functional programming", "imperative programming"],
        "creator": "Guido van Rossum",
        "typing": "dynamic typing",
    },
    {
        "title": "Rust_(programming_language)",
        "label": "Rust",
        "paradigms": ["systems programming", "functional programming", "concurrent programming"],
        "creator": "Graydon Hoare",
        "typing": "static typing",
    },
    {
        "title": "JavaScript",
        "label": "JavaScript",
        "paradigms": ["event-driven programming", "functional programming", "object-oriented programming"],
        "creator": "Brendan Eich",
        "typing": "dynamic typing",
    },
    {
        "title": "Go_(programming_language)",
        "label": "Go",
        "paradigms": ["concurrent programming", "imperative programming"],
        "creator": "Rob Pike",
        "typing": "static typing",
    },
]

def store(entity, entity_type=None, properties=None, confidence=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = properties
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/store", json=payload, timeout=5)
    r.raise_for_status()
    return r.json()

def relate(from_entity, relationship, to_entity, confidence=None):
    payload = {"from": from_entity, "relationship": relationship, "to": to_entity}
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/relate", json=payload, timeout=5)
    r.raise_for_status()
    return r.json()

def tell(statement, source=None):
    payload = {"statement": statement}
    if source:
        payload["source"] = source
    r = requests.post(f"{ENGRAM}/tell", json=payload, timeout=5)
    r.raise_for_status()
    return r.json()

def fetch_wiki_summary(title):
    url = f"{WIKI}/{title}"
    r = requests.get(url, headers={"User-Agent": "engram-demo/1.0"}, timeout=10)
    r.raise_for_status()
    return r.json()

def extract_is_a_facts(label, extract):
    """
    Very simple rule-based extraction: find sentences matching
    '<label> is a ...' or '<label> is an ...' in the first 200 chars.
    Returns a list of (subject, predicate) pairs suitable for /tell.
    """
    facts = []
    # Normalize whitespace
    text = " ".join(extract.split())
    # Match "Label is a/an <noun phrase ending at period or comma>"
    pattern = re.compile(
        rf"{re.escape(label)}\s+is\s+(?:a|an)\s+([^,.;]+)",
        re.IGNORECASE,
    )
    for m in pattern.finditer(text[:300]):
        predicate = m.group(1).strip().rstrip(".")
        # Truncate to first two words to avoid over-long predicates
        short_pred = " ".join(predicate.split()[:3])
        if short_pred:
            facts.append((label, short_pred))
    return facts

def main():
    print(f"Importing {len(ARTICLES)} programming languages into engram...\n")

    for article in ARTICLES:
        label = article["label"]
        print(f"--- {label} ---")

        # Fetch Wikipedia summary
        try:
            data = fetch_wiki_summary(article["title"])
        except Exception as e:
            print(f"  Wikipedia fetch failed: {e}")
            continue

        extract = data.get("extract", "")
        description = data.get("description", "")
        print(f"  extract: {extract[:100]}...")

        # Store the language as a node with metadata
        result = store(
            entity=label,
            entity_type="programming_language",
            properties={
                "description": description,
                "wikipedia_title": article["title"],
                "creator": article["creator"],
                "typing": article["typing"],
                "source_url": data.get("content_urls", {}).get("desktop", {}).get("page", ""),
            },
            confidence=0.90,  # Wikipedia API source
        )
        print(f"  stored node id={result['node_id']} confidence={result['confidence']}")

        # Store the creator as a node and relate
        store(entity=article["creator"], entity_type="person", confidence=0.90)
        relate(label, "created_by", article["creator"], confidence=0.90)
        print(f"  related: {label} -[created_by]-> {article['creator']}")

        # Store paradigms and relate
        for paradigm in article["paradigms"]:
            store(entity=paradigm, entity_type="programming_paradigm", confidence=0.90)
            relate(label, "uses", paradigm, confidence=0.90)
            print(f"  related: {label} -[uses]-> {paradigm}")

        # Store typing discipline
        store(entity=article["typing"], entity_type="type_system", confidence=0.90)
        relate(label, "has", article["typing"], confidence=0.90)

        # Use /tell to assert "X is a Y" facts extracted from the summary
        facts = extract_is_a_facts(label, extract)
        for subject, predicate in facts:
            resp = tell(f"{subject} is a {predicate}", source="wikipedia")
            print(f"  told: {resp['interpretation']}")

        # Polite delay between Wikipedia requests
        time.sleep(0.5)

    print("\nImport complete.")

    # Show stats
    r = requests.get(f"{ENGRAM}/stats", timeout=5)
    stats = r.json()
    print(f"Graph: {stats['nodes']} nodes, {stats['edges']} edges")

if __name__ == "__main__":
    main()
```

#### Step 4: Run the import

```bash
python import_wiki.py
```

Expected output (Wikipedia text varies; node IDs and exact counts depend on which facts are extracted):

```
Importing 4 programming languages into engram...

--- Python ---
  extract: Python is a high-level, general-purpose programming language. Its design philos...
  stored node id=1 confidence=0.9
  related: Python -[created_by]-> Guido van Rossum
  related: Python -[uses]-> object-oriented programming
  related: Python -[uses]-> functional programming
  related: Python -[uses]-> imperative programming
  told: Python is a type of high-level general-purpose

--- Rust ---
  extract: Rust is a multi-paradigm, general-purpose programming language that emphasizes ...
  stored node id=5 confidence=0.9
  related: Rust -[created_by]-> Graydon Hoare
  related: Rust -[uses]-> systems programming
  related: Rust -[uses]-> functional programming
  related: Rust -[uses]-> concurrent programming
  told: Rust is a type of multi-paradigm general-purpose

--- JavaScript ---
  extract: JavaScript, often abbreviated as JS, is a programming language and core technol...
  stored node id=9 confidence=0.9
  related: JavaScript -[created_by]-> Brendan Eich
  related: JavaScript -[uses]-> event-driven programming
  related: JavaScript -[uses]-> functional programming
  related: JavaScript -[uses]-> object-oriented programming

--- Go ---
  extract: Go is a statically typed, compiled high-level programming language designed at ...
  stored node id=13 confidence=0.9
  related: Go -[created_by]-> Rob Pike
  related: Go -[uses]-> concurrent programming
  related: Go -[uses]-> imperative programming

Import complete.
Graph: 24 nodes, 22 edges
```

### Querying the Results

#### Search by keyword

```bash
engram search "functional programming" wiki.brain
```

Expected output:

```
Results (4):
  functional programming
  Python
  Rust
  JavaScript
```

The BM25 index ranks the `functional programming` node first (exact match on both tokens), then the three languages that use it.

#### Search with a confidence filter

```bash
engram search "confidence>0.85" wiki.brain
```

Expected output — only the nodes stored with `confidence=0.90`:

```
Results (24):
  Python
  Rust
  JavaScript
  Go
  Guido van Rossum
  ...
```

#### Search with a property filter

```bash
engram search "prop:typing=static typing" wiki.brain
```

Expected output:

```
Results (2):
  Rust
  Go
```

#### Query a node and its direct edges

```bash
engram query Python 1 wiki.brain
```

Expected output:

```
Node: Python
  id: 1
  confidence: 0.90
  memory_tier: active
Properties:
  creator: Guido van Rossum
  description: high-level general-purpose programming language
  typing: dynamic typing
  wikipedia_title: Python_(programming_language)
Edges out:
  Python -[created_by]-> Guido van Rossum (confidence: 0.90)
  Python -[uses]-> object-oriented programming (confidence: 0.90)
  Python -[uses]-> functional programming (confidence: 0.90)
  Python -[uses]-> imperative programming (confidence: 0.90)
  Python -[has]-> dynamic typing (confidence: 0.90)
  Python -[is_a]-> high-level general-purpose (confidence: 0.80)
Reachable (1-hop): 6 nodes
```

#### Ask a natural language question

```bash
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What does Rust connect to?"}'
```

Expected output:

```json
{
  "interpretation": "outgoing edges from: Rust",
  "results": [
    {"label": "Graydon Hoare",          "confidence": 0.9, "relationship": "created_by", "detail": null},
    {"label": "systems programming",    "confidence": 0.9, "relationship": "uses",       "detail": null},
    {"label": "functional programming", "confidence": 0.9, "relationship": "uses",       "detail": null},
    {"label": "concurrent programming", "confidence": 0.9, "relationship": "uses",       "detail": null},
    {"label": "static typing",          "confidence": 0.9, "relationship": "has",        "detail": null}
  ]
}
```

#### Traverse two hops to find shared paradigms

```bash
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "functional programming", "depth": 2, "min_confidence": 0.8}'
```

This returns all nodes reachable from `functional programming` within 2 hops — which includes Python, Rust, and JavaScript, giving you a language-family view from the paradigm perspective.

### Key Takeaways

- Engram stores whatever you tell it. Wikipedia article content is not magically parsed; the import script does the entity extraction. Engram handles storage, indexing, and traversal.
- The `/tell` endpoint handles "X is a Y" patterns reliably. Other patterns in article text require your own extraction logic.
- Property-based filtering (`prop:typing=static typing`) works at search time without pre-defining schemas.
- The entire graph — 24 nodes, 22 edges, full-text index — lives in a single `wiki.brain` file.
