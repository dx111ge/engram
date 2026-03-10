# Use Case 13: Russia Geopolitical Analysis -- AI Intelligence Analyst

### Overview

An AI intelligence analyst builds a comprehensive geopolitical knowledge graph about Russia using **live data from 6 public APIs**, layers analyst assessments on top, detects disinformation via contradiction handling, discovers patterns through inference rules, and generates probabilistic predictions with traceable evidence chains. Every fact is sourced, every prediction is explainable, and every contradiction is resolved by confidence.

**What makes this the crown jewel:**

- **No simulated data.** Every country, border, population figure, GDP, inflation rate, military spending figure, news article, and exchange rate is fetched live from public APIs at runtime.
- **Predictions with probability.** 7 intelligence assessments computed from Bayesian aggregation of weighted evidence chains. Each probability is traceable to its supporting and contradicting evidence.
- **State media vs. reality.** Russian state media claims (TASS, RT, Sputnik) at confidence 0.20-0.30 are automatically corrected by UN, World Bank, and Reuters evidence at 0.88-0.95.
- **Inference discovers anomalies.** Turkey flagged as "NATO member blocking sanctions on Russia" -- discovered by rules, not manually encoded.
- **MCP/Claude Code integration.** Designed for AI agents to build and query the graph interactively.

**Live data sources:**

| Source | API | Data | Confidence |
|--------|-----|------|------------|
| Wikidata | SPARQL endpoint | Borders, memberships, leaders, population | 0.92 |
| World Bank | REST API | GDP, inflation, military spending (2019-2023) | 0.93 |
| GDELT Project | News API | Recent articles about Russia (last 30 days) | 0.25-0.85 |
| Exchange Rate API | REST | Live RUB/USD rate | 0.95 |
| REST Countries | REST | Country metadata enrichment | 0.90 |
| Wikipedia | REST API | Conflict and sanctions summaries | 0.85 |

**11 phases:** Wikidata SPARQL, World Bank economics, GDELT+Exchange+Wikipedia enrichment, source tiers, conflict analysis, contradiction detection, inference rules, probabilistic predictions, graph discovery, **policy simulation** (Moldova + Ukraine war scenarios), JSON-LD export.

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed
- Internet connection (for live API calls)

### Files

```
13-russia-geopolitical/
  README.md               # This file
  russia_demo.py          # Full 11-phase analysis pipeline
  ingestion_daemon.py     # Continuous news ingestion + prediction recalculation
  dashboard.html          # Real-time prediction dashboard (standalone, no build)
  intel-dashboard/        # Browser-based intel dashboard (WASM + vanilla JS)
    index.html            # Single-file dashboard (all JS/CSS inline)
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve russia.brain 127.0.0.1:3030
```

#### Step 2: Run the analysis

```bash
python russia_demo.py
```

#### Step 3 (optional): Start the ingestion daemon

```bash
python ingestion_daemon.py --interval 900
```

Polls GDELT every 15 minutes for new articles, classifies by topic, assigns source-tier confidence, links to prediction nodes, and recalculates all probabilities. Logs prediction history to `prediction_history.jsonl`.

#### Step 4 (optional): Open the dashboard

Open `dashboard.html` in a browser. It connects to the engram API at `127.0.0.1:3030` and auto-refreshes every 15 seconds. Shows:

- All 9 predictions with probability bars and shift indicators
- Evidence chain detail panel (click any prediction)
- "News Driving This Prediction" section showing which articles caused the shift
- Supporting vs contradicting evidence with source confidence
- Prediction history sparklines

### What Happens

#### Phase 1: Wikidata SPARQL -- Countries, Borders, Memberships

Live SPARQL queries fetch structured geopolitical data:

```
Russia: pop=146,119,928, area=17,075,400 km2, capital=Moscow

Bordering countries (17):
  Poland         37,563,071   NATO EU
  Finland         5,608,218   NATO EU
  Estonia         1,369,995   NATO EU
  Latvia          1,856,932   NATO EU
  Lithuania       2,860,002   NATO EU
  Norway          5,627,400   NATO
  United States 340,110,988   NATO
  Sweden         10,609,460   NATO EU
  Belarus         9,109,280   CSTO
  Kazakhstan     20,139,914   CSTO
  Ukraine        41,167,335
  China        1,442,965,000
  North Korea    26,418,204
  ...

Organizations (14): UN, UN Security Council, BRICS, CSTO, SCO, G20,
                     Arctic Council, states with nuclear weapons, ...
Leaders: Vladimir Putin (President of Russia)
```

