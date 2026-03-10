# Engram Intel Dashboard -- Roadmap

## Current State (2026-03-08)
Single-page geopolitical intelligence platform powered by engram + WASM.
- License-gated, locale-restricted
- 98 countries with Wikidata profiles, World Bank economic data
- Assessment builder with Bayesian probability engine (WASM)
- Auto Wikipedia context on assessment creation
- Multi-source news ingestion: GDELT (via engram proxy) + Google News RSS (via engram proxy)
- WASM classifier: 18 topic categories, source tier assessment, evidence linking
- Collapsible country profile panels, prediction cards with sparklines
- Per-country localStorage persistence + engram graph storage
- 5-minute auto-ingestion timer

## Architecture Bottlenecks

### 1. localStorage is the weak link
Assessments, predictions, and ingested news live in browser localStorage.
- 5-10MB hard limit per origin
- Lost if user clears browser data
- Not shared across devices/browsers
- No multi-user support

### 2. GDELT rate limiting
1 request per 5 seconds. 10 queries per country = ~1 minute per country.
Browser-bound ingestion blocks the UI during fetch cycles.

### 3. News classification is keyword-only
The WASM classifier uses static keyword matching. No semantic understanding.
"Iran summons ambassador" gets tagged as regime_change because "ambassador" isn't in the keyword list but the context is diplomatic.

### 4. Single-user, single-browser
No authentication beyond license key. No collaboration. No shared assessments.

---

## Phase 1: Server-Side Persistence

**Goal**: Move all data from localStorage into engram graph.

- [ ] `/predictions` REST endpoints: GET/POST/PUT/DELETE per country
- [ ] Store predictions as typed nodes: `Prediction:{id}` with properties (title, category, timeframe, probability, country)
- [ ] Store evidence as edges: `Prediction:{id} --supported_by--> Evidence:{id}`
- [ ] Store news as nodes: `News:{hash}` linked to predictions via edges
- [ ] Load predictions from engram on country switch (fall back to localStorage if offline)
- [ ] Sync localStorage <-> engram bidirectionally for offline support
- [ ] Add `/predictions/{country}` query endpoint with filtering

## Phase 2: Server-Side Ingestion

**Goal**: Move GDELT/RSS ingestion to the engram backend. Browser just displays results.

- [ ] Background ingestion worker in engram (Tokio task)
- [ ] `/ingest/start/{country}` and `/ingest/status/{country}` endpoints
- [ ] Configurable ingestion interval per country (default 5 min)
- [ ] Rate-limit-aware GDELT queue (respect 5s cooldown server-side)
- [ ] Server-side RSS fetching for all prediction-specific queries
- [ ] Server stores classified articles directly into the graph
- [ ] Dashboard polls `/ingest/status` and renders new articles
- [ ] Historical backfill: on first country select, fetch 90 days of GDELT data

## Phase 3: Smarter Classification

**Goal**: Move beyond keyword matching to semantic classification.

- [ ] Use engram's embedding engine (ONNX) for semantic similarity between article titles and topic definitions
- [ ] Hybrid: keyword match first (fast), then embedding similarity for unmatched articles
- [ ] Confidence scores based on semantic distance, not just source tier
- [ ] Learn from user corrections: if user moves an article from "supports" to "contradicts", adjust topic model
- [ ] Entity extraction: identify actors, locations, organizations mentioned in articles
- [ ] Sentiment analysis on article titles for direction (supports/weakens)

## Phase 4: Knowledge Graph Intelligence

**Goal**: Use engram's graph capabilities for cross-country, cross-topic analysis.

- [ ] Automatic relationship discovery: "Iran sanctions" articles link to "Russia trade" articles via shared entities
- [ ] Cross-country impact propagation: sanctions on Russia affect Iran's energy exports
- [ ] Temporal trend detection: "protests in Iran" frequency increasing over 30 days
- [ ] Contradiction detection: two articles with opposing claims flagged automatically
- [ ] Confidence decay: old evidence loses weight over time (already in engram-core learning engine)
- [ ] Graph visualization: show prediction network, evidence chains, country relationships
- [ ] "What if" scenarios: simulate removing/adding evidence, see probability cascades

## Phase 5: Multi-Source Intelligence

**Goal**: Expand beyond GDELT and Google News.

- [ ] Twitter/X firehose (via API or scraping proxy) for real-time signals
- [ ] Telegram channel monitoring (OSINT channels)
- [ ] Government press releases (state.gov, kremlin.ru, etc.)
- [ ] UN Security Council resolutions and voting records
- [ ] ACLED conflict event data (armed conflict tracker)
- [ ] SIPRI arms transfer database
- [ ] Satellite imagery change detection (Sentinel-2 for military buildup)
- [ ] Financial signals: currency movements, bond spreads, commodity prices
- [ ] Each source gets its own proxy endpoint and ingestion worker

