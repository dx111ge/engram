# Engram Use Cases

**Version:** 1.0.0
**Last updated:** 2026-03-07

This directory contains eleven end-to-end walkthroughs showing how to use engram in real applications. Each walkthrough is reproducible: every command, script, and expected output is shown as it actually runs against engram v1.0.0. Where external tools are required beyond the engram binary, this is called out explicitly.

## Use Cases

| # | Use Case | Description | Key Features Demonstrated |
|---|----------|-------------|---------------------------|
| 1 | [Wikipedia Import](01-wikipedia-import/) | Build a knowledge graph from Wikipedia article summaries | `/tell`, `/store`, `/relate`, BM25 search, graph traversal, `/ask` |
| 2 | [Document Import](02-document-import/) | Ingest local markdown/text files with metadata and entity extraction | Document nodes, `prop:key=value` filters, `mentions` relationships |
| 3 | [Inference & Reasoning](03-inference-reasoning/) | Vulnerability propagation and SLA mismatch detection in a service graph | `/learn/derive`, rule engine, transitive closure, `flag`, backward chaining, `/explain` |
| 4 | [Support Knowledge Base](04-support-knowledge-base/) | IT support error/cause/solution graphs with confidence lifecycle | `/learn/reinforce`, `/learn/correct`, `/learn/decay`, `/learn/derive`, memory tiers |
| 5 | [Threat Intelligence](05-threat-intelligence/) | Threat actor, malware, CVE, and TTP relationship graphs | Typed nodes, IOC properties, confidence-scored attribution, correction propagation |
| 6 | [Learning Lifecycle](06-learning-lifecycle/) | Full lifecycle of knowledge: store, reinforce, correct, decay, archive | Confidence evolution, memory tier transitions, inference rules, decay mechanics |
| 7 | [OSINT](07-osint/) | Open Source Intelligence gathering with multi-source correlation | Multi-source provenance, graph traversal for hidden connections, inference rules |
| 8 | [Fact Checker](08-fact-checker/) | Multi-source claim verification with source reliability tiers | Source reliability, corroboration, contradiction handling, credibility rules |
| 9 | [Web Search Import](09-web-search-import/) | Progressive knowledge building from web search results | Case-insensitive dedup, iterative deepening, scheduled import, decay |
| 10 | [NER Entity Extraction](10-ner-entity-extraction/) | spaCy NER pipeline for extracting entities and relationships from text | NER, dependency parsing, co-occurrence, entity resolution, custom patterns |
| 11 | [Semantic Web](11-semantic-web/) | JSON-LD import/export for linked data interoperability | JSON-LD, Wikidata, schema.org, RDF roundtrip, inference enrichment |

---

## Quick Reference

### CLI Synopsis

```
engram create [path]                         Create .brain file (default: knowledge.brain)
engram stats [path]                          Print node and edge counts
engram store <label> [path]                  Store a node (default confidence 0.80)
engram set <label> <key> <value> [path]      Set a string property
engram relate <from> <rel> <to> [path]       Create a directed edge
engram query <label> [depth] [path]          Query node + BFS traversal
engram search <query> [path]                 BM25 search with optional filters
engram delete <label> [path]                 Soft-delete a node
engram serve [path] [addr]                   Start HTTP API (default addr: 0.0.0.0:3030)
engram mcp [path]                            Start MCP server over stdio
engram reindex [path]                        Rebuild embedding index after model change
```

### Search Filter Syntax

```
engram search "postgresql"                   Full-text keyword search
engram search "confidence>0.8"               Nodes above confidence threshold
engram search "prop:role=database"           Nodes with a specific property value
engram search "tier:active"                  Nodes in a specific memory tier
engram search "type:server"                  Nodes with a specific entity type
engram search "type:server AND confidence>0.5"  Boolean AND
engram search "postgresql OR mysql"          Boolean OR
engram search "database NOT mysql"           Boolean NOT
```

### HTTP API Endpoints

| Method | Path | Purpose |
|---|---|---|
| POST | /store | Store a node with optional type, properties, confidence |
| POST | /relate | Create a directed edge between two nodes |
| POST | /query | BFS traversal from a start node |
| POST | /search | BM25 full-text search |
| POST | /similar | Hybrid search (BM25 + embedding if available) |
| POST | /ask | Natural language query |
| POST | /tell | Natural language fact assertion |
| GET | /node/{label} | Get a node with all edges and properties |
| DELETE | /node/{label} | Soft-delete a node |
| GET | /explain/{label} | Full explanation: confidence, cooccurrences, all edges |
| POST | /learn/reinforce | Boost confidence (access or confirmation) |
| POST | /learn/correct | Apply contradiction penalty and propagate |
| POST | /learn/decay | Apply time-based decay to all nodes |
| POST | /learn/derive | Run inference rules and create derived edges |
| GET | /health | Health check |
| GET | /stats | Node and edge counts |
| POST | /batch | Bulk store entities and relationships |
| GET | /export/jsonld | Export graph as JSON-LD linked data |
| POST | /import/jsonld | Import JSON-LD data into the graph |
| POST | /rules | Load push-based inference rules |
| GET | /rules | List loaded rules |
| DELETE | /rules | Clear all loaded rules |
| GET | /compute | Hardware and embedder info |
| GET | /tools | MCP tool definitions |

### Confidence Source Table

| Source | Initial Confidence | Maximum Cap |
|---|---|---|
| sensor | 0.95 | 0.99 |
| api | 0.90 | 0.95 |
| user | 0.80 | 0.95 |
| derived | 0.50 | 0.80 |
| llm | 0.30 | 0.70 |
| correction | 0.90 | 0.95 |

### Rule Syntax Reference

Rules are submitted as multi-line strings to `/learn/derive`:

```
rule <name>
when edge(<var>, "<relationship>", <var>)
when prop(<var>, "<key>", "<value>")
when confidence(<var>, "<op>", <threshold>)
then edge(<var>, "<relationship>", <var>, min(<e1>, <e2>))
then edge(<var>, "<relationship>", <var>, product(<e1>, <e2>))
then edge(<var>, "<relationship>", <var>, <literal_float>)
then flag(<var>, "<reason>")
```

Operators for confidence conditions: `>`, `>=`, `<`, `<=`.

Multiple `when` lines are AND-ed together. All conditions must match for the rule to fire.
