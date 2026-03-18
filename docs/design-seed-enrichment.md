# Seed Enrichment Pipeline Design

**Date**: 2026-03-14 (updated 2026-03-15)
**Status**: IMPLEMENTED (v1.1.0) -- replaces old 6-phase approach
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

### Step 0: Identify Area of Interest (MOST IMPORTANT STEP)

This is the organizing principle for everything. Must be accurate.

The area of interest is NOT limited to events. It's the **domain lens** -- the context
through which all entities are connected. Examples:

- "Russia Ukraine war" (geopolitical event)
- "semiconductor supply chain" (industry)
- "protein folding mechanisms" (science)
- "competitor landscape in fintech" (business)
- "my family history in Bavaria" (personal)

**Strategy cascade (try in order):**

1. **LLM extraction** (mandatory): Ask the local LLM:
   "What is the primary area of interest or domain in this text? Reply with a short topic phrase."
   Input: seed text. Output: "Russia-Ukraine war" or "semiconductor supply chain".
   Most reliable for any domain. Uses Ollama configured in wizard.

2. **First sentence analysis** (fallback if LLM unavailable): The first sentence usually states the domain.
   "I track the Russia Ukraine war..." → extract after "I track" / "I monitor" / "I analyze" / "I research".

3. **User confirmation** (always): Show the detected area of interest in the wizard seed step
   and let the user confirm or edit. "Detected area of interest: Russia Ukraine war [Edit]"

- Store as graph property: `area_of_interest = "Russia Ukraine war"`
- This drives ALL subsequent enrichment
- Wrong area of interest = wrong connections = useless graph

```
Input:  "I track the Russia Ukraine war across military..."
Output: area_of_interest = "Russia Ukraine war" (confirmed by user)
```

