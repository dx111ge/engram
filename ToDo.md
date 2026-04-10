# Engram Open Issues & Roadmap

Last updated: 2026-04-10

## v1.1.2 Bug Fix Round

1. [ ] **Chat: search tool missing LLM summary** -- `dispatch.rs:163`, `search` falls into generic path without `llm_analysis()`. Needs own match arm like `explain`.
2. [ ] **Chat: topic_map tool missing LLM summary** -- `dispatch.rs:695`, same issue. Add `llm_analysis()` after tool card result.
3. [ ] **Insights: pagination** -- Documents list and other long lists need pagination, page is too long.
4. [ ] **Mesh endpoints return 503 when disabled** -- `/mesh/audit` and `/mesh/identity` return 503 even when mesh is disabled. Should return 200 with empty/disabled status, or 404.
5. [ ] **Ingest page 422: missing field `items`** -- Ingest Pipeline page sends wrong JSON format to `/ingest` endpoint. Error: "missing field `items` at line 1 column 22".
6. [ ] **Onboarding wizard: Serper.dev missing** -- search engine configuration step in onboarding wizard doesn't include Serper.dev as an option.
7. [ ] **GitHub docs: SearxNG setup guide** -- document where to find SearxNG settings, how to configure delays, and how to enable all search engines.

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
