#!/usr/bin/env python3
"""
Use Case 9: Web Search Knowledge Import

Demonstrates progressive knowledge building from web search results.
Uses simulated search results (no external API needed) to show the pattern:
search -> extract -> store -> deduplicate -> reinforce.

Usage:
  engram serve websearch.brain 127.0.0.1:3030
  python web_search_demo.py
"""

import json
import re
import sys
import requests

ENGRAM = "http://127.0.0.1:3030"


def api(method, path, payload=None):
    url = f"{ENGRAM}{path}"
    if method == "GET":
        r = requests.get(url, timeout=10)
    elif method == "POST":
        r = requests.post(url, json=payload, timeout=10)
    else:
        raise ValueError(f"Unknown method: {method}")
    r.raise_for_status()
    return r.json()


def store(entity, entity_type=None, properties=None, confidence=None, source=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = {k: str(v) for k, v in properties.items()}
    if confidence is not None:
        payload["confidence"] = confidence
    if source:
        payload["source"] = source
    return api("POST", "/store", payload)


def relate(from_e, rel, to_e, confidence=None):
    payload = {"from": from_e, "relationship": rel, "to": to_e}
    if confidence is not None:
        payload["confidence"] = confidence
    return api("POST", "/relate", payload)


def tell(statement, source=None):
    payload = {"statement": statement}
    if source:
        payload["source"] = source
    return api("POST", "/tell", payload)


def section(title):
    print(f"\n{'=' * 60}")
    print(f"  {title}")
    print(f"{'=' * 60}")


def subsection(title):
    print(f"\n--- {title} ---")


# Simulated search results (no external API needed)
SIMULATED_SEARCHES = {
    "Rust programming language": [
        {
            "title": "Rust (programming language) - Wikipedia",
            "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            "content": "Rust is a general-purpose programming language emphasizing performance, type safety, and concurrency. It enforces memory safety without a garbage collector."
        },
        {
            "title": "The Rust Programming Language - rust-lang.org",
            "url": "https://www.rust-lang.org/",
            "content": "Rust is a systems programming language focused on safety, speed, and concurrency. Rust was originally designed by Graydon Hoare at Mozilla Research."
        },
        {
            "title": "Why Rust? - Stack Overflow Blog",
            "url": "https://stackoverflow.blog/2024/rust-why/",
            "content": "Rust is a compiled language that offers C-level performance without memory bugs. The ownership system prevents data races at compile time."
        },
    ],
    "Rust ownership model": [
        {
            "title": "Understanding Ownership - The Rust Book",
            "url": "https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html",
            "content": "Ownership is a set of rules that govern how Rust manages memory. Each value in Rust has an owner. There can only be one owner at a time."
        },
        {
            "title": "Rust Ownership Explained - freeCodeCamp",
            "url": "https://www.freecodecamp.org/news/rust-ownership/",
            "content": "Rust is a systems language that uses ownership instead of garbage collection. The borrow checker enforces ownership rules at compile time."
        },
    ],
    "Rust cargo package manager": [
        {
            "title": "Cargo - The Rust Package Manager",
            "url": "https://doc.rust-lang.org/cargo/",
            "content": "Cargo is the Rust package manager. It downloads dependencies, compiles packages, and publishes to crates.io. Cargo is included with Rust."
        },
        {
            "title": "Getting Started with Cargo",
            "url": "https://doc.rust-lang.org/cargo/getting-started/",
            "content": "Cargo is a build system and package manager for Rust. Cargo handles downloading libraries, compiling code, and managing dependencies."
        },
    ],
}


def extract_facts(snippet):
    """Extract simple 'X is a Y' facts from a snippet."""
    facts = []
    for match in re.finditer(
        r'([A-Z][a-zA-Z]{1,20})\s+is\s+(?:a|an)\s+([a-zA-Z\s\-]{2,30}?)(?:\.|,|\s+that|\s+which|\s+focused|\s+emph)',
        snippet
    ):
        subj = match.group(1).strip()
        obj = match.group(2).strip()
        if len(subj.split()) <= 2 and len(obj.split()) <= 4:
            facts.append((subj, obj))
    return facts


def import_search_results(query):
    """Import simulated search results into engram."""
    results = SIMULATED_SEARCHES.get(query, [])
    if not results:
        print(f"  No results for: {query}")
        return 0

    entities_seen = set()
    facts_total = 0

    for i, result in enumerate(results):
        title = result["title"]
        url = result["url"]
        snippet = result["content"]
        domain = url.split("/")[2]

        print(f"\n  [{i+1}] {title}")
        print(f"      {domain}")

        # Store as document node (overflow labels stored in property region)
        doc_label = f"doc:{title}"
        store(doc_label, "search_result", {
            "url": url,
            "source_domain": domain,
            "query": query,
        }, confidence=0.60, source=f"web:{domain}")

        # Relate doc to query topic
        topic_label = query.replace(" ", "-")
        store(topic_label, "topic", {"query": query}, confidence=0.80)
        relate(doc_label, "about", topic_label, 0.50)

        # Extract facts from snippet
        facts = extract_facts(snippet)
        for subj, obj in facts:
            print(f"      -> {subj} is a {obj}")
            tell(f"{subj} is a {obj}", source=f"web:{domain}")

            subj_lower = subj.lower()
            if subj_lower in entities_seen:
                api("POST", "/learn/reinforce", {
                    "entity": subj, "source": f"web:{domain}"
                })
                print(f"         (reinforced)")
            entities_seen.add(subj_lower)
            facts_total += 1

    return facts_total


def main():
    try:
        health = api("GET", "/health")
        print(f"Server: {health}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve websearch.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: First Search Pass -- 'Rust programming language'")
    # ================================================================

    facts = import_search_results("Rust programming language")
    print(f"\n  Facts extracted: {facts}")

    stats = api("GET", "/stats")
    print(f"  Graph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Iterative Deepening -- Related Topics")
    # ================================================================

    subsection("Search: 'Rust ownership model'")
    import_search_results("Rust ownership model")

    subsection("Search: 'Rust cargo package manager'")
    import_search_results("Rust cargo package manager")

    stats = api("GET", "/stats")
    print(f"\n  Graph after 3 search passes: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 3: Case-Insensitive Deduplication")
    # ================================================================

    subsection("Multiple casings resolve to same node")
    tell("Rust is a programming language", source="web:wikipedia.org")
    tell("RUST is a systems language", source="web:reddit.com")
    tell("rust is a compiled language", source="web:stackoverflow.com")

    node = api("GET", "/node/Rust")
    print(f"  Node label: {node['label']}")
    print(f"  Confidence: {node['confidence']:.2f}")
    print(f"  (3 different casings all merged into one node)")

    # ================================================================
    section("PHASE 4: Confidence as Corroboration Signal")
    # ================================================================

    subsection("Well-supported entities (mentioned by multiple sources)")
    # Rust was mentioned by many sources -- check its confidence
    node = api("GET", "/node/Rust")
    print(f"  Rust: {node['confidence']:.2f} (mentioned across many results)")

    subsection("Search for entities by text")
    result = api("POST", "/search", {"query": "programming language", "limit": 5})
    hits = result.get("results", [])
    for h in hits:
        print(f"  {h['label']}: conf={h['confidence']:.2f}")

    subsection("Search for all search_result documents")
    result = api("POST", "/search", {"query": "type:search_result", "limit": 20})
    hits = result.get("results", [])
    print(f"  Documents imported: {len(hits)}")

    # ================================================================
    section("PHASE 5: Graph Traversal from Central Entity")
    # ================================================================

    subsection("Traverse from 'Rust' (depth=2)")
    result = api("POST", "/query", {
        "start": "Rust",
        "depth": 2,
        "min_confidence": 0.0,
    })
    nodes = result.get("nodes", [])
    print(f"  Reachable: {len(nodes)} nodes")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"]))[:15]:
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")
    if len(nodes) > 15:
        print(f"    ... and {len(nodes) - 15} more")

    # ================================================================
    section("PHASE 6: Decay and Maintenance")
    # ================================================================

    subsection("Apply decay (simulating daily maintenance)")
    result = api("POST", "/learn/decay")
    print(f"  Nodes decayed: {result.get('nodes_decayed', '?')}")
    print(f"  (0 days elapsed -- in production, run daily via cron)")

    # ================================================================
    section("PHASE 7: Explainability")
    # ================================================================

    subsection("Explain: Rust")
    resp = api("GET", "/explain/Rust")
    print(f"  Confidence: {resp.get('confidence', '?'):.2f}")
    edges_from = resp.get("edges_from", [])
    edges_to = resp.get("edges_to", [])
    print(f"  Outgoing edges ({len(edges_from)}):")
    for e in edges_from[:5]:
        print(f"    -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")
    if len(edges_from) > 5:
        print(f"    ... and {len(edges_from) - 5} more")
    print(f"  Incoming edges ({len(edges_to)}):")
    for e in edges_to[:5]:
        print(f"    {e['from']} -[{e['relationship']}]-> (conf={e.get('confidence', '?')})")
    if len(edges_to) > 5:
        print(f"    ... and {len(edges_to) - 5} more")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"\n  Web search import pattern demonstrated:")
    print(f"    - Progressive knowledge building (search -> extract -> store)")
    print(f"    - Case-insensitive deduplication (Rust/RUST/rust -> one node)")
    print(f"    - Fact extraction from snippets via regex + /tell NL interface")
    print(f"    - Confidence grows with corroboration from multiple sources")
    print(f"    - Source provenance tracks which web domain provided each fact")
    print(f"    - Decay keeps the knowledge fresh over time")


if __name__ == "__main__":
    main()