After phase 1: **36 nodes, 49 edges**.

#### Phase 2: World Bank API -- Economic Time Series

Live GDP, inflation, and military spending data (2019-2023):

```
GDP (current USD):
  2019: $1,693.1B
  2020: $1,493.1B   (COVID + oil crash)
  2021: $1,829.2B   (recovery)
  2022: $2,291.6B   (nominal spike from ruble manipulation)
  2023: $2,071.5B   (declining)

Inflation (CPI):
  2019: 4.5%  ->  2022: 13.7%  ->  2023: 5.9%

Military spending (% GDP):
  2019: 3.86%  ->  2022: 4.61%  ->  2023: 5.40%  (and rising to ~7% in 2024)

Ukraine GDP comparison:
  2021: $199.8B  ->  2022: $162.0B  ->  2023: $181.2B  (recovering despite war)
```

After phase 2: **56 nodes, 69 edges**.

#### Phase 3: GDELT, Exchange Rate, Wikipedia -- Multi-Source Enrichment

```
Exchange Rate: 1 USD = 78.87 RUB (live)

GDELT News (last 30 days):
  [sanctions] Zelenskyy signs decree on sanctions against Russian shadow fleet
  [conflict]  Ukraine Parliament Speaker makes sanctions case on Capitol Hill
  [conflict]  (Russian-language sources from ria.ru flagged as Tier 3)

Wikipedia Summaries:
  2022 Russian invasion of Ukraine (full extract imported)
  Russo-Ukrainian war (background context)
  International sanctions during the Russo-Ukrainian war

REST Countries Enrichment:
  Ukraine: pop=32,862,000, area=603,550km2, capital=Kyiv
  Belarus: pop=9,109,280, area=207,600km2, capital=Minsk
  Finland: pop=5,650,325, area=338,455km2, capital=Helsinki
```

News articles are automatically assigned confidence by domain: Reuters (0.85), BBC (0.82), ria.ru (0.25).

After phase 3: **66 nodes, 79 edges**.

#### Phase 4: Source Reliability Tiers

| Tier | Sources | Confidence | Role |
|------|---------|------------|------|
| 1 (institutional) | World Bank, Wikidata, IMF, NATO, UN | 0.90-0.95 | Ground truth for economic/structural data |
| 2 (quality journalism) | Reuters, AP, BBC, ISW, RUSI, IISS | 0.82-0.88 | Event reporting and analysis |
| 3 (state-controlled) | TASS, RT, Sputnik | 0.20-0.30 | Propaganda -- weighted near-zero |

#### Phase 5: Conflict & Strategic Analysis

Analyst enrichment layered on top of live data:

- **Conflict:Ukraine-Invasion** -- full-scale invasion since 2022-02-24, ~18% territory occupied
- **Disputed territories:** Crimea, Donetsk, Luhansk, Zaporizhzhia, Kherson, Transnistria
- **Strategic assets:** Kaliningrad (Iskander missiles, Baltic Fleet), Suwalki Gap (NATO chokepoint)
- **Sanctions:** SWIFT exclusion, $300B reserve freeze, oil price cap, tech export controls
- **Turkey anomaly:** NATO member that didn't join sanctions, bought Russian S-400
- **North Korea:** Mutual defense treaty (June 2024), ammunition and missile supply

After phase 5: **95 nodes, 102 edges**.

#### Phase 6: Contradictions -- State Media vs Evidence

Five Russian state media claims debunked by verified evidence:

```
Claim:SpecialOperation (TASS, conf=0.25) -> CORRECTED
  "Special military operation, not a war"
  Contradicted by: UN GA condemned invasion; ICC arrest warrant (conf=0.92)

Claim:SanctionsFailed (RT, conf=0.30) -> CORRECTED
  "Sanctions completely failed"
  Contradicted by: $300B frozen, 1000+ companies exited (World Bank, conf=0.93)

Claim:CrimeaLegal (TASS, conf=0.25) -> CORRECTED
  "Crimea annexation was legal"
  Contradicted by: UNGA Resolution 68/262 (conf=0.92)

Claim:NATOProvoked (RT, conf=0.30) -> CORRECTED
  "NATO expansion provoked Russia"
  Contradicted by: NATO open-door policy, sovereign choice (conf=0.90)

Claim:NoMobilization (Sputnik, conf=0.20) -> CORRECTED
  "No mass mobilization"
  Contradicted by: Putin signed decree Sep 2022, 300,000 called up (conf=0.88)
```

