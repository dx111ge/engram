# Use Case 9: Web Search Knowledge Import

### Overview

Building a knowledge base from web search results lets you progressively accumulate structured knowledge from unstructured web pages. This walkthrough uses simulated search results (no external API needed) to demonstrate the pattern: search, extract facts, store in engram, deduplicate via case-insensitive matching, and build confidence through corroboration across multiple sources.

**What this demonstrates:**

- Progressive knowledge building (search -> extract -> store -> reinforce)
- Case-insensitive deduplication (Rust/RUST/rust -> one node)
- Fact extraction from snippets via regex + `/tell` NL interface
- Confidence grows with corroboration from multiple sources
- Source provenance tracks which web domain provided each fact
- Label overflow: document titles longer than 47 bytes stored via property region
- Graph traversal from central entities
- Decay for freshness maintenance

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)
- For production use: a web search API (SearXNG, Brave, SerpAPI)

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed
- No external API keys needed (uses simulated search results)

### Files

```
09-web-search-import/
  README.md              # This file
  web_search_demo.py     # Full demo with simulated search results
  web_search_import.py   # Template for real search API integration
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve websearch.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python web_search_demo.py
```

### What Happens

#### Phase 1: First Search Pass

Simulated search for "Rust programming language" returns 3 results from Wikipedia, rust-lang.org, and Stack Overflow Blog. Facts extracted via regex:

```
Rust is a systems programming language
Rust is a compiled language (reinforced -- seen in multiple results)
```

After pass 1: **7 nodes, 5 edges** (3 doc nodes, 1 topic, extracted entities + relationships).

#### Phase 2: Iterative Deepening

Two more search passes for related topics:
- "Rust ownership model" -- 2 results, extracts "Ownership is a set of rules", "Rust is a systems language"
- "Rust cargo package manager" -- 2 results (no simple "X is a Y" facts extracted)

After 3 passes: **16 nodes, 11 edges**.

#### Phase 3: Case-Insensitive Deduplication

Three `/tell` calls with different casings all merge into one node:

```python
tell("Rust is a programming language", source="web:wikipedia.org")
tell("RUST is a systems language", source="web:reddit.com")
tell("rust is a compiled language", source="web:stackoverflow.com")
```

Result: single "Rust" node at confidence 0.90.

#### Phase 4: Confidence as Corroboration Signal

Entities mentioned by many sources accumulate higher confidence:

```
Rust: 0.90 (mentioned across many results)
```

Text search for "programming language" returns related entities ranked by confidence:

```
Rust-programming-language: conf=0.80
programming language: conf=0.80
systems programming language: conf=0.80
doc:Rust (programming language) - Wikipedia: conf=0.60
doc:The Rust Programming Language - rust-lang.org: conf=0.60
```

Document labels now use full titles (e.g. `doc:The Rust Programming Language - rust-lang.org`, 49 bytes) -- labels longer than 47 bytes overflow to the property region automatically.

#### Phase 5: Graph Traversal

Traversing from "Rust" (depth=2) shows all extracted relationships:

```
Rust (depth=0, conf=0.90)
  compiled language (depth=1, conf=0.80)
  programming language (depth=1, conf=0.80)
  systems language (depth=1, conf=0.80)
  systems programming language (depth=1, conf=0.80)
```

#### Phase 6: Decay

Decay returns 0 (just stored). In production, run `/learn/decay` daily via cron to let stale knowledge fade.

#### Phase 7: Explainability

```
Confidence: 0.90
Outgoing edges (6):
  -[is_a]-> systems programming language (conf=0.8)
  -[is_a]-> compiled language (conf=0.8)
  -[is_a]-> systems language (conf=0.8)
  -[is_a]-> programming language (conf=0.8)
  -[is_a]-> systems language (conf=0.8)
  ... and 1 more
Incoming edges (0)
```

Final graph: **17 nodes, 14 edges**.

### Adapting for Real Search APIs

The demo uses simulated results. To connect to a real search API, replace the `SIMULATED_SEARCHES` dict with actual API calls:

**SearXNG** (self-hosted, free):
```python
def search_web(query, num_results=10):
    resp = requests.get("http://localhost:8888/search", params={
        "q": query, "format": "json", "categories": "general"
    })
    return resp.json().get("results", [])[:num_results]
```

**Brave Search** (free tier: 2,000 queries/month):
```python
def search_web(query, num_results=10):
    resp = requests.get("https://api.search.brave.com/res/v1/web/search",
        headers={"X-Subscription-Token": API_KEY},
        params={"q": query, "count": num_results})
    results = resp.json().get("web", {}).get("results", [])
    return [{"title": r["title"], "content": r.get("description", ""),
             "url": r["url"]} for r in results]
```

### Key Takeaways

- **Progressive knowledge building** works naturally. Each search pass adds entities, and entities mentioned across multiple results get reinforced automatically.
- **Case-insensitive deduplication** prevents duplicate nodes when different sources capitalize differently.
- **Store deduplicates automatically**: calling `/store` with an existing label returns the existing node ID instead of creating a duplicate.
- **Label overflow**: labels longer than 47 bytes are stored in the property region under `_label`. The inline buffer holds a prefix for display, but all lookups, traversals, and searches use the full label.
- **Source provenance** via the `source` parameter tracks which web domain provided each fact, enabling trust assessment.
- **Confidence as corroboration**: entities mentioned by many sources accumulate higher confidence than one-off mentions.
- **Decay keeps knowledge fresh**: running `/learn/decay` periodically ensures that knowledge not reinforced by recent searches gradually fades.
- **Simple regex extraction** + engram's `/tell` NL interface is enough to build useful knowledge graphs from search snippets. For higher-quality extraction, see [Use Case 10: NER-Based Entity Extraction](../10-ner-entity-extraction/).