**Wizard integration**: The seed step should show:
1. Text input for seed
2. Auto-detected "Area of Interest" field (editable)
3. "Analyze" previews entities + area of interest
4. "Seed KB" starts enrichment with confirmed area of interest

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
  │     Step 2: Fetch area-of-interest article → co-occurrence PAIRS (~1s)
  │             + individual search for unmentioned entities (~2s)
  │             + web search fallback for still-unconnected entities (~2s)
  │             NOTE: article text is retained for Step 5
  │     Step 3: Batch SPARQL direct connections (~2s)
  │     Step 3b: Property expansion (~2s)
  │              Guards: empty propLabel and QID valueLabel bindings are skipped
  │     Step 3c: Shortest path fallback for remaining islands (~2s)
  │     Step 5: GLiNER2 classifies unresolved co-occurrence pairs (~1s)
  │              Uses the article text from Step 2 (not the caller's input text)
  │              Pairs without GLiNER2 match fall back to "related_to"
  │
  ├── Post-process: upgrade "related_to" → typed relation where SPARQL found one
  │   + dedup by (head, tail, type), keeping highest confidence
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

## Web Search Integration

Web search should be available from the start -- configured in the setup wizard alongside
the LLM and embedding model. Used for:

1. **Entity context enrichment** (Step 2c): when Wikipedia doesn't have a direct article,
   web search for "{entity} {area_of_interest}" finds news articles, analysis pieces
   that contain factual connections.

2. **Entity disambiguation** (Step 1): if Wikipedia returns ambiguous results, web search
   for "{entity} {area_of_interest}" naturally returns the right person/thing.

3. **Current events**: Wikipedia may not have the latest events. Web search catches
   recent developments (e.g., latest weapons deliveries, diplomatic meetings).

**Wizard setup**: Step 2 (or new step) should configure web search:
- Provider: DuckDuckGo (no API key), Brave Search (API key), SearXNG (self-hosted)
- Or: use MCP web search tool if available
- Store in config: `web_search_provider`, `web_search_api_key`

## Configuration

```json
{
  "seed_enrichment": {
    "enabled": true,
    "skip_wikidata": false,
    "property_expansion": true,
    "shortest_path_fallback": true,
    "max_wikipedia_fetches": 25,
    "interactive_disambiguation": false,
    "web_search_provider": "duckduckgo",
    "use_llm_for_area_of_interest": true
  }
}
```

For private/internal domains: set `skip_wikidata: true` and `enabled: false`.
Seed will only use GLiNER2 NER+RE, no external enrichment.

---

## Mandatory Prerequisites (Setup Wizard)

LLM and web search are NOT optional -- they're required for quality seed enrichment.
The setup wizard must configure both BEFORE allowing seed:

1. **LLM** (mandatory): Ollama endpoint + model. Used for area-of-interest extraction,
   entity disambiguation confirmation, and relationship classification.
   Wizard step: "Configure Language Model" -- cannot proceed without a working LLM.

2. **Web Search** (mandatory): DuckDuckGo (default, no key), Brave, or SearXNG.
   Used for contextual entity enrichment and current events.
   Wizard step: "Configure Web Search" -- defaults to DuckDuckGo if no API key provided.

3. **Wikidata** (default on): SPARQL endpoint for structured knowledge.
   Wizard step: "Knowledge Sources" -- enabled by default.

Without LLM + web search, the seed produces a mediocre graph. With them, it produces
a "wow" graph. Make them mandatory, not optional.

---

## Human-in-the-Loop (ALL steps)

The seed enrichment is NOT a black box. Every step shows results and asks for confirmation
via SSE streaming to the frontend. The user stays engaged and in control.

### Step 0: Area of Interest
- LLM proposes: "Russia-Ukraine war"
- UI shows: "Detected area of interest: **Russia-Ukraine war** [Confirm] [Edit]"
- User confirms or edits before proceeding

### Step 1: Entity Identification
- For each entity, show the Wikipedia/Wikidata match:
  ```
  Putin         → Vladimir Putin (President of Russia)     [OK] [Change]
  Macron        → Emmanuel Macron (President of France)    [OK] [Change]
  HIMARS        → M142 HIMARS (rocket launcher)            [OK] [Change]
  EU            → European Union (political union)         [OK] [Change]
  drones        → Unmanned aerial vehicle                  [OK] [Change] [Skip]
  oil           → Petroleum                                [OK] [Change] [Skip]
  ```
- Ambiguous entities show choices:
  ```
  China         → ?
    [ ] People's Republic of China (country)
    [ ] China national football team
    [ ] China (porcelain)
    [Select]
  ```
- User confirms all before proceeding

### Step 2: Contextual Connections
- Show discovered connections as they're found (SSE streaming):
  ```
  Finding connections via "Russo-Ukrainian war" article...
  ✓ Putin ← participant → Russia Ukraine war
  ✓ Zelensky ← participant → Russia Ukraine war
  ✓ NATO ← military support → Ukraine
  ✓ HIMARS ← weapon used → Russia Ukraine war
  ✓ Leopard 2 ← military aid → Ukraine
  ...
  [Accept All] [Review] [Skip]
  ```

### Step 3: SPARQL Structured Relations
- Show typed relations found:
  ```
  Wikidata relations:
  ✓ Putin → citizen_of → Russia
  ✓ Macron → citizen_of → France
  ✓ NATO → headquartered_in → Brussels
  ✓ HIMARS → manufactured_by → Lockheed Martin
  ...
  [Accept All] [Review]
  ```

### Progress Bar
- Overall: "Building knowledge graph... Step 2/4 (connecting entities)"
- Per-entity: "Linking entities (14/20)..."
- Time estimate: "~15 seconds remaining"

This turns a 25-second wait into an interactive experience where the user
sees the graph being built and can correct mistakes in real-time.

---

## Implementation Notes

- `rel_knowledge_base/mod.rs`: 4-step enrichment + Step 5 GLiNER2 classification
- `rel_knowledge_base/sparql.rs`: batch SPARQL, property expansion, shortest paths
- `rel_knowledge_base/entity_link.rs`: Wikipedia entity linking
- Key methods: `fetch_area_of_interest_article()`, `find_cooccurrences()`,
  `batch_relation_lookup()`, `property_expansion()`, `batch_shortest_paths()`

## Data Quality Guards (added 2026-03-18)

| Guard | Location | Problem | Fix |
|-------|----------|---------|-----|
| Empty rel_type | `wikidata_prop_to_rel_type()` | Wikidata returns no label -> empty type_id 0 | Return "related_to" for empty input |
| Empty propLabel | `property_expansion()` | SPARQL binding has empty/URI-only propLabel | Skip binding |
| QID valueLabel | `property_expansion()` | Label service fails, returns "Q38715852" | Skip values matching `Q\d+` pattern |
| Empty type_id | `TypeRegistry::get_or_create()` | Empty string creates type_id 0 | Redirect "" to "related_to" |
| GLiNER2 empty text | `extract_relations()` Step 5 | Caller passes empty text, GLiNER2 returns nothing | Use article text from Step 2 |
| Edge label display | `graph_canvas.rs` + `graph-bridge.js` | Empty label = no display | Fallback to "related_to" |

## Relation Review UI (updated 2026-03-18)

All confidence tiers (confirmed, likely, uncertain, no_relation) now show editable
dropdown selectors. Previously only `no_relation` had a dropdown; other tiers showed
read-only text. Backend infrastructure (`SeedConfirmRelationsRequest.modified`) already
existed; the UI now uses it for all tiers.
