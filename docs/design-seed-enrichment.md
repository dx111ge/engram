# Seed Enrichment Pipeline Design

**Date**: 2026-03-14
**Status**: DESIGN (replaces current over-engineered 6-phase approach)
**Goal**: From a descriptive seed text, produce a richly connected knowledge graph in <30s

---

## Problem

User writes: "I track the Russia Ukraine war... Key actors include Russia, Ukraine, NATO...
Key figures include Zelensky, Putin, Lavrov, Stoltenberg, and Macron..."

Current result: 69 nodes, 99 edges but disconnected subgraphs. Leopard 2 connected to
its manufacturers but not to the war. HIMARS isolated. Over-engineered with 6 tangled phases,
duplicate canonical nodes, label mapping gymnastics.

Expected result: ALL entities connected through the stated area of interest. Leopard 2 connected
to Ukraine (delivered as military aid), HIMARS connected to the war, Macron connected to
diplomatic negotiations. One coherent graph, not islands.

---

## Architecture: 4 Steps

### Step 0: Extract Area of Interest

Parse the seed text for the DOMAIN/TOPIC. This is the organizing principle for everything.

- Primary: use EVENT entities from GLiNER2 NER (e.g., "Russia Ukraine war")
- Fallback: first sentence of seed text, or top-3 entity labels concatenated
- Store as graph property: `area_of_interest = "Russia Ukraine war"`
- This drives ALL subsequent enrichment

```
Input:  "I track the Russia Ukraine war across military..."
Output: area_of_interest = "Russia Ukraine war"
```

### Step 1: Identify Entities via Wikipedia

For each NER entity, resolve to Wikidata QID via Wikipedia search.

- Search Wikipedia for entity label alone (not with type -- "Macron" returns Emmanuel Macron)
- Get canonical name from Wikipedia page title
- Get QID from page props
- Store on the NER node as properties: `kb_id:wikidata`, `canonical_name`
- Do NOT create separate canonical nodes -- use NER labels for all edges

```
"Putin"       → Wikipedia → "Vladimir Putin"    → Q7747
"HIMARS"      → Wikipedia → "M142 HIMARS"       → Q2495802
"Macron"      → Wikipedia → "Emmanuel Macron"   → Q3052772
```

**Ambiguity handling** (future SE.1): if Wikipedia returns an unexpected result (e.g., "Putin" → 2024 film),
show candidates to user for confirmation. For now, plain label search works for well-known entities.

### Step 2: Connect via Area of Interest (THE KEY STEP)

This is the primary connection mechanism. Two sub-steps:

**2a: Fetch the area-of-interest article**

Fetch the Wikipedia article for the area of interest ("Russo-Ukrainian war").
This ONE article typically mentions most seed entities in context.

```
GET https://en.wikipedia.org/w/api.php?action=query&titles=Russo-Ukrainian_war
    &prop=extracts&explaintext=true&exchars=5000&format=json
```

**2b: Entity co-occurrence in context**

Scan the article text for mentions of each seed entity (case-insensitive substring match).
When two seed entities co-occur in the same paragraph, create a "related_to" edge.

```
Article paragraph: "Western allies provided Ukraine with military aid including
HIMARS rocket systems and Leopard 2 tanks from Germany."

→ HIMARS  related_to  Ukraine
→ Leopard 2  related_to  Ukraine
→ Leopard 2  related_to  Germany  (if Germany is a seed entity)
```

**2c: Individual context search for unmentioned entities**

For seed entities NOT found in the main article, search Wikipedia for
"{entity} {area_of_interest}" and scan THAT article for co-occurrences.

```
"Stoltenberg Russia Ukraine war" → article about NATO's response
→ mentions NATO, Ukraine, Russia → edges created
```

**Why this works**: Wikipedia articles about geopolitical events naturally contain
the factual connections between all relevant actors, weapons, countries, and organizations.
One article fetch replaces 20+ SPARQL queries.

### Step 3: SPARQL Structured Connections

One batch query for ALL linked QID pairs. Finds typed Wikidata properties.

```sparql
SELECT ?s ?sLabel ?p ?pLabel ?o ?oLabel WHERE {
  VALUES ?s { wd:Q7747 wd:Q159 wd:Q212 wd:Q7184 ... }
  VALUES ?o { wd:Q7747 wd:Q159 wd:Q212 wd:Q7184 ... }
  ?s ?prop ?o .
  ?p wikibase:directClaim ?prop .
  FILTER(?s != ?o)
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en" }
} LIMIT 500
```

