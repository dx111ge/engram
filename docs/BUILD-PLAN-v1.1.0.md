# engram v1.1.0 Build Plan

**Created:** 2026-03-10
**Design doc:** `docs/DESIGN-v1.1.0.md`
**Starting point:** v1.0.0 compiles clean, 355 tests pass

---

## Status Legend

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked
- `[-]` Skipped / deferred

---

## Phase 7: Foundation (event system + bulk endpoint)

**Goal:** Event infrastructure that all new crates depend on.
**Crate:** `engram-core` (enhancement)

| # | Task | Effort | Deps | Status |
|---|------|--------|------|--------|
| 7.1 | Add `GraphEvent` enum and bounded channel to `engram-core` | Small | -- | `[x]` |
| 7.2 | Emit events from all graph mutation methods (store, relate, update_confidence, etc.) | Small | 7.1 | `[x]` |
| 7.3 | Upgrade `POST /batch` with NDJSON streaming | Medium | -- | `[x]` |
| 7.4 | Add chunked write locking to batch | Small | 7.3 | `[x]` |
| 7.5 | Add upsert mode to batch | Small | 7.4 | `[x]` |

**Phase 7 done:** `[x]`

---

## Phase 8: Ingest Pipeline

**Goal:** Full ELT pipeline with NER, entity resolution, source management, search ledger.
**Crate:** `engram-ingest` (new)

| # | Task | Effort | Deps | Status |
|---|------|--------|------|--------|
| 8.1 | Create `engram-ingest` crate skeleton (traits, pipeline executor) | Medium | -- | `[x]` |
| 8.2 | Implement pipeline stages: Parse, Transform, Load | Medium | 8.1 | `[x]` |
| 8.3 | Language detection integration (whatlang) | Small | 8.1 | `[x]` |
| 8.4 | Graph gazetteer (dynamic, self-updating) | Medium | 8.1 | `[x]` |
| 8.5 | Rule-based NER (regex patterns, per-language rule files) | Medium | 8.3 | `[x]` |
| 8.6 | NER chain (cascade/merge strategies) | Medium | 8.4, 8.5 | `[x]` |
| 8.7 | Anno backend (feature-gated, `anno_backend.rs`, GLiNER2 + coreference) | Medium | 8.6 | `[ ]` |
| 8.8 | SpaCy HTTP sidecar integration | Small | 8.6 | `[ ]` |
| 8.9 | LLM fallback NER (with restrictions) | Small | 8.6 | `[ ]` |
| 8.10 | Entity resolution (conservative, progressive 4-step) | Large | 8.6 | `[ ]` |
| 8.11 | Deduplication (content hash + semantic) | Small | 8.10 | `[ ]` |
| 8.12 | Conflict detection and resolution | Medium | 8.11 | `[ ]` |
| 8.13 | Confidence calculation (learned trust * extraction confidence, author > source > baseline) | Small | 8.12 | `[ ]` |
| 8.14 | Multi-threaded pipeline executor (rayon + tokio) | Medium | 8.2 | `[ ]` |
| 8.15 | Pipeline shortcuts (`?skip=ner,resolve` query params) | Small | 8.14 | `[ ]` |
| 8.16 | Wire into API: `POST /ingest`, `POST /ingest/file`, `POST /ingest/configure` | Small | 8.14 | `[ ]` |
| 8.17 | Source trait with capabilities + usage endpoint | Medium | 8.1 | `[ ]` |
| 8.18 | File source (notify crate, watch mode, poll fallback, format auto-detect) | Medium | 8.17 | `[ ]` |
| 8.19 | Search ledger (`.brain.ledger`, temporal cursors, content hash dedup) | Medium | 8.17 | `[ ]` |
| 8.20 | Query subsumption (substring check, configurable window) | Small | 8.19 | `[ ]` |
| 8.21 | Adaptive frequency scheduler (min/max bounds, yield-based adjustment) | Medium | 8.19 | `[ ]` |
| 8.22 | Source usage endpoint integration (pre-fetch budget check, soft/hard limits) | Medium | 8.17 | `[ ]` |
| 8.23 | Mesh fast path (skip NER, resolve locally, peer trust multiplier) | Small | 8.10, 8.15 | `[ ]` |
| 8.24 | Learned patterns from graph co-occurrence | Medium | 8.6 | `[ ]` |
| 8.25 | NER correction feedback loop | Small | 8.24 | `[ ]` |
| 8.26 | Source health monitoring (success/failure rate, latency, auth status) | Small | 8.17 | `[ ]` |
| 8.27 | Wire source APIs: `GET /sources`, `GET /sources/{name}/usage`, `GET /sources/{name}/ledger` | Small | 8.22, 8.19 | `[ ]` |
| 8.28 | Learned trust: create Source/Author nodes on first encounter, `from_source`/`authored_by` edges | Medium | 8.13, 8.17 | `[ ]` |
| 8.29 | Learned trust: auto-adjust via corroboration/correction propagation, per-source author scoping | Medium | 8.28 | `[ ]` |

