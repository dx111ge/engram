# Use Case 11: Semantic Web -- Linked Data Integration

**Version:** 0.1.0
**Last updated:** 2026-03-08

This walkthrough demonstrates how to use engram as a bridge between AI agent memory and the semantic web ecosystem. We import structured knowledge from Wikidata and schema.org via JSON-LD, enrich it with engram's confidence scoring and inference engine, and export it back as interoperable linked data.

---

## Why This Matters

The semantic web (RDF, JSON-LD, SPARQL) and AI agent memory are converging. Agents need structured knowledge, and the linked data ecosystem (Wikidata, DBpedia, schema.org) already has billions of facts. Engram's JSON-LD import/export bridges these worlds:

- **Import**: Pull structured facts from any JSON-LD source into engram's knowledge graph
- **Enrich**: Apply confidence scoring, learning, inference rules, and decay
- **Export**: Push enriched knowledge back as standard JSON-LD consumable by any RDF-aware system
- **Interoperate**: Share knowledge between engram instances, other agents, and semantic web tools

---

## Prerequisites

```bash
# Start engram server
engram serve semantic.brain

# Verify it's running
curl http://localhost:3030/health
```

Python with `requests` library for the import scripts.

---

## Step 1: Import from Schema.org

Schema.org types are the standard vocabulary for structured data on the web. We import a small domain model.

```python
import requests

ENGRAM = "http://localhost:3030"

# A small schema.org-compatible knowledge graph about programming languages
data = {
    "@context": {
        "schema": "https://schema.org/",
        "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
        "engram": "engram://vocab/"
    },
    "@graph": [
        {
            "@id": "schema:Rust_(programming_language)",
            "@type": "schema:ComputerLanguage",
            "rdfs:label": "Rust",
            "schema:dateCreated": "2010",
            "schema:creator": {"@id": "engram://node/Graydon%20Hoare"},
            "schema:operatingSystem": "Cross-platform"
        },
        {
            "@id": "engram://node/Graydon%20Hoare",
            "@type": "schema:Person",
            "rdfs:label": "Graydon Hoare"
        },
        {
            "@id": "schema:WebAssembly",
            "@type": "schema:ComputerLanguage",
            "rdfs:label": "WebAssembly",
            "schema:dateCreated": "2015"
        },
        {
            "@id": "schema:Mozilla",
            "@type": "schema:Organization",
            "rdfs:label": "Mozilla",
            "schema:sponsor": {"@id": "engram://node/Graydon%20Hoare"}
        }
    ]
}

resp = requests.post(f"{ENGRAM}/import/jsonld", json={
    "data": data,
    "source": "schema.org"
})
print(resp.json())
# {"nodes_imported": 4, "edges_imported": 2, "errors": null}
```

Engram creates nodes from each `@graph` entry. Object references (values with `@id`) become edges. String values become properties.

---

## Step 2: Import from Wikidata

Wikidata provides structured knowledge about nearly everything. We can import Wikidata entities by converting their JSON-LD representation.

```python
import requests

ENGRAM = "http://localhost:3030"

# Simplified Wikidata-style data about a city
wikidata = {
    "@context": {
        "wd": "http://www.wikidata.org/entity/",
        "wdt": "http://www.wikidata.org/prop/direct/",
        "rdfs": "http://www.w3.org/2000/01/rdf-schema#"
    },
    "@graph": [
        {
            "@id": "wd:Q64",
            "@type": "wd:Q515",
            "rdfs:label": "Berlin",
            "wdt:P17": {"@id": "wd:Q183"},
            "wdt:P1082": "3645000"
        },
        {
            "@id": "wd:Q183",
            "rdfs:label": "Germany",
            "@type": "wd:Q6256"
        },
        {
            "@id": "wd:Q515",
            "rdfs:label": "city"
        },
        {
            "@id": "wd:Q6256",
            "rdfs:label": "country"
        }
    ]
}

resp = requests.post(f"{ENGRAM}/import/jsonld", json={
    "data": wikidata,
    "source": "wikidata"
})
print(resp.json())
# {"nodes_imported": 4, "edges_imported": 1, "errors": null}

# Now query the imported data
resp = requests.post(f"{ENGRAM}/query", json={"start": "Berlin", "depth": 2})
print(resp.json())
```

Note: Wikidata property IDs (P17 = country, P1082 = population) become edge and property labels. You can post-process them by adding rules or a mapping table.

---

## Step 3: Enrich with Inference Rules

Once imported, we can use engram's inference engine to derive new facts.