This adds structured relations: citizen_of, headquartered_in, country, etc.
Runs AFTER Step 2 so the graph already has contextual connections.

### Step 3b: Property expansion (configurable)

For each linked entity, fetch key Wikidata properties (P39 position_held,
P17 country, P176 manufacturer, P36 capital, etc.). Creates new nodes
for discovered entities (Moscow, Lockheed Martin, etc.) with typed edges.

One SPARQL query for all entities.

### Step 3c: Shortest path fallback

For entity pairs STILL disconnected after Steps 2-3b:
- Batch 1-hop SPARQL: find intermediate entities connecting any pair
- Only for pairs with no connection whatsoever
- Creates bridge nodes

---

## What to Remove (from current implementation)

| Remove | Why |
|--------|-----|
| Canonical node creation (same_as edges) | Creates duplicate nodes. Store canonical as property instead. |
| label_map / canonical_names HashMap | NER labels used for all edges. No mapping needed. |
| Phase 3 pairwise SPARQL fallback | Batch query + contextual enrichment handles this. |
| 6-phase numbering | Replaced by 4 clean steps. |
| `entity_type` as Wikipedia search context | Plain label works better ("Macron" finds Emmanuel Macron, "Macron person" finds Brigitte). |

## What to Keep

| Keep | Why |
|------|-----|
| Wikipedia search for entity linking | Works perfectly for well-known entities. |
| Batch SPARQL for structured relations | One query, efficient, gives typed edges. |
| Property expansion | Discovers manufacturers, capitals, positions -- adds depth. |
| Shortest path as fallback | Safety net for truly disconnected pairs. |
| Auto-create nodes for new entities | Property expansion discovers new entities (Lockheed Martin). |
| Two-pass entity/relation write | Ensures nodes exist before edges are created. |

---

## API Flow

```
POST /ingest (seed text)
  │
  ├── GLiNER2 NER: extract entities (20 entities)
  │
  ├── KB Enrichment:
  │     Step 0: area_of_interest = "Russia Ukraine war"
  │     Step 1: Wikipedia link each entity → QID + canonical (20 API calls, ~2s)
  │     Step 2: Fetch area-of-interest article → co-occurrence edges (~1s)
  │             + individual search for unmentioned entities (~2s)
  │     Step 3: Batch SPARQL direct connections (~2s)
  │     Step 3b: Property expansion (~2s)
  │     Step 3c: Shortest path fallback for remaining islands (~2s)
  │
  ├── Two-pass write: entities first, then relations
  │
  └── Result: ~60-80 nodes, 100+ edges, fully connected graph (~25s total)
```

---

## Expected Outcome

For the Russia Ukraine war seed text:

| Entity | Expected Connections |
|--------|---------------------|
| Putin | → Russia (citizen), → Russia Ukraine war (participant), → President of Russia (position) |
| Zelensky | → Ukraine (citizen), → Russia Ukraine war (participant), → President of Ukraine (position) |
| NATO | → Brussels (HQ), → Stoltenberg (Secretary General), → Ukraine (support) |
| HIMARS | → United States (origin), → Lockheed Martin (manufacturer), → Ukraine (delivered to, via context) |
| Leopard 2 | → Germany (origin), → Ukraine (delivered to, via context), → KNDS (manufacturer) |
| Macron | → France (citizen), → President of France (position), → Russia Ukraine war (diplomatic role, via context) |
| F-16 | → United States (origin), → Lockheed Martin (manufacturer), → Ukraine (pledged, via context) |

---

## Configuration

```json
{
  "seed_enrichment": {
    "enabled": true,
    "skip_wikidata": false,
    "property_expansion": true,
    "shortest_path_fallback": true,
    "max_wikipedia_fetches": 25,
    "interactive_disambiguation": false
  }
}
```

For private/internal domains: set `skip_wikidata: true`. Seed will only use GLiNER2 NER+RE,
no Wikipedia/Wikidata enrichment.

---

## Implementation Notes

- `rel_knowledge_base.rs` to be rewritten with clean 4-step structure
- Remove canonical node creation, label_map, pairwise fallback
- Keep `wikipedia_search_qid()` and `batch_relation_lookup()` methods
- Add `fetch_area_of_interest_article()` and `find_cooccurrences()` methods
- Step 2 is the new core -- contextual connection via Wikipedia article text
- Estimated implementation: 2-3 hours