**Phase 8 done:** `[ ]`

---

## Phase 9: Action Engine

**Goal:** Event-driven rule engine that reacts to graph changes.
**Crate:** `engram-action` (new)

| # | Task | Effort | Deps | Status |
|---|------|--------|------|--------|
| 9.1 | Create `engram-action` crate skeleton | Small | 7.1 | `[ ]` |
| 9.2 | Event subscriber (consumes GraphEvent channel) | Small | 9.1 | `[ ]` |
| 9.3 | Rule parser (TOML rule definitions) | Medium | 9.2 | `[ ]` |
| 9.4 | Condition evaluator (pattern matching against events) | Medium | 9.3 | `[ ]` |
| 9.5 | Internal effects (confidence cascade, edge creation, tier change) | Medium | 9.4 | `[ ]` |
| 9.6 | External effects (webhook, API call, message notification) | Medium | 9.4 | `[ ]` |
| 9.7 | `CreateIngestJob` effect (dynamic jobs, `QueryTemplate`, `ReconcileStrategy`) | Medium | 9.4, 8.14 | `[ ]` |
| 9.8 | Safety constraints (cooldown, chain depth, effect budget) | Small | 9.5-9.7 | `[ ]` |
| 9.9 | Timer-based triggers (scheduled rules) | Small | 9.4 | `[ ]` |
| 9.10 | Wire into API: rule management endpoints | Small | 9.8 | `[ ]` |
| 9.11 | Dry run mode | Small | 9.4 | `[ ]` |

**Phase 9 done:** `[ ]`

---

## Phase 10: Black Area Detection & Enrichment

**Goal:** Knowledge gap analysis, mesh federation, 3-tier enrichment.
**Crate:** `engram-reason` (new)

| # | Task | Effort | Deps | Status |
|---|------|--------|------|--------|
| 10.1 | Create `engram-reason` crate skeleton | Small | -- | `[ ]` |
| 10.2 | Frontier node detection | Small | 10.1 | `[ ]` |
| 10.3 | Structural hole detection | Medium | 10.1 | `[ ]` |
| 10.4 | Asymmetric cluster analysis | Medium | 10.1 | `[ ]` |
| 10.5 | Temporal gap detection | Small | 10.1 | `[ ]` |
| 10.6 | Confidence desert detection | Small | 10.1 | `[ ]` |
| 10.6b | Coordinated cluster detection (dense internal, sparse external, low author trust, temporal sync) | Medium | 10.4 | `[ ]` |
| 10.7 | Severity scoring and ranking | Small | 10.2-10.6 | `[ ]` |
| 10.8 | Suggested query generation (mechanical, from graph topology) | Medium | 10.7 | `[ ]` |
| 10.9 | LLM-suggested queries (optional, via existing LLM endpoint) | Small | 10.8 | `[ ]` |
| 10.10 | Mesh knowledge profile auto-derivation (cluster -> DomainCoverage) | Medium | 10.4 | `[ ]` |
| 10.11 | Mesh profile gossip broadcast + ProfileQuery message type | Medium | 10.10 | `[ ]` |
| 10.12 | Mesh federated query protocol (FederatedQuery/FederatedResult) | Medium | 10.11 | `[ ]` |
| 10.13 | Mesh discovery API (`/mesh/profiles`, `/mesh/discover`) | Small | 10.12 | `[ ]` |
| 10.14 | 3-tier enrichment dispatcher (mesh > free external > paid external) | Medium | 10.12, 8.17 | `[ ]` |
| 10.15 | Query-triggered enrichment (eager + await modes) | Medium | 10.14 | `[ ]` |
| 10.16 | Mesh-level black area detection (uncovered areas across all peers) | Small | 10.10, 10.7 | `[ ]` |
| 10.17 | Wire into API: `/reason/gaps`, `/query?enrich=`, `/mesh/query` | Small | 10.15, 10.13 | `[ ]` |

