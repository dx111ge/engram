# Use Case 9: Web Search Knowledge Import

### Overview

Building a knowledge base from web search results lets you progressively accumulate structured knowledge from unstructured web pages. This walkthrough uses a search API to discover information about a topic, imports the results into engram, deduplicates entities, and builds a growing knowledge graph through iterative search-and-import cycles.

**What this demonstrates:**
- Progressive knowledge building (search, import, search deeper, import more)
- Deduplication via case-insensitive label matching
- Source tracking for web-sourced information
- Confidence scoring based on how many search results corroborate a fact
- Natural language interface for quick knowledge entry from snippets

### Prerequisites

- engram binary
- Python 3.8+ with `requests`
- A web search API. Options:
  - **SearXNG** (self-hosted, free, no API key) -- recommended
  - **Brave Search API** (free tier: 2,000 queries/month)
  - **SerpAPI** (free tier: 100 queries/month)

This walkthrough uses SearXNG as the example, but the pattern works with any search API that returns JSON results.

### Step 1: Create the Knowledge Base

```bash
engram create websearch.brain
engram serve websearch.brain 127.0.0.1:3030
```

### Step 2: Web Search Collector Script

Full script: [web_search_import.py](web_search_import.py)

```python
import requests
import re
import json

ENGRAM_API = "http://127.0.0.1:3030"
# SearXNG instance (self-hosted or public)
SEARCH_API = "http://localhost:8888/search"

def search_web(query, num_results=10):
    """Search via SearXNG and return results."""
    resp = requests.get(SEARCH_API, params={
        "q": query,
        "format": "json",
        "categories": "general",
        "language": "en",
        "pageno": 1
    })
    if resp.status_code != 200:
        print(f"Search failed: {resp.status_code}")
        return []
    data = resp.json()
    return data.get("results", [])[:num_results]

def store_entity(entity, entity_type=None, props=None, source="web-search",
                 confidence=0.60):
    """Store an entity in engram. Returns the response."""
    body = {"entity": entity, "source": source, "confidence": confidence}
    if entity_type:
        body["type"] = entity_type
    if props:
        body["properties"] = props
    return requests.post(f"{ENGRAM_API}/store", json=body).json()

def relate(from_e, to_e, rel, confidence=0.60):
    body = {"from": from_e, "to": to_e, "relationship": rel,
            "confidence": confidence}
    return requests.post(f"{ENGRAM_API}/relate", json=body).json()

def tell(statement, source="web-search"):
    """Use natural language interface to store a fact."""
    return requests.post(f"{ENGRAM_API}/tell", json={
        "statement": statement, "source": source
    }).json()

def reinforce(entity, source="web-search"):
    return requests.post(f"{ENGRAM_API}/learn/reinforce", json={
        "entity": entity, "source": source
    }).json()

def extract_facts_from_snippet(snippet):
    """Extract simple 'X is a Y' facts from a search snippet.
    Returns list of (subject, object) tuples."""
    facts = []
    # Match patterns like "X is a Y", "X is an Y"
    for match in re.finditer(
        r'([A-Z][a-zA-Z\s]{1,30})\s+is\s+(?:a|an)\s+([a-zA-Z\s]{2,40})',
        snippet
    ):
        subj = match.group(1).strip()
        obj = match.group(2).strip()
        # Filter out overly long or generic matches
        if len(subj.split()) <= 4 and len(obj.split()) <= 5:
            facts.append((subj, obj))
    return facts
```

### Step 3: Search and Import -- First Pass

```python
def import_search_results(query, topic_type=None):
    """Search the web and import results into engram."""
    print(f"\n--- Searching: '{query}' ---")
    results = search_web(query, num_results=10)

    if not results:
        print("No results found")
        return

    entities_seen = set()

    for i, result in enumerate(results):
        title = result.get("title", "")
        snippet = result.get("content", "")
        url = result.get("url", "")
        source_domain = url.split("/")[2] if "/" in url else "unknown"

        print(f"\n[{i+1}] {title}")
        print(f"    {url}")

        # Store the search result as a document node
        doc_label = f"doc:{title[:60]}"
        store_entity(doc_label, "search_result", {
            "url": url,
            "source_domain": source_domain,
            "query": query,
            "snippet": snippet[:200]
        }, source=f"web:{source_domain}", confidence=0.60)

        # Extract facts from the snippet
        facts = extract_facts_from_snippet(snippet)
        for subj, obj in facts:
            print(f"    Fact: {subj} is a {obj}")

            # Use the NL interface for natural "is a" relationships
            tell(f"{subj} is a {obj}", source=f"web:{source_domain}")

            # Track entity for dedup
            subj_lower = subj.lower()
            if subj_lower in entities_seen:
                # Entity seen in multiple results -- reinforce
                reinforce(subj, source=f"web:{source_domain}")
                print(f"    (reinforced: {subj})")
            entities_seen.add(subj_lower)

        # Relate document to the search topic
        if topic_type:
            relate(doc_label, query, "about", confidence=0.50)

    print(f"\nImported {len(results)} results, "
          f"{len(entities_seen)} unique entities")

# -- Run the first search pass --

import_search_results("Rust programming language", topic_type="language")
```

