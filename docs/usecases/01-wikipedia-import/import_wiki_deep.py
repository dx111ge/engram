#!/usr/bin/env python3
"""
Deep Wikipedia import into engram.

Goes far beyond the basic 4-language demo:
  - 12+ seed articles across programming languages, CS concepts, and people
  - Follows "See also" links to discover related articles (1 level deep)
  - Extracts multiple fact patterns from article text (is a, was created, designed by, etc.)
  - Imports full article sections via the Wikipedia parse API
  - Creates cross-domain relationships (language -> paradigm -> creator -> institution)
  - Uses /batch for bulk ingestion where possible
  - Stores rich properties: descriptions, dates, URLs, categories

Usage:
  python import_wiki_deep.py                    # keyword search only
  python import_wiki_deep.py --with-embedder    # also tests semantic search
"""

import re
import sys
import json
import requests
import time

ENGRAM = "http://127.0.0.1:3030"
WIKI_SUMMARY = "https://en.wikipedia.org/api/rest_v1/page/summary"
WIKI_PARSE = "https://en.wikipedia.org/w/api.php"
HEADERS = {"User-Agent": "engram-deep-demo/1.0 (knowledge-graph research)"}

# ── Seed articles: broad coverage across CS domains ──

SEED_ARTICLES = [
    # Programming languages
    {
        "title": "Python_(programming_language)",
        "label": "Python",
        "domain": "programming_language",
        "paradigms": ["object-oriented programming", "functional programming",
                      "imperative programming", "structured programming"],
        "creator": "Guido van Rossum",
        "typing": "dynamic typing",
        "first_appeared": "1991",
        "influenced_by": ["C", "Haskell", "Lisp"],
    },
    {
        "title": "Rust_(programming_language)",
        "label": "Rust",
        "domain": "programming_language",
        "paradigms": ["systems programming", "functional programming",
                      "concurrent programming", "generic programming"],
        "creator": "Graydon Hoare",
        "typing": "static typing",
        "first_appeared": "2010",
        "influenced_by": ["C++", "Haskell", "OCaml", "Erlang"],
    },
    {
        "title": "JavaScript",
        "label": "JavaScript",
        "domain": "programming_language",
        "paradigms": ["event-driven programming", "functional programming",
                      "object-oriented programming", "prototype-based programming"],
        "creator": "Brendan Eich",
        "typing": "dynamic typing",
        "first_appeared": "1995",
        "influenced_by": ["Java", "Scheme", "Self"],
    },
    {
        "title": "Go_(programming_language)",
        "label": "Go",
        "domain": "programming_language",
        "paradigms": ["concurrent programming", "imperative programming",
                      "structured programming"],
        "creator": "Rob Pike",
        "typing": "static typing",
        "first_appeared": "2009",
        "influenced_by": ["C", "Oberon", "Limbo"],
    },
    {
        "title": "C_(programming_language)",
        "label": "C",
        "domain": "programming_language",
        "paradigms": ["imperative programming", "structured programming",
                      "systems programming"],
        "creator": "Dennis Ritchie",
        "typing": "static typing",
        "first_appeared": "1972",
        "influenced_by": ["B", "BCPL", "ALGOL"],
    },
    {
        "title": "Haskell_(programming_language)",
        "label": "Haskell",
        "domain": "programming_language",
        "paradigms": ["functional programming", "lazy evaluation",
                      "type inference"],
        "creator": "Lennart Augustsson",
        "typing": "static typing",
        "first_appeared": "1990",
        "influenced_by": ["Miranda", "ML", "Lisp"],
    },
    {
        "title": "TypeScript",
        "label": "TypeScript",
        "domain": "programming_language",
        "paradigms": ["object-oriented programming", "functional programming",
                      "generic programming"],
        "creator": "Anders Hejlsberg",
        "typing": "static typing",
        "first_appeared": "2012",
        "influenced_by": ["JavaScript", "Java", "C#"],
    },
    {
        "title": "C++",
        "label": "C++",
        "domain": "programming_language",
        "paradigms": ["object-oriented programming", "generic programming",
                      "imperative programming", "systems programming"],
        "creator": "Bjarne Stroustrup",
        "typing": "static typing",
        "first_appeared": "1985",
        "influenced_by": ["C", "Simula", "ALGOL"],
    },

    # CS Concepts
    {
        "title": "Machine_learning",
        "label": "machine learning",
        "domain": "cs_concept",
        "related_to": ["artificial intelligence", "neural network", "deep learning",
                       "supervised learning", "unsupervised learning"],
    },
    {
        "title": "Knowledge_graph",
        "label": "knowledge graph",
        "domain": "cs_concept",
        "related_to": ["semantic web", "ontology", "graph database",
                       "linked data", "RDF"],
    },
    {
        "title": "Operating_system",
        "label": "operating system",
        "domain": "cs_concept",
        "related_to": ["kernel", "Linux", "process management",
                       "file system", "memory management"],
    },
    {
        "title": "Compiler",
        "label": "compiler",
        "domain": "cs_concept",
        "related_to": ["LLVM", "parser", "abstract syntax tree",
                       "code generation", "optimization"],
    },

    # Institutions
    {
        "title": "Mozilla",
        "label": "Mozilla",
        "domain": "organization",
        "related_to": ["Firefox", "Rust", "open source"],
    },
    {
        "title": "Google",
        "label": "Google",
        "domain": "organization",
        "related_to": ["Go", "TensorFlow", "Chromium", "Android"],
    },
]