**Phase 10 done:** `[ ]`

---

## Phase 11: Streaming & Protocol Extensions

**Goal:** SSE, webhooks, WebSockets, MCP/A2A/gRPC extensions.
**Crates:** `engram-core`, `engram-api`, `engram-a2a` (enhancements)

| # | Task | Effort | Deps | Status |
|---|------|--------|------|--------|
| 11.1 | `EventBus` (tokio broadcast channel) in `engram-core` | Small | 7.1 | `[ ]` |
| 11.2 | SSE event subscription endpoint (`GET /events/stream`) | Medium | 11.1 | `[ ]` |
| 11.3 | Webhook receiver endpoint (`POST /ingest/webhook/{id}`) | Medium | 8.13 | `[ ]` |
| 11.4 | WebSocket ingest endpoint (`WS /ingest/ws/{id}`) | Medium | 8.13 | `[ ]` |
| 11.5 | SSE response streaming for enrichment (`?enrich=await`) | Medium | 10.11 | `[ ]` |
| 11.6 | SSE ingest progress streaming (`GET /batch/jobs/{id}/stream`) | Small | 7.3 | `[ ]` |
| 11.7 | MCP tools: `engram_gaps`, `engram_enrich`, `engram_sources` | Medium | 10.17, 8.27 | `[ ]` |
| 11.8 | MCP tools: `engram_mesh_discover`, `engram_mesh_query` | Small | 10.13 | `[ ]` |
| 11.9 | MCP restricted tools: `engram_ingest`, `engram_create_rule` (opt-in) | Small | 8.16, 9.10 | `[ ]` |
| 11.10 | A2A skills: `ingest_text`, `enrich_query`, `analyze_gaps` | Medium | 8.16, 10.17 | `[ ]` |
| 11.11 | A2A skills: `federated_search`, `suggest_investigations` | Small | 10.13, 10.9 | `[ ]` |
| 11.12 | A2A streaming task support for long-running operations | Medium | 11.5 | `[ ]` |
| 11.13 | gRPC proto definitions (`proto/engram_v110.proto`) | Medium | 8.27, 10.17 | `[ ]` |
| 11.14 | gRPC server-streaming RPCs (ingest progress, enrichment, events) | Medium | 11.13, 11.2 | `[ ]` |
| 11.15 | gRPC client-streaming RPC (bulk ingest) | Small | 11.13, 8.16 | `[ ]` |

**Phase 11 done:** `[ ]`

---

## Phase 12: Frontend (Leptos WASM)

**Goal:** Full frontend rewrite in Leptos. 11 pages, vis.js interop, SSE live updates.
**Crate:** `engram-ui` (new, built via trunk, excluded from workspace)