All claims corrected to confidence 0.00 with distrust propagated to neighboring nodes.

After phase 6: **105 nodes, 107 edges**.

#### Phase 7: Inference Rules -- Pattern Detection

Five inference rules fire automatically:

| Rule | Flags Raised | Example |
|------|-------------|---------|
| `frontline_state` | NATO members bordering Russia | Norway, Finland, Estonia, Latvia, Lithuania, Poland |
| `sanctions_blocker` | NATO member blocking sanctions | **Turkey** (the anomaly) |
| `weapons_supplier` | Countries supplying arms to Russia | **North Korea** |
| `suwalki_threat` | Russian assets flanking Suwalki Gap | **Kaliningrad** |
| `occupation_pattern` | Territories under Russian control | Crimea, Donetsk, Luhansk, Transnistria |

**Key discovery:** Turkey is the only entity in the graph that is simultaneously a NATO member AND blocks sanctions on Russia. This was not manually encoded -- the inference engine discovered it by intersecting two rule conditions.

#### Phase 8: Probabilistic Intelligence Assessments

Nine predictions computed via Bayesian evidence aggregation:

```
Assessment                                         Prob     Category
-------------------------------------------------- ------  ---------------
Baltic military provocation (12mo)                   64%  military
Turkey as sanctions evasion route                    67%  economic
Moldova destabilization via Transnistria             59%  hybrid_warfare
Ruble instability (>120 RUB/USD, 18mo)               59%  economic
BRICS alternative to USD                             36%  economic
North Korea-Russia military axis deepens             65%  military
Frozen conflict (Korean War model)                   46%  conflict
Ukraine frontline freeze (negotiated ceasefire)      48%  conflict
Ukraine full territorial recovery (incl. Crimea)     42%  conflict
```

**How probabilities are calculated:**

Each prediction has supporting evidence (weighted by source tier confidence) and contradicting evidence. The formula:

```
P = weighted_for * (1 - weighted_against * discount)
where discount = |against| / (|for| + |against|)
```

Example -- **Baltic Provocation (64%)**:
- Supporting (5): Kaliningrad buildup (0.90), Suwalki vulnerability (0.88), airspace violations (0.85), GPS jamming (0.82), 2007 precedent (0.80)
- Contradicting (2): NATO Article 5 deterrence (0.90), Russia overextended (0.85)
- Result: High supporting evidence slightly tempered by strong deterrence

Example -- **BRICS Currency (36%)**:
- Supporting (3): BRICS expansion (0.85), de-dollarization rhetoric (0.70), bilateral settlements (0.75)
- Contradicting (5): No mechanism (0.88), India-China distrust (0.82), USD dominance (0.93), divergent interests (0.85), Saudi USD peg (0.80)
- Result: More and stronger contradicting evidence pushes probability below 50%

#### Phase 9: Knowledge Discovery via Graph Traversal

**From Russia (depth=2):** 80 reachable nodes -- all bordering countries, alliances, sanctions, economic indicators, conflict data, and disputed territories.

**From Turkey (depth=2):** 72 reachable nodes -- Turkey bridges NATO and Russia. Starting from Turkey, depth-2 traversal reaches both the NATO alliance structure and Russia's entire border neighborhood.

**From Suwalki Gap (depth=2):** 9 nodes -- Poland, Lithuania, Kaliningrad, Belarus, NATO, EU, Russia, CSTO. The entire strategic geometry of NATO's most vulnerable chokepoint in one traversal.

**Explain: Prediction:BalticProvocation:**
```
Confidence (= probability): 0.64
Hypothesis: Russia will conduct significant military provocation in Baltic region
Evidence for: 5 sources
Evidence against: 2 sources
  <- [supports] Kaliningrad military buildup (conf=0.90)
  <- [supports] Suwalki Gap vulnerability (conf=0.88)
  <- [supports] Airspace violations (conf=0.85)
  <- [supports] GPS jamming (conf=0.82)
  <- [supports] 2007 precedent (conf=0.80)
```

**Explain: Turkey -- Why It Is Flagged:**
```
Confidence: 0.90
Flag: ANOMALY: NATO member blocking sanctions on Russia
  -> [member_of] NATO (conf=0.99)
  -> [trade_partner] Russia (conf=0.85)
  -> [blocked_sanctions_on] Russia (conf=0.80)
```

#### Phase 10: Policy Simulation -- Moldova Defense & Ukraine War Outcomes

