# Engram Open Issues & Roadmap

Last updated: 2026-04-10

## v1.1.2 Bug Fix Round (RELEASED 2026-04-10)

1. [x] **Chat: search tool LLM summary** -- added own match arm with `llm_analysis()`
2. [x] **Chat: topic_map tool LLM summary** -- added own match arm with `llm_analysis()`
3. [x] **Insights: conflicts pagination** -- proper page controls replacing `.take(20)`
4. [x] **Mesh endpoints 200 when disabled** -- graceful empty response instead of 503
5. [x] **Ingest page 422 fixed** -- sends `{items:[{text}]}` instead of `{text}`
6. [x] **Onboarding wizard: Serper.dev** -- card, API key field, hint
7. [x] **SearxNG setup guide** -- `docs/searxng-setup.md`

## Fixed in v1.1.0/v1.1.1 (closed)

- [x] LLM output budget caps (b568dbf)
- [x] Data link removal from gap-closing (b568dbf)
- [x] doc_date editable (ebb4e54)
- [x] Temporal facts (ebb4e54)
- [x] Contradictions & Conflicts (5d52f05)
- [x] Intelligence Gaps UX overhaul (b568dbf)
- [x] System prompt wire-up (b568dbf)
- [x] Domain taxonomy (b568dbf)

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