| # | Task | Effort | Deps | Status |
|---|------|--------|------|--------|
| 12.1 | Create `engram-ui` Leptos crate (Trunk.toml, main.rs, app.rs, router) | Medium | -- | `[ ]` |
| 12.2 | Shared components: nav, toast, modal, settings, table, stat_card | Medium | 12.1 | `[ ]` |
| 12.3 | API client module (gloo-net, context provider, error handling) | Small | 12.1 | `[ ]` |
| 12.4 | SSE listener component (EventSource -> Leptos signals) | Medium | 12.1 | `[ ]` |
| 12.5 | vis.js interop (wasm-bindgen extern, GraphCanvas component) | Medium | 12.1 | `[ ]` |
| 12.6 | Page: Dashboard (stats, health, system overview) | Medium | 12.2, 12.3 | `[ ]` |
| 12.7 | Page: Graph (vis.js visualization, node inspector, filtering) | Large | 12.5, 12.3 | `[ ]` |
| 12.8 | Page: Search (BM25 + semantic, result list, property filters) | Medium | 12.3 | `[ ]` |
| 12.9 | Page: Natural Language (/tell, /ask, conversation view) | Medium | 12.3 | `[ ]` |
| 12.10 | Page: Import (JSON-LD upload/download, preview) | Small | 12.3 | `[ ]` |
| 12.11 | Page: Learning (confidence scores, reinforcement, decay timeline) | Medium | 12.3 | `[ ]` |
| 12.12 | Page: Ingest (pipeline config, live progress via SSE) | Large | 12.2, 12.4, 8.16 | `[ ]` |
| 12.13 | Page: Sources (health dashboard, usage, ledger view) | Medium | 12.3, 8.27 | `[ ]` |
| 12.14 | Page: Actions (rule editor, dry run, event log) | Medium | 12.3, 9.10 | `[ ]` |
| 12.15 | Page: Gaps (black area map, severity table, LLM suggestions + warning) | Large | 12.5, 12.3, 10.17 | `[ ]` |
| 12.16 | Page: Mesh (peer topology, profiles, federated query, trust controls) | Large | 12.5, 12.3, 10.13 | `[ ]` |
| 12.17 | CSS migration (adapt existing style.css for Leptos class bindings) | Medium | 12.6-12.16 | `[ ]` |
| 12.18 | Trunk release build integration (output to frontend/dist, served by engram) | Small | 12.17 | `[ ]` |

**Phase 12 done:** `[ ]`

---

## Summary

| Phase | Tasks | Small | Medium | Large | Status |
|-------|-------|-------|--------|-------|--------|
| 7 Foundation | 5 | 3 | 1 | 0 | `[ ]` |
| 8 Ingest | 27 | 10 | 14 | 1 | `[ ]` |
| 9 Action | 11 | 5 | 4 | 0 | `[ ]` |
| 10 Reason | 17 | 5 | 9 | 0 | `[ ]` |
| 11 Streaming | 15 | 4 | 8 | 0 | `[ ]` |
| 12 Frontend | 18 | 2 | 10 | 4 | `[ ]` |
| **Total** | **93** | **29** | **46** | **5** | |

## Parallel Work Streams

Based on dependency analysis, these can run in parallel after Phase 7:

```
Phase 7 (Foundation)
    |
    +---> Phase 8 (Ingest)  ----+
    |                           |
    +---> Phase 9 (Action)  ----+--> Phase 11 (Streaming)
    |                           |
    +---> Phase 10 (Reason) ----+
    |
    +---> Phase 12.1-12.11 (Frontend: existing pages, no backend deps)
              |
              +--> Phase 12.12-12.16 (Frontend: new pages, need backend APIs)
                        |
                        +--> Phase 12.17-12.18 (CSS + build)
```

**Critical path:** 7 -> 8 -> 11 -> 12 (ingest pipeline is the longest chain)

## Build Verification Gates

After each phase, verify:

1. **Phase 7:** `cargo test` passes, events emitted for every mutation, batch NDJSON works
2. **Phase 8:** Full pipeline test with sample data, NER extracts correctly, ledger dedup works
3. **Phase 9:** Rules fire on events, safety limits enforced, dry run matches live
4. **Phase 10:** Gaps detected in test graph, mesh profiles generated, federated query returns results
5. **Phase 11:** SSE delivers events to browser, webhook ingests data, gRPC streams work
6. **Phase 12:** All 11 pages render, vis.js graph works, SSE updates live, `trunk build --release` produces working WASM

## Notes

- All new crates are feature-gated. Base binary (`cargo build`) stays unchanged from v1.0.0.
- `cargo build --features full` enables everything.
- `engram-ui` is excluded from workspace, built separately via `trunk`.
- `engram-intel` remains separate (WASM geopolitical engine, unrelated to frontend).
- Zero warnings policy continues from v1.0.0.
