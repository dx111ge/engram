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

def daily_import():
    """Run as a scheduled task to continuously build knowledge."""
    TOPICS = [
        "Rust programming language news",
        "Rust security advisories",
        "Rust new crate releases",
    ]

    for topic in TOPICS:
        import_search_results(topic)

    # Apply decay so stale knowledge loses confidence
    requests.post(f"{ENGRAM_API}/learn/decay")

    stats = requests.get(f"{ENGRAM_API}/stats").json()
    print(f"Daily import complete: {stats['nodes']} nodes, "
          f"{stats['edges']} edges")

if __name__ == "__main__":
    # Run the first search pass
    import_search_results("Rust programming language", topic_type="language")

    # Check what we have so far
    stats = requests.get(f"{ENGRAM_API}/stats").json()
    print(f"Knowledge base: {stats['nodes']} nodes, {stats['edges']} edges")

    # Iterative deepening
    import_search_results("Rust ownership model")
    import_search_results("Rust cargo package manager")
    import_search_results("Rust vs Go performance")