### Step 4: Iterative Deepening

```python
# After the first pass, search for related topics discovered in results

# Check what we have so far
stats = requests.get(f"{ENGRAM_API}/stats").json()
print(f"Knowledge base: {stats['nodes']} nodes, {stats['edges']} edges")

# Search for topics mentioned in the first pass
import_search_results("Rust ownership model")
import_search_results("Rust cargo package manager")
import_search_results("Rust vs Go performance")

# Each pass adds more entities and edges. Entities mentioned in
# multiple search results get reinforced automatically.
```

### Step 5: Deduplication Through Case-Insensitive Matching

Engram's case-insensitive label matching handles a common web import problem automatically:

```python
# These all resolve to the same node:
tell("Rust is a programming language", source="web:wikipedia.org")
tell("RUST is a systems language", source="web:reddit.com")
tell("rust is a compiled language", source="web:stackoverflow.com")

# Query any casing -- they all find the same node
resp = requests.get(f"{ENGRAM_API}/node/rust")
print(f"Node: {resp.json()['label']}")  # Original label preserved
print(f"Confidence: {resp.json()['confidence']}")  # Boosted by 3 stores
```

### Step 6: Progressive Confidence Building

```python
# After multiple search passes, check which entities are well-established
# (mentioned across many sources) vs weakly supported

# Well-supported entities (mentioned by 3+ sources)
resp = requests.post(f"{ENGRAM_API}/search", json={
    "query": "confidence>0.7", "limit": 20
})
print("Well-supported entities:")
for hit in resp.json()["results"]:
    print(f"  {hit['label']}: {hit['confidence']:.2f}")

# Weakly supported (single source)
resp = requests.post(f"{ENGRAM_API}/search", json={
    "query": "confidence<0.5", "limit": 20
})
print("\nNeeds more evidence:")
for hit in resp.json()["results"]:
    print(f"  {hit['label']}: {hit['confidence']:.2f}")
```

### Step 7: Query the Accumulated Knowledge

```bash
# Full-text search across all imported content
engram search "ownership" websearch.brain
engram search "performance" websearch.brain

# Find all entities from a specific source
engram search "prop:source_domain=wikipedia.org" websearch.brain

# Traverse from a central entity
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "Rust", "depth": 2, "min_confidence": 0.5}'
```

### Step 8: Scheduled Import for Continuous Knowledge Building

```python
# Run as a scheduled task (cron / Windows Task Scheduler)
# to continuously build knowledge

TOPICS = [
    "Rust programming language news",
    "Rust security advisories",
    "Rust new crate releases",
]

def daily_import():
    for topic in TOPICS:
        import_search_results(topic)

    # Apply decay so stale knowledge loses confidence
    requests.post(f"{ENGRAM_API}/learn/decay")

    stats = requests.get(f"{ENGRAM_API}/stats").json()
    print(f"Daily import complete: {stats['nodes']} nodes, "
          f"{stats['edges']} edges")

daily_import()
```

### Alternative: Using Brave Search API

If you prefer a hosted search API with no self-hosting:

```python
BRAVE_API_KEY = "your-api-key-here"

def search_brave(query, num_results=10):
    """Search via Brave Search API."""
    resp = requests.get("https://api.search.brave.com/res/v1/web/search",
        headers={"X-Subscription-Token": BRAVE_API_KEY},
        params={"q": query, "count": num_results}
    )
    if resp.status_code != 200:
        return []
    results = resp.json().get("web", {}).get("results", [])
    # Normalize to same format as SearXNG
    return [{"title": r["title"], "content": r.get("description", ""),
             "url": r["url"]} for r in results]
```

### Key Takeaways

- **Progressive knowledge building** works naturally with engram. Each search pass adds entities, and entities mentioned across multiple results get reinforced automatically.
- **Case-insensitive deduplication** prevents duplicate nodes when different sources capitalize differently ("Rust" vs "RUST" vs "rust").
- **Source tracking** via provenance lets you trace every fact back to the web page that provided it.
- **Confidence as corroboration signal.** Entities mentioned by many sources accumulate higher confidence than one-off mentions. This naturally separates well-established facts from noise.
- **Decay keeps the knowledge fresh.** Running `learn/decay` periodically ensures that knowledge not reinforced by recent searches gradually loses confidence, preventing stale facts from dominating results.
- **No NLP required.** The simple regex extraction plus engram's `/tell` natural language interface is enough to build useful knowledge graphs from search snippets without heavyweight NLP dependencies. For higher-quality extraction, see [Use Case 10: NER-Based Entity Extraction](../10-ner-entity-extraction/).