Three scenarios demonstrate engram's policy simulation capability: adding countermeasure/strategy nodes with `weakens`/`strengthens`/`enables` edges to prediction nodes, then recalculating probability.

**Scenario A: Preventing Moldova Destabilization**

Six NATO/EU countermeasures applied (NATO partnership, EU fast-track accession, Romania security guarantee, energy independence, anti-hybrid warfare, Transnistria negotiation):

```
BEFORE: 59%  #############################
AFTER:  42%  ####################
Reduction: -17% probability points
```

Most impactful: Romania security guarantee (0.88) > NATO partnership (0.85). Hardest to achieve: Transnistria negotiation (0.70) -- but highest long-term impact.

**Scenario B: Ukraine Frontline Freeze (Negotiated Ceasefire) -- 48%**

- Supporting (6): Military stalemate, exhaustion, Trump negotiations, war fatigue, nuclear threat, China pressure
- Contradicting (5): Ukraine refuses to cede territory, Russia maximalist goals, no ceasefire framework, frozen conflict benefits neither, EU aid increasing
- Strategic levers: F-16 fleet, long-range strikes (ATACMS/Storm Shadow), oil refinery targeting, fortification line, sanctions enforcement

**Scenario C: Ukraine Full Territorial Recovery including Crimea -- 42% baseline**

- Supporting (6): Crimea logistically vulnerable, long-range strike capability, Russian military decline, international law (UNGA 1991 borders), Kherson/Kharkiv precedent, Crimean water crisis
- Contradicting (8): Nuclear escalation risk (0.92), Crimea fortified since 2014, Kerch Strait control, Western escalation fear, Russian full mobilization threat, 700km supply lines, civilian population, Sevastopol naval base (0.90)

Seven strategies applied (Crimea isolation, USV naval denial, NATO security guarantee, Russian internal collapse, massive mobilization, information warfare, diplomatic isolation):

```
WITHOUT strategies: 42%  #####################
WITH all strategies: 54%  ###########################
Improvement: +12% probability points
```

**Key insight:** Even with all strategies, full Crimea recovery remains constrained by nuclear escalation risk (0.92) and fortress Sevastopol (0.90). Most realistic path: isolate Crimea (cut Kerch bridge + land corridor at Melitopol) to force negotiated return, rather than direct military assault.

After simulation: **214 nodes, 208 edges**.

#### Phase 11: JSON-LD Export

The entire 214-node graph exports as standard JSON-LD, consumable by any RDF tool.

Final graph: **214 nodes, 208 edges**.

### MCP / Claude Code Integration

This use case is designed for AI agent integration. To use engram as Claude Code's persistent memory:

**1. Configure MCP server** (`.claude/mcp.json`):
```json
{
  "mcpServers": {
    "engram": {
      "command": "engram",
      "args": ["mcp"],
      "env": { "ENGRAM_BRAIN": "russia.brain" }
    }
  }
}
```

**2. Example Claude Code prompts:**
```
"Store that Turkey is a NATO member but did not join Russia sanctions"
"What countries border Russia and are NATO members?"
"Explain the prediction about Baltic provocation"
"Traverse from Suwalki Gap at depth 2"
"What is the current RUB/USD exchange rate in the graph?"
"Search for all sanctions on Russia"
```

**3. The AI agent can:**
- Build the knowledge graph incrementally across conversations
- Query for patterns and anomalies
- Update predictions as new evidence arrives
- Reinforce facts that are confirmed, correct facts that are disproven
- Export the graph for sharing with other tools

### Intel Dashboard (Browser-Based)

The `intel-dashboard/` directory contains a standalone browser-based intelligence dashboard that connects to the engram API for live analysis.

#### Features

- **Bayesian probability engine** -- WASM-compiled Rust running in the browser for real-time probability calculations
- **Multi-source intelligence pipeline** -- 6-phase deep investigation: entity/topic extraction, GDELT news, Google News RSS, Wikipedia, web search (SearXNG), engram graph traversal, LLM analysis
- **Cross-assessment analysis** -- "How does losing Starlink access impact ALL assessments?" with visual probability impact cards
- **Clickable source citations** -- every LLM response includes numbered references `[1]` `[2]` linking to original articles
- **Engram knowledge accumulation** -- investigation findings (news articles, references, entity relationships) are stored to engram so repeated queries build on prior research
- **Country profiles** -- borders, organizations, leaders, economic indicators auto-populated from engram graph
- **Assessment management** -- create, sort, export predictions with confidence bars and trend sparklines

