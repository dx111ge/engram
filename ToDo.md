# Engram Open Issues & Roadmap

Last updated: 2026-04-09

## Current Release -- Bug Fix Round

1. [ ] **LLM output budget caps** -- raise `short_output_budget` cap from 2048 to 8192, `medium_output_budget` from 4096 to 16384. Dynamic scaling based on model context window.
2. [ ] **Data link removal from gap-closing** -- remove data link fetch from `fetch_article_content`. Fixes wrong MIME + 8000 char truncation. Data link processing belongs in reprocess (future).
3. [ ] **doc_date editable** -- make `doc_date` field editable in document detail UI. No auto-extraction, user sets manually.
4. [ ] **Temporal facts** -- three-layer: extend LLM prompts with valid_from/valid_to, add quick non-thinking LLM pass after NER/RE, user manual fallback. Wire `relate_with_temporal()` in pipeline.
5. [ ] **Contradictions & Conflicts** -- wire ConflictDetector into production pipeline, store as `conflicts_with` graph edges, user-configurable singular properties, API endpoint, UI display with resolution actions.
6. [ ] **Intelligence Gaps UX overhaul** -- filter internal types, human-readable labels, show suggested queries, domain-based asymmetric detection (remove type-based noise), background quality enrichment endpoint with LLM query generation + user edit, persist dismissed gaps.
7. [ ] **System prompt wire-up** -- add `llm_system_prompt` to EngineConfig (currently UI writes but backend ignores), use in all LLM calls, generate during onboarding.
8. [ ] **Domain taxonomy** -- user-defined domains in EngineConfig, LLM suggests domains from graph content, auto-classify entities during ingest/debate, UI in System page + onboarding wizard.

## Next Release

- [ ] **Gap-closing parallelism** -- Ollama serializes heavy requests on single GPU. Deferred until vLLM / multi-GPU / remote API.
- [ ] **Machine-readable data in reprocess** -- scan HTML for data links (CSV/JSON/XML) during re-fetch.
- [ ] **Universal markdown intermediate** -- normalize all formats to Markdown first. Designed, deferred (YAGNI with 2 formats).
- [ ] **XLSX parsing** -- `calamine` crate for Excel data extraction.
- [ ] **Temporal knowledge (full)** -- fact volatility, freshness checks, confidence decay beyond basic valid_from.
- [ ] **Coreference resolution** -- pronoun resolution. Park for external solution.
- [ ] **Disinformation detection** -- contradiction detection, propagation tracking, source credibility erosion.
- [ ] **OSINT platform** -- troll network detection, social media ingestion.
- [ ] **Multi-user debate streaming** -- live debate rooms, vote aggregation, replay.
- [ ] **Meshed knowledge** -- federated peer-to-peer graph federation.