```python
import requests

ENGRAM = "http://localhost:3030"

# Load rules that work on the imported data
rules = {
    "rules": [
        # If A is created by B, and B is sponsored by C, then A is related to C
        'rule creator_org\nwhen edge(A, "creator", B)\nwhen edge(C, "sponsor", B)\nthen edge(A, "associated_with", C, product(e1, e2))',

        # Transitive type hierarchy
        'rule type_transitive\nwhen edge(A, "is_a", B)\nwhen edge(B, "is_a", C)\nthen edge(A, "is_a", C, min(e1, e2))'
    ]
}

resp = requests.post(f"{ENGRAM}/rules", json=rules)
print(f"Rules loaded: {resp.json()}")

# Derive new facts
resp = requests.post(f"{ENGRAM}/learn/derive", json={})
print(f"Derivation: {resp.json()}")
```

---

## Step 4: Export as JSON-LD

Export the enriched graph back as standard JSON-LD. The output is consumable by any RDF tool (Apache Jena, RDFLib, Virtuoso, GraphDB).

```python
import requests
import json

ENGRAM = "http://localhost:3030"

resp = requests.get(f"{ENGRAM}/export/jsonld")
doc = resp.json()

# Pretty-print the JSON-LD
print(json.dumps(doc, indent=2))

# Save to file for use with other tools
with open("knowledge.jsonld", "w") as f:
    json.dump(doc, f, indent=2)
```

The exported JSON-LD includes:
- `@context` with engram, schema.org, RDF, and RDFS namespaces
- `@graph` array with one entry per node
- Each node has `@id` (URI), `rdfs:label`, `engram:confidence`, `engram:memoryTier`
- Node types as `@type`
- Properties as namespace-prefixed keys
- Edges as object references with confidence annotations

---

## Step 5: Round-Trip with External Tools

The exported JSON-LD can be loaded into any RDF store or processed with standard tools.

### Load into RDFLib (Python)

```python
from rdflib import Graph as RDFGraph

g = RDFGraph()
g.parse("knowledge.jsonld", format="json-ld")

# SPARQL query over engram data
results = g.query("""
    SELECT ?label ?confidence WHERE {
        ?node <http://www.w3.org/2000/01/rdf-schema#label> ?label .
        ?node <engram://vocab/confidence> ?confidence .
    }
    ORDER BY DESC(?confidence)
""")

for row in results:
    print(f"{row.label}: confidence={row.confidence}")
```

### Load into Apache Jena

```bash
# Load into Jena TDB2
tdb2.tdbloader --loc=jena-db knowledge.jsonld

# Query with SPARQL
tdb2.tdbquery --loc=jena-db --query=query.rq
```

### Validate with JSON-LD Playground

Upload `knowledge.jsonld` to the [JSON-LD Playground](https://json-ld.org/playground/) to visualize the graph structure and verify the context resolves correctly.

---

## Architecture: Engram as a Semantic Web Bridge

```
+----------------+     JSON-LD      +----------+     JSON-LD      +----------------+
|   Wikidata     | ───────────────> |          | ───────────────> |   Apache Jena  |
|   DBpedia      |                  |  engram  |                  |   RDFLib       |
|   schema.org   | <─ import ─────  |          |  ── export ───>  |   GraphDB      |
+----------------+                  +----------+                  +----------------+
                                         |
                                    Enrichment:
                                    - Confidence scoring
                                    - Inference rules
                                    - Decay & reinforcement
                                    - Memory tiers
                                    - Co-occurrence stats
```

Engram is not an RDF store. It does not support SPARQL natively, OWL reasoning, or named graphs. What it adds to the linked data ecosystem:

1. **Confidence lifecycle**: Every fact has a confidence score that changes over time through reinforcement, decay, and correction. RDF stores treat all triples as equally true.

2. **AI-native protocols**: MCP (for LLM tool-calling), A2A (for agent-to-agent), and natural language interfaces. RDF stores require SPARQL.

3. **Single-binary deployment**: No JVM, no external database, no configuration. Import JSON-LD, query it, export it.

4. **Knowledge mesh**: Multiple engram instances can sync knowledge with trust-based confidence propagation. RDF federation exists but is complex.

---

## Key Takeaways

- JSON-LD is the bridge between engram and the semantic web
- Import from Wikidata, DBpedia, schema.org, or any JSON-LD source
- Engram enriches imported data with confidence, inference, and decay
- Export back as standard JSON-LD for interoperability
- Use engram where you need AI-native memory with semantic web compatibility
- Use a proper RDF store (Jena, Virtuoso) where you need SPARQL, OWL, or standards compliance