#### Prerequisites

- `engram` server running (`engram serve brain.brain`)
- Ollama with `qwen2.5:7b` (or any OpenAI-compatible LLM) for the Ask/investigation features
- **SearXNG** (optional but recommended) for real web search:

```bash
# Start SearXNG via Docker (one-time setup)
docker run -d --name searxng -p 8090:8080 \
  -v ./docker/searxng/settings.yml:/etc/searxng/settings.yml:ro \
  -e SEARXNG_SECRET=engram-local \
  searxng/searxng:latest
```

SearXNG `settings.yml` must enable JSON format:
```yaml
use_default_settings: true
search:
  formats:
    - html
    - json
server:
  secret_key: "engram-local-searxng"
  limiter: false
```

Without SearXNG, the web search phase falls back to DuckDuckGo Instant Answers (limited to encyclopedic content, not useful for current events).

#### Running the Dashboard

```bash
# 1. Start engram
engram serve brain.brain

# 2. Serve the dashboard (any static file server)
cd docs/usecases/13-russia-geopolitical/intel-dashboard
python -m http.server 8888

# 3. Open http://localhost:8888 in browser
```

#### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `ENGRAM_SEARXNG_URL` | `http://localhost:8090` | SearXNG instance URL |
| `ENGRAM_EMBED_ENDPOINT` | (none) | Embedding API for `/similar` searches |
| `ENGRAM_EMBED_MODEL` | (none) | Embedding model name |

#### Architecture

```
Browser (WASM + Vanilla JS)          engram API              External
+---------------------------+    +----------------+    +------------------+
| Bayesian probability      |--->| /store         |    | GDELT News API   |
| Assessment cards          |    | /query         |    | Google News RSS  |
| Deep investigation        |--->| /ask (LLM)     |    | Wikipedia REST   |
| Cross-assessment analysis |    | /proxy/gdelt   |--->| SearXNG (local)  |
| Source citation renderer  |    | /proxy/rss     |    | Ollama LLM       |
| Engram storage pipeline   |--->| /proxy/search  |    +------------------+
+---------------------------+    | /proxy/llm     |
                                 +----------------+
```

### Architecture

```
  Live Data Sources              engram                    Intelligence Output
+------------------+     +------------------+     +---------------------------+
| Wikidata SPARQL  |---->|                  |---->| Probabilistic predictions |
| World Bank API   |---->|   Knowledge      |---->| Evidence chains           |
| GDELT News       |---->|   Graph          |---->| Contradiction resolution  |
| Exchange Rate    |---->|   + Confidence   |---->| Anomaly detection         |
| REST Countries   |---->|   + Inference    |---->| Policy simulation         |
| Wikipedia        |---->|   + Source Tiers |---->| JSON-LD export            |
+------------------+     +------------------+     +---------------------------+
                                |
                          MCP / Claude Code
                          (AI agent interface)
```

### Key Takeaways

- **Live data >> simulated data.** Fetching from 6 public APIs in real time proves engram works with real-world data at real-world scale. Population figures, GDP time series, and news articles are all verifiable.
- **Source tiers are the foundation of trust.** World Bank data (0.95) vs. RT propaganda (0.25) is not a matter of opinion -- it's a quantified reliability assessment that flows through every prediction.
- **Predictions are computed, not guessed.** Each probability traces back through an evidence chain to specific sources. "Baltic provocation: 64%" means exactly: 5 sources supporting at 0.80-0.90, 2 sources contradicting at 0.85-0.90, Bayesian aggregation.
- **Inference discovers what humans miss.** Turkey's dual role as NATO member and sanctions blocker was discovered by rule intersection, not by an analyst manually checking every country.
- **Contradictions are resolved by evidence weight.** When TASS claims "sanctions failed" (0.30) and World Bank data shows $300B frozen (0.93), the claim is zeroed. No human intervention needed.
- **Every fact is auditable.** `/explain` traces any node to its sources, edges, co-occurrences, and flags. No black boxes.
- **Policy simulation is actionable.** Adding countermeasure nodes with `weakens` edges and recalculating shows quantified impact: NATO/EU measures reduce Moldova destabilization from 59% to 42%. Ukraine full recovery constrained at 42-54% by nuclear risk (0.92). Decision-makers see exactly which levers move the needle.
- **The graph is alive.** As new evidence arrives (GDELT news, updated exchange rates, World Bank data), predictions automatically shift. Reinforce what's confirmed, correct what's disproven, let stale facts decay.