## Phase 6: Multi-User & Collaboration

**Goal**: Turn this into a team tool.

- [ ] User accounts with role-based access (analyst, reviewer, admin)
- [ ] Shared assessments with attribution (who created, who modified)
- [ ] Assessment review workflow: draft -> review -> published
- [ ] Comments on predictions and evidence
- [ ] Audit trail: full history of probability changes with who/what caused them
- [ ] Per-team country assignments
- [ ] Real-time sync via WebSocket (multiple analysts on same country)
- [ ] Export: PDF intelligence briefs, JSON-LD for interop

## Phase 7: LLM-Assisted Analysis

**Goal**: Use LLMs as analyst assistants, not oracles.

- [ ] "Summarize evidence" button: LLM reads all evidence for a prediction and generates a 2-paragraph assessment
- [ ] "Challenge assessment" button: LLM plays devil's advocate, finds contradicting angles
- [ ] Auto-generate hypotheses: given a country and recent news, suggest assessments to create
- [ ] Article summarization: fetch full article text, generate 2-sentence summary for evidence descriptions
- [ ] Translation: non-English GDELT articles translated via LLM for analysis
- [ ] Structured analytic techniques: LLM-guided ACH (Analysis of Competing Hypotheses), Key Assumptions Check
- [ ] Important: LLM suggestions always marked as "LLM-generated", never auto-accepted. Human confirms.

## Phase 8: Alerting & Monitoring

**Goal**: Don't wait for analysts to check -- push critical changes.

- [ ] Probability threshold alerts: "Iran regime change crossed 70%, was 53% yesterday"
- [ ] Spike detection: sudden probability shift > 10% in 24 hours
- [ ] New high-confidence article alert for tracked predictions
- [ ] Daily digest email per analyst: changes across all tracked countries
- [ ] Webhook integration: push alerts to Slack, Teams, or custom endpoints
- [ ] Dashboard widget: "Critical Changes" panel showing biggest movers across all countries

## Phase 9: Migrate Dashboard from HTML/JS to Rust

**Goal**: Single binary. No Python HTTP server, no separate HTML files. Everything is Rust.

- [ ] Embed dashboard HTML/CSS/JS into the engram binary using `include_str!` or `rust-embed`
- [ ] Serve dashboard from engram's axum server at `/` (static file handler)
- [ ] Replace rss2json / browser-side fetch logic with server-side Rust ingestion workers
- [ ] Move WASM classification logic to server-side Rust (same code, no WASM compilation needed)
- [ ] Replace localStorage with engram graph API calls (Phase 1 prerequisite)
- [ ] Dashboard becomes a thin UI layer: all intelligence logic runs server-side in Rust
- [ ] Single `engram serve` command starts everything: API + dashboard + ingestion workers
- [ ] Optional: Leptos/Yew/Dioxus for a full Rust frontend (SPA compiled to WASM)
- [ ] Optional: Tauri desktop app wrapping the dashboard for offline analyst workstations
- [ ] Remove Python dependency entirely (currently used for `python3 -m http.server`)

## Phase 10: Production Hardening

**Goal**: Make it deployable and reliable. Zero external dependencies.

- [ ] Docker container with engram + dashboard in one image
- [ ] TLS termination (HTTPS)
- [ ] License server with proper key management (not FNV-1a hash)
- [ ] Rate limiting on API endpoints
- [ ] Backup/restore for .brain files
- [ ] Health monitoring and metrics (Prometheus endpoint)
- [ ] Horizontal scaling: multiple engram instances with mesh sync for high-availability
- [ ] Data retention policies: auto-archive news older than X days

---

## Quick Wins (Low effort, High impact)

1. **Server-side prediction storage** -- 1 day. Biggest reliability improvement.
2. **Server-side GDELT worker** -- 1 day. Removes browser dependency for ingestion.
3. **Probability threshold alerts** -- half day. Toast notifications when thresholds cross.
4. **Article date sorting** -- trivial. Sort ingested news by date, newest first.
5. **Prediction sorting** -- trivial. Sort by probability or last-changed.
6. **Fullscreen detail view** -- half day. Click prediction to get full-screen analysis view.
7. **Export to JSON** -- trivial. Download all predictions + evidence as JSON.

we should also introuce the "what needs to be done to reduce or increase the prediction for an outcome ... this is the most important part .... 