# ── Additional articles discovered via "See also" or known relationships ──

FOLLOW_UP_ARTICLES = [
    "Artificial_intelligence",
    "Neural_network",
    "Deep_learning",
    "LLVM",
    "Linux",
    "Graph_database",
    "Semantic_Web",
    "RDF",
    "Type_system",
    "Garbage_collection_(computer_science)",
    "Memory_safety",
    "Concurrency_(computer_science)",
    "Lambda_calculus",
    "Turing_machine",
    "Algorithm",
    "Data_structure",
]


def api_store(entity, entity_type=None, properties=None, confidence=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = properties
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/store", json=payload, timeout=10)
    r.raise_for_status()
    return r.json()


def api_relate(from_entity, relationship, to_entity, confidence=None):
    payload = {"from": from_entity, "relationship": relationship, "to": to_entity}
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/relate", json=payload, timeout=10)
    r.raise_for_status()
    return r.json()


def api_tell(statement, source=None):
    payload = {"statement": statement}
    if source:
        payload["source"] = source
    r = requests.post(f"{ENGRAM}/tell", json=payload, timeout=10)
    r.raise_for_status()
    return r.json()


def api_ask(question):
    r = requests.post(f"{ENGRAM}/ask", json={"question": question}, timeout=10)
    r.raise_for_status()
    return r.json()


def api_search(query, limit=20):
    r = requests.post(f"{ENGRAM}/search", json={"query": query, "limit": limit}, timeout=10)
    r.raise_for_status()
    return r.json()


def api_query(start, depth=2, min_confidence=0.0):
    r = requests.post(f"{ENGRAM}/query",
                      json={"start": start, "depth": depth, "min_confidence": min_confidence},
                      timeout=10)
    r.raise_for_status()
    return r.json()


def api_batch(operations):
    r = requests.post(f"{ENGRAM}/batch", json={"operations": operations}, timeout=30)
    r.raise_for_status()
    return r.json()


def api_stats():
    r = requests.get(f"{ENGRAM}/stats", timeout=5)
    r.raise_for_status()
    return r.json()


def fetch_wiki_summary(title):
    url = f"{WIKI_SUMMARY}/{title}"
    r = requests.get(url, headers=HEADERS, timeout=10)
    if r.status_code == 404:
        return None
    r.raise_for_status()
    return r.json()


def fetch_wiki_links(title, limit=20):
    """Fetch internal links from a Wikipedia article (top N)."""
    params = {
        "action": "parse",
        "page": title,
        "prop": "links",
        "format": "json",
        "pllimit": str(limit),
    }
    try:
        r = requests.get(WIKI_PARSE, params=params, headers=HEADERS, timeout=10)
        r.raise_for_status()
        data = r.json()
        links = data.get("parse", {}).get("links", [])
        return [l["*"] for l in links if l.get("ns") == 0 and l.get("exists") is not None]
    except Exception:
        return []


def fetch_wiki_categories(title, limit=15):
    """Fetch categories for a Wikipedia article."""
    params = {
        "action": "parse",
        "page": title,
        "prop": "categories",
        "format": "json",
        "cllimit": str(limit),
    }
    try:
        r = requests.get(WIKI_PARSE, params=params, headers=HEADERS, timeout=10)
        r.raise_for_status()
        data = r.json()
        cats = data.get("parse", {}).get("categories", [])
        # Filter out maintenance categories
        return [c["*"].replace("_", " ") for c in cats
                if not any(skip in c["*"].lower() for skip in
                          ["articles", "pages", "webarchive", "short_description",
                           "use_", "all_", "cs1_", "wikipedia", "accuracy"])]
    except Exception:
        return []


def extract_facts(label, text):
    """Extract structured facts from article text using multiple patterns."""
    facts = []
    text = " ".join(text.split())[:500]

    # "X is a/an Y"
    for m in re.finditer(rf"{re.escape(label)}\s+is\s+(?:a|an)\s+([^,.;]+)", text, re.I):
        pred = " ".join(m.group(1).strip().split()[:5])
        if pred and len(pred) > 3:
            facts.append(("is_a", pred))

    # "X was designed/developed/created by Y"
    for m in re.finditer(rf"{re.escape(label)}\s+was\s+(?:designed|developed|created|invented)\s+by\s+([^,.;]+)", text, re.I):
        who = m.group(1).strip().split(" at ")[0].split(" in ")[0]
        who = " ".join(who.split()[:4])
        if who:
            facts.append(("created_by", who))

    # "X is used for/in Y"
    for m in re.finditer(rf"{re.escape(label)}\s+is\s+(?:used|employed|applied)\s+(?:for|in)\s+([^,.;]+)", text, re.I):
        use = " ".join(m.group(1).strip().split()[:4])
        if use:
            facts.append(("used_for", use))

    # "X supports Y"
    for m in re.finditer(rf"{re.escape(label)}\s+supports?\s+([^,.;]+)", text, re.I):
        what = " ".join(m.group(1).strip().split()[:4])
        if what and len(what) > 3:
            facts.append(("supports", what))

    return facts


def import_seed_article(article):
    """Import a seed article with full metadata and relationships."""
    label = article["label"]
    domain = article["domain"]

    # Fetch Wikipedia summary
    data = fetch_wiki_summary(article["title"])
    if not data:
        print(f"  [skip] Wikipedia article not found: {article['title']}")
        return 0

    extract = data.get("extract", "")
    description = data.get("description", "")
    url = data.get("content_urls", {}).get("desktop", {}).get("page", "")

    # Build properties
    props = {
        "description": description[:200] if description else "",
        "wikipedia_title": article["title"],
        "domain": domain,
    }
    if url:
        props["source_url"] = url
    if "creator" in article:
        props["creator"] = article["creator"]
    if "typing" in article:
        props["typing"] = article["typing"]
    if "first_appeared" in article:
        props["first_appeared"] = article["first_appeared"]

    # Store main node
    result = api_store(label, entity_type=domain, properties=props, confidence=0.92)
    print(f"  stored: {label} (id={result['node_id']})")

    ops_count = 1

    # Programming language specifics
    if domain == "programming_language":
        # Creator
        creator = article.get("creator", "")
        if creator:
            api_store(creator, entity_type="person", confidence=0.90)
            api_relate(label, "created_by", creator, confidence=0.92)
            ops_count += 2

        # Paradigms
        for p in article.get("paradigms", []):
            api_store(p, entity_type="paradigm", confidence=0.90)
            api_relate(label, "uses_paradigm", p, confidence=0.90)
            ops_count += 2

        # Typing
        typing = article.get("typing", "")
        if typing:
            api_store(typing, entity_type="type_system", confidence=0.90)
            api_relate(label, "has_type_system", typing, confidence=0.90)
            ops_count += 2

        # Influenced by
        for inf in article.get("influenced_by", []):
            api_store(inf, entity_type="programming_language", confidence=0.85)
            api_relate(label, "influenced_by", inf, confidence=0.85)
            ops_count += 2

    # CS concept or organization: related_to links
    for rel in article.get("related_to", []):
        api_store(rel, entity_type="concept", confidence=0.80)
        api_relate(label, "related_to", rel, confidence=0.80)
        ops_count += 2

    # Extract facts from summary text
    facts = extract_facts(label, extract)
    for rel_type, obj in facts:
        resp = api_tell(f"{label} {rel_type.replace('_', ' ')} {obj}", source="wikipedia")
        print(f"  told: {resp.get('interpretation', '?')}")
        ops_count += 1

    # Fetch categories and store as relationships
    cats = fetch_wiki_categories(article["title"], limit=8)
    for cat in cats[:5]:
        try:
            api_store(cat, entity_type="category", confidence=0.75)
            api_relate(label, "in_category", cat, confidence=0.75)
            ops_count += 2
            print(f"  category: {cat}")
        except Exception as e:
            print(f"  [warn] category '{cat}' failed: {e}")

    time.sleep(0.3)
    return ops_count


def import_followup_article(title):
    """Import a follow-up article discovered via links or predefined list."""
    label = title.replace("_", " ").replace("(", "").replace(")", "").strip()

    # Skip if already stored (check via search)
    try:
        existing = api_search(f'"{label}"', limit=1)
        if existing.get("results") and any(r["label"].lower() == label.lower()
                                           for r in existing["results"]):
            return 0
    except Exception:
        pass

    data = fetch_wiki_summary(title)
    if not data:
        return 0

    extract = data.get("extract", "")
    description = data.get("description", "")
    url = data.get("content_urls", {}).get("desktop", {}).get("page", "")

    props = {"description": description[:200] if description else ""}
    if url:
        props["source_url"] = url
    props["wikipedia_title"] = title

    api_store(label, entity_type="concept", properties=props, confidence=0.80)
    print(f"  followup: {label}")

    ops_count = 1

    # Extract and tell facts
    facts = extract_facts(label, extract)
    for rel_type, obj in facts[:3]:
        api_tell(f"{label} {rel_type.replace('_', ' ')} {obj}", source="wikipedia")
        ops_count += 1

    time.sleep(0.2)
    return ops_count


def run_queries(with_embedder=False):
    """Run a battery of queries to verify the imported data."""
    print("\n" + "=" * 60)
    print("QUERY RESULTS")
    print("=" * 60)

    # 1. Stats
    stats = api_stats()
    print(f"\nGraph size: {stats['nodes']} nodes, {stats['edges']} edges")

    # 2. Keyword searches
    print("\n--- Keyword Search ---")

    for query in ["functional programming", "static typing", "systems programming",
                  "machine learning", "knowledge graph"]:
        results = api_search(query, limit=5)
        hits = results.get("results", [])
        labels = [r["label"] for r in hits[:5]]
        print(f"  '{query}': {labels}")

    # 3. Property filter searches
    print("\n--- Property Filters ---")

    for query in ["prop:typing=static typing", "prop:typing=dynamic typing",
                  "prop:domain=programming_language", "prop:domain=cs_concept"]:
        results = api_search(query, limit=10)
        hits = results.get("results", [])
        labels = [r["label"] for r in hits]
        print(f"  '{query}': {labels}")

    # 4. Confidence filter
    print("\n--- Confidence Filter ---")
    results = api_search("confidence>0.9", limit=20)
    hits = results.get("results", [])
    print(f"  High confidence (>0.9): {len(hits)} nodes")
    for r in hits[:8]:
        print(f"    {r['label']} (conf={r['confidence']:.2f})")

    # 5. Boolean search
    print("\n--- Boolean Search ---")
    for query in ["functional AND programming", "static AND typing",
                  "type:programming_language AND confidence>0.85"]:
        results = api_search(query, limit=5)
        hits = results.get("results", [])
        labels = [r["label"] for r in hits[:5]]
        print(f"  '{query}': {labels}")

    # 6. Graph traversal
    print("\n--- Graph Traversal ---")

    for start_node in ["Python", "Rust", "machine learning", "knowledge graph"]:
        result = api_query(start_node, depth=2, min_confidence=0.7)
        nodes = result.get("nodes", [])
        edges = result.get("edges", [])
        print(f"  From '{start_node}' (2-hop, min_conf=0.7): {len(nodes)} nodes, {len(edges)} edges")
        for n in nodes[:6]:
            print(f"    {n['label']} (conf={n['confidence']:.2f}, depth={n.get('depth', '?')})")
        if len(nodes) > 6:
            print(f"    ... and {len(nodes) - 6} more")

    # 7. Natural language /ask
    print("\n--- Natural Language Ask ---")

    questions = [
        "What does Python connect to?",
        "What is Rust related to?",
        "What are the types of programming language?",
        "What connects to functional programming?",
        "Who created Go?",
        "What uses static typing?",
        "What is machine learning?",
        "What does TypeScript connect to?",
    ]
    for q in questions:
        resp = api_ask(q)
        interp = resp.get("interpretation", "?")
        results = resp.get("results", [])
        top = [f"{r['label']}" for r in results[:4]]
        print(f"  Q: {q}")
        print(f"    [{interp}] -> {top}")

    # 8. Semantic search (embedder only)
    if with_embedder:
        print("\n--- Semantic Search (embedder) ---")

        semantic_queries = [
            "languages good for web development",
            "how computers understand human language",
            "memory safe programming",
            "distributed computing systems",
            "artificial intelligence applications",
            "type checking and safety",
            "graph databases and linked data",
            "concurrent and parallel execution",
        ]
        for q in semantic_queries:
            results = api_search(q, limit=5)
            hits = results.get("results", [])
            labeled = [f"{r['label']}({r.get('score', 0):.2f})" for r in hits[:5]]
            print(f"  '{q}':")
            print(f"    {labeled}")

    # 9. Cross-domain connections (use /ask for incoming edges)
    print("\n--- Cross-Domain Connections ---")

    # Find what languages use each paradigm (incoming edges)
    for paradigm in ["functional programming", "systems programming", "concurrent programming"]:
        resp = api_ask(f"What connects to {paradigm}?")
        results = resp.get("results", [])
        langs = [r["label"] for r in results]
        print(f"  Languages using '{paradigm}': {langs}")

    # Find what connects to each creator (incoming edges)
    for creator in ["Guido van Rossum", "Graydon Hoare", "Dennis Ritchie"]:
        resp = api_ask(f"What connects to {creator}?")
        results = resp.get("results", [])
        things = [r["label"] for r in results]
        print(f"  '{creator}' connects to: {things}")

    # Influence chains
    print("\n--- Influence Chains ---")
    for lang in ["Rust", "Python", "TypeScript", "Go"]:
        resp = api_ask(f"What does {lang} connect to?")
        results = resp.get("results", [])
        influences = [f"{r['label']}({r.get('relationship', '?')})"
                      for r in results if r.get("relationship") in
                      ("influenced_by", "uses_paradigm", "created_by", "has_type_system")]
        print(f"  {lang}: {influences}")


def main():
    with_embedder = "--with-embedder" in sys.argv

    # Verify server is up
    try:
        r = requests.get(f"{ENGRAM}/health", timeout=5)
        r.raise_for_status()
        print(f"Server healthy: {r.json()}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve wiki_deep.brain 127.0.0.1:3030")
        sys.exit(1)

    total_ops = 0

    # Phase 1: Import seed articles
    print(f"\n{'=' * 60}")
    print(f"PHASE 1: Importing {len(SEED_ARTICLES)} seed articles")
    print(f"{'=' * 60}\n")

    for article in SEED_ARTICLES:
        print(f"\n--- {article['label']} ---")
        try:
            ops = import_seed_article(article)
            total_ops += ops
            print(f"  ({ops} operations)")
        except Exception as e:
            print(f"  [ERROR] {e}")

    stats = api_stats()
    print(f"\nAfter Phase 1: {stats['nodes']} nodes, {stats['edges']} edges ({total_ops} API calls)")

    # Phase 2: Follow-up articles
    print(f"\n{'=' * 60}")
    print(f"PHASE 2: Importing {len(FOLLOW_UP_ARTICLES)} follow-up articles")
    print(f"{'=' * 60}\n")

    for title in FOLLOW_UP_ARTICLES:
        ops = import_followup_article(title)
        total_ops += ops

    stats = api_stats()
    print(f"\nAfter Phase 2: {stats['nodes']} nodes, {stats['edges']} edges ({total_ops} total API calls)")

    # Phase 3: Cross-link with /tell
    print(f"\n{'=' * 60}")
    print("PHASE 3: Cross-linking with natural language")
    print(f"{'=' * 60}\n")

    cross_links = [
        "Rust was developed at Mozilla",
        "Go was developed at Google",
        "TypeScript was developed at Microsoft",
        "LLVM is used by Rust",
        "LLVM is used by C",
        "LLVM is used by C++",
        "Python is popular for machine learning",
        "JavaScript runs in web browsers",
        "Linux is written in C",
        "C++ evolved from C",
        "TypeScript is a superset of JavaScript",
        "Haskell influenced Rust",
        "neural network is part of deep learning",
        "deep learning is part of machine learning",
        "knowledge graph uses RDF",
        "knowledge graph is related to semantic web",
        "compiler uses abstract syntax tree",
        "compiler performs code generation",
        "garbage collection is used in Go",
        "garbage collection is used in Python",
        "garbage collection is used in JavaScript",
        "memory safety is a feature of Rust",
        "concurrency is supported by Go",
        "concurrency is supported by Rust",
        "concurrency is supported by Erlang",
        "lambda calculus is the foundation of functional programming",
        "algorithm is a fundamental concept in computer science",
        "data structure is used in algorithm",
    ]

    for stmt in cross_links:
        try:
            resp = api_tell(stmt, source="wikipedia-crosslink")
            interp = resp.get("interpretation", "?")
            print(f"  {stmt} -> [{interp}]")
            total_ops += 1
        except Exception as e:
            print(f"  [warn] '{stmt}' failed: {e}")

    stats = api_stats()
    print(f"\nAfter Phase 3: {stats['nodes']} nodes, {stats['edges']} edges ({total_ops} total API calls)")

    # Phase 4: Deep enrichment -- give leaf nodes their own relationships
    print(f"\n{'=' * 60}")
    print("PHASE 4: Deep enrichment (creators, paradigms, concepts)")
    print(f"{'=' * 60}\n")

    # 4a: Creator -> institution/birthplace/era relationships
    creator_facts = [
        # Guido van Rossum
        ("Guido van Rossum", "worked_at", "Google", 0.88),
        ("Guido van Rossum", "worked_at", "Dropbox", 0.88),
        ("Guido van Rossum", "worked_at", "Microsoft", 0.88),
        ("Guido van Rossum", "born_in", "Netherlands", 0.90),
        ("Guido van Rossum", "studied_at", "University of Amsterdam", 0.88),
        ("Guido van Rossum", "created", "Python", 0.95),
        # Dennis Ritchie
        ("Dennis Ritchie", "worked_at", "Bell Labs", 0.92),
        ("Dennis Ritchie", "co_created", "Unix", 0.92),
        ("Dennis Ritchie", "born_in", "United States", 0.90),
        ("Dennis Ritchie", "studied_at", "Harvard University", 0.88),
        ("Bell Labs", "part_of", "AT&T", 0.90),
        ("Bell Labs", "located_in", "United States", 0.90),
        ("Unix", "influenced", "Linux", 0.90),
        ("Unix", "written_in", "C", 0.92),
        # Graydon Hoare
        ("Graydon Hoare", "worked_at", "Mozilla", 0.90),
        ("Graydon Hoare", "worked_at", "Apple", 0.85),
        ("Graydon Hoare", "created", "Rust", 0.95),
        # Rob Pike
        ("Rob Pike", "worked_at", "Bell Labs", 0.90),
        ("Rob Pike", "worked_at", "Google", 0.90),
        ("Rob Pike", "co_created", "Go", 0.92),
        ("Rob Pike", "co_created", "Plan 9", 0.88),
        ("Rob Pike", "studied_at", "University of Toronto", 0.85),
        # Brendan Eich
        ("Brendan Eich", "worked_at", "Netscape", 0.90),
        ("Brendan Eich", "co_founded", "Mozilla", 0.90),
        ("Brendan Eich", "created", "JavaScript", 0.95),
        ("Brendan Eich", "born_in", "United States", 0.90),
        # Anders Hejlsberg
        ("Anders Hejlsberg", "worked_at", "Microsoft", 0.90),
        ("Anders Hejlsberg", "worked_at", "Borland", 0.88),
        ("Anders Hejlsberg", "created", "TypeScript", 0.92),
        ("Anders Hejlsberg", "created", "Turbo Pascal", 0.90),
        ("Anders Hejlsberg", "designed", "C#", 0.90),
        ("Anders Hejlsberg", "born_in", "Denmark", 0.90),
        # Bjarne Stroustrup
        ("Bjarne Stroustrup", "worked_at", "Bell Labs", 0.90),
        ("Bjarne Stroustrup", "worked_at", "Morgan Stanley", 0.85),
        ("Bjarne Stroustrup", "created", "C++", 0.95),
        ("Bjarne Stroustrup", "born_in", "Denmark", 0.90),
        ("Bjarne Stroustrup", "studied_at", "University of Cambridge", 0.88),
        # Lennart Augustsson
        ("Lennart Augustsson", "worked_at", "Chalmers University", 0.85),
        ("Lennart Augustsson", "born_in", "Sweden", 0.85),
    ]

    print("  -- Creators --")
    for subj, rel, obj, conf in creator_facts:
        try:
            api_store(subj, entity_type="person", confidence=conf)
            api_store(obj, confidence=conf)
            api_relate(subj, rel, obj, confidence=conf)
            print(f"  {subj} -[{rel}]-> {obj}")
            total_ops += 3
        except Exception as e:
            print(f"  [warn] {subj}->{obj}: {e}")

    # 4b: Paradigm interconnections (paradigm -> paradigm, paradigm -> concept)
    paradigm_links = [
        ("functional programming", "based_on", "lambda calculus", 0.90),
        ("functional programming", "related_to", "type inference", 0.85),
        ("functional programming", "contrasts_with", "imperative programming", 0.85),
        ("functional programming", "supports", "immutability", 0.85),
        ("functional programming", "supports", "higher-order functions", 0.85),
        ("functional programming", "supports", "pattern matching", 0.85),
        ("object-oriented programming", "uses", "inheritance", 0.88),
        ("object-oriented programming", "uses", "polymorphism", 0.88),
        ("object-oriented programming", "uses", "encapsulation", 0.88),
        ("object-oriented programming", "related_to", "design patterns", 0.85),
        ("imperative programming", "related_to", "structured programming", 0.85),
        ("imperative programming", "uses", "mutable state", 0.85),
        ("concurrent programming", "uses", "threads", 0.88),
        ("concurrent programming", "uses", "message passing", 0.88),
        ("concurrent programming", "related_to", "parallelism", 0.85),
        ("concurrent programming", "challenges", "race conditions", 0.85),
        ("concurrent programming", "challenges", "deadlock", 0.85),
        ("systems programming", "requires", "memory management", 0.88),
        ("systems programming", "targets", "operating system", 0.85),
        ("systems programming", "targets", "embedded systems", 0.85),
        ("generic programming", "uses", "type parameters", 0.85),
        ("generic programming", "related_to", "type inference", 0.85),
        ("generic programming", "enables", "code reuse", 0.85),
        ("lazy evaluation", "used_in", "Haskell", 0.90),
        ("lazy evaluation", "related_to", "functional programming", 0.85),
        ("prototype-based programming", "alternative_to", "class-based programming", 0.85),
        ("prototype-based programming", "used_in", "JavaScript", 0.90),
        ("event-driven programming", "uses", "callbacks", 0.85),
        ("event-driven programming", "uses", "event loop", 0.88),
        ("event-driven programming", "used_in", "JavaScript", 0.88),
    ]

    print("\n  -- Paradigm interconnections --")
    for subj, rel, obj, conf in paradigm_links:
        try:
            api_store(subj, entity_type="paradigm", confidence=conf)
            api_store(obj, confidence=conf)
            api_relate(subj, rel, obj, confidence=conf)
            print(f"  {subj} -[{rel}]-> {obj}")
            total_ops += 3
        except Exception as e:
            print(f"  [warn] {subj}->{obj}: {e}")

    # 4c: CS concept depth (concept -> concept)
    concept_links = [
        # Machine learning depth
        ("machine learning", "subfield_of", "artificial intelligence", 0.92),
        ("deep learning", "subfield_of", "machine learning", 0.92),
        ("neural network", "used_in", "deep learning", 0.90),
        ("neural network", "inspired_by", "biological neuron", 0.80),
        ("supervised learning", "type_of", "machine learning", 0.90),
        ("unsupervised learning", "type_of", "machine learning", 0.90),
        ("artificial intelligence", "studies", "natural language processing", 0.85),
        ("artificial intelligence", "studies", "computer vision", 0.85),
        ("artificial intelligence", "studies", "robotics", 0.80),
        ("artificial intelligence", "uses", "algorithm", 0.85),
        # Knowledge graph depth
        ("knowledge graph", "stores_data_as", "triples", 0.88),
        ("knowledge graph", "uses", "ontology", 0.88),
        ("RDF", "standard_by", "W3C", 0.90),
        ("RDF", "serialized_as", "JSON-LD", 0.88),
        ("RDF", "serialized_as", "Turtle", 0.85),
        ("semantic web", "proposed_by", "Tim Berners-Lee", 0.90),
        ("semantic web", "uses", "RDF", 0.90),
        ("semantic web", "uses", "OWL", 0.85),
        ("linked data", "part_of", "semantic web", 0.88),
        ("linked data", "uses", "URIs", 0.88),
        ("graph database", "examples", "Neo4j", 0.85),
        ("graph database", "examples", "Amazon Neptune", 0.80),
        ("graph database", "stores", "knowledge graph", 0.85),
        ("ontology", "defines", "classes and relations", 0.85),
        ("ontology", "expressed_in", "OWL", 0.85),
        # Compiler depth
        ("compiler", "phase", "lexical analysis", 0.88),
        ("compiler", "phase", "parsing", 0.88),
        ("compiler", "phase", "semantic analysis", 0.85),
        ("compiler", "phase", "code generation", 0.88),
        ("compiler", "phase", "optimization", 0.88),
        ("LLVM", "type_of", "compiler infrastructure", 0.90),
        ("LLVM", "created_by", "Chris Lattner", 0.90),
        ("LLVM", "developed_at", "University of Illinois", 0.85),
        ("LLVM", "used_by", "Rust", 0.90),
        ("LLVM", "used_by", "C++", 0.88),
        ("LLVM", "used_by", "Swift", 0.88),
        ("abstract syntax tree", "used_in", "compiler", 0.90),
        ("abstract syntax tree", "represents", "source code structure", 0.85),
        ("parser", "produces", "abstract syntax tree", 0.88),
        ("parser", "type_of", "compiler component", 0.85),
        # Operating system depth
        ("operating system", "manages", "process management", 0.90),
        ("operating system", "manages", "memory management", 0.90),
        ("operating system", "manages", "file system", 0.90),
        ("operating system", "provides", "device drivers", 0.85),
        ("operating system", "provides", "system calls", 0.85),
        ("Linux", "created_by", "Linus Torvalds", 0.92),
        ("Linux", "type_of", "operating system", 0.92),
        ("Linux", "uses", "kernel", 0.90),
        ("Linux", "written_in", "C", 0.92),
        ("Linux", "license", "GPL", 0.90),
        ("kernel", "part_of", "operating system", 0.90),
        ("kernel", "manages", "hardware abstraction", 0.85),
        ("file system", "examples", "ext4", 0.80),
        ("file system", "examples", "NTFS", 0.80),
        ("process management", "uses", "scheduler", 0.85),
        ("process management", "uses", "threads", 0.85),
        # Type system depth
        ("static typing", "catches", "type errors at compile time", 0.88),
        ("static typing", "used_by", "Rust", 0.90),
        ("static typing", "used_by", "Go", 0.90),
        ("static typing", "used_by", "TypeScript", 0.90),
        ("static typing", "used_by", "Haskell", 0.90),
        ("static typing", "used_by", "C", 0.90),
        ("static typing", "used_by", "C++", 0.90),
        ("dynamic typing", "checks", "type errors at runtime", 0.88),
        ("dynamic typing", "used_by", "Python", 0.90),
        ("dynamic typing", "used_by", "JavaScript", 0.90),
        ("type inference", "used_by", "Rust", 0.88),
        ("type inference", "used_by", "Haskell", 0.90),
        ("type inference", "related_to", "Hindley-Milner", 0.85),
        # Data structures & algorithms depth
        ("algorithm", "examples", "sorting algorithm", 0.85),
        ("algorithm", "examples", "graph algorithm", 0.85),
        ("algorithm", "measured_by", "time complexity", 0.88),
        ("algorithm", "measured_by", "space complexity", 0.88),
        ("data structure", "examples", "array", 0.85),
        ("data structure", "examples", "hash table", 0.85),
        ("data structure", "examples", "tree", 0.85),
        ("data structure", "examples", "graph", 0.85),
        ("data structure", "used_in", "algorithm", 0.88),
        ("graph algorithm", "examples", "BFS", 0.85),
        ("graph algorithm", "examples", "Dijkstra", 0.85),
        ("graph algorithm", "operates_on", "graph", 0.88),
        # Memory & safety
        ("memory safety", "prevents", "buffer overflow", 0.90),
        ("memory safety", "prevents", "use after free", 0.90),
        ("memory safety", "prevents", "null pointer dereference", 0.88),
        ("memory safety", "enforced_by", "borrow checker", 0.90),
        ("borrow checker", "part_of", "Rust", 0.92),
        ("borrow checker", "ensures", "memory safety", 0.92),
        ("garbage collection", "alternative_to", "manual memory management", 0.85),
        ("garbage collection", "used_by", "Python", 0.90),
        ("garbage collection", "used_by", "Go", 0.90),
        ("garbage collection", "used_by", "JavaScript", 0.90),
        ("garbage collection", "used_by", "Java", 0.90),
        ("manual memory management", "used_by", "C", 0.90),
        ("manual memory management", "used_by", "C++", 0.88),
        # Concurrency depth
        ("concurrency", "model", "actor model", 0.85),
        ("concurrency", "model", "CSP", 0.85),
        ("concurrency", "model", "shared memory", 0.85),
        ("actor model", "used_by", "Erlang", 0.90),
        ("CSP", "inspired", "Go", 0.88),
        ("CSP", "created_by", "Tony Hoare", 0.90),
        ("message passing", "used_in", "actor model", 0.88),
        ("message passing", "used_in", "CSP", 0.85),
        ("threads", "part_of", "concurrent programming", 0.88),
        ("threads", "managed_by", "operating system", 0.85),
        # Lambda calculus depth
        ("lambda calculus", "created_by", "Alonzo Church", 0.92),
        ("lambda calculus", "equivalent_to", "Turing machine", 0.90),
        ("lambda calculus", "foundation_of", "functional programming", 0.92),
        ("Turing machine", "created_by", "Alan Turing", 0.92),
        ("Turing machine", "defines", "computability", 0.90),
        ("Alan Turing", "worked_at", "Bletchley Park", 0.90),
        ("Alan Turing", "born_in", "United Kingdom", 0.90),
        ("Alonzo Church", "worked_at", "Princeton University", 0.88),
        ("Alonzo Church", "born_in", "United States", 0.88),
        # Organizations depth
        ("Mozilla", "created", "Firefox", 0.92),
        ("Mozilla", "sponsored", "Rust", 0.90),
        ("Mozilla", "type_of", "open source foundation", 0.88),
        ("Google", "created", "Go", 0.90),
        ("Google", "created", "TensorFlow", 0.90),
        ("Google", "created", "Chromium", 0.88),
        ("Google", "created", "Android", 0.88),
        ("Google", "founded_by", "Larry Page", 0.92),
        ("Google", "founded_by", "Sergey Brin", 0.92),
        ("Google", "headquartered_in", "Mountain View", 0.90),
        ("Microsoft", "created", "TypeScript", 0.90),
        ("Microsoft", "created", "Visual Studio Code", 0.88),
        ("Microsoft", "created", "Azure", 0.85),
        ("Microsoft", "founded_by", "Bill Gates", 0.92),
        ("Microsoft", "headquartered_in", "Redmond", 0.90),
    ]

    print("\n  -- CS concept depth --")
    for subj, rel, obj, conf in concept_links:
        try:
            api_store(subj, confidence=conf)
            api_store(obj, confidence=conf)
            api_relate(subj, rel, obj, confidence=conf)
            print(f"  {subj} -[{rel}]-> {obj}")
            total_ops += 3
        except Exception as e:
            print(f"  [warn] {subj}->{obj}: {e}")

    stats = api_stats()
    print(f"\nFinal graph: {stats['nodes']} nodes, {stats['edges']} edges ({total_ops} total API calls)")

    # Run queries
    run_queries(with_embedder=with_embedder)

    # Export JSON-LD
    print(f"\n{'=' * 60}")
    print("JSON-LD EXPORT")
    print(f"{'=' * 60}")
    try:
        r = requests.get(f"{ENGRAM}/export/jsonld", timeout=10)
        r.raise_for_status()
        jsonld = r.json()
        graph = jsonld.get("@graph", [])
        print(f"  Exported {len(graph)} entities to JSON-LD")
        # Show a sample
        if graph:
            print(f"  Sample entity: {json.dumps(graph[0], indent=2)[:300]}")
    except Exception as e:
        print(f"  JSON-LD export failed: {e}")


if __name__ == "__main__":
    main()
