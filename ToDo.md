# Engram Open Issues & Roadmap

Last updated: 2026-04-09

## High Priority

- [x] **GLiNER2 ONNX runs on CPU** -- FIXED: conditional execution providers (DirectML/CUDA/CoreML per platform) with CPU fallback. Features: `directml`, `cuda`, `coreml`. Build with `--features directml` on Windows to enable GPU.
- [x] **Multi-language search not wired into debate** -- FIXED: language detection moved before briefing, `topic_languages` passed to `build_starter_plate`, `gather_facts_for_question` retries with non-English languages, Wikidata/SPARQL queries use topic language.
- [x] **Evidence board always shows 0** -- FIXED: graph traversal results now added to `all_evidence` (were only logged to `research_summary` text). Dead duplicate branch removed.

## Moderate Priority

### Debate
- [ ] **Inject question flow untested** -- fully implemented (controls.rs + API endpoint), needs manual verification with active debate.
- [x] **Doc nodes missing metadata** -- FIXED: call-site guard skips doc node creation when URL is empty.
- [x] **No language property on Document nodes** -- FIXED: `create_pending_document_node` already accepts and sets `language` parameter.

### Sources / System UX
- [x] **Sources page disconnect** -- FIXED: `/sources` route removed, `SourcesSection` embedded in System page.
- [x] **Wizard should open on System page** -- FIXED: wizard opens as inline modal in System page.
- [x] **Unify Sources and System pages** -- FIXED: sources merged into System page under "Ingestion Sources".

### War Room
- [x] **Help content** -- FIXED: help button (fa-circle-question) with modal explaining agents, voting, inject, continue, synthesize, evidence.

### NER / Pipeline
- [x] **NER category learning** -- FIXED: three-tier label system (core + user-defined + auto-discovered from graph). GLiNER2 labels dynamically resolved at pipeline construction. Self-improving loop: ingest -> new node types -> labels expand -> better NER. API: GET/POST `/config/entity-labels`. UI: Entity Categories section in System/NER config.

## Low Priority

- [ ] **Data link wrong MIME** -- debate stores CSV/JSON with parent page MIME (`text/html`) instead of data file MIME. Low impact, gap closure still works.
- [ ] **Data link truncated at 8000 chars** -- debate caps data links for speed. Fine for gap closure, not for full processing.
- [x] **No language on documents** -- FIXED: pipeline propagates `topic_languages.first()` to Document node via `create_pending_document_node`.
- [ ] **doc_date always empty** -- no date extraction from document content.

## Performance

- [ ] **Gap-closing sequential** -- each gap takes 40-65s (4 gaps/round = ~3 min). Could parallelize with `tokio::join!` when LLM supports concurrent requests.
- [ ] **LLM JSON response truncation** -- article reading responses sometimes truncated. Increase `max_tokens` for article extraction prompt.

## Future Enhancements

### Document Processing
- [ ] **Machine-readable data in reprocess** -- scan HTML for data links (CSV/JSON/XML) during re-fetch, download full dataset as additional document.
- [ ] **Universal markdown intermediate** -- normalize all formats (PDF/HTML/XML/CSV/JSON/XLSX) to Markdown first, one parser for all. Designed, deferred (YAGNI with 2 formats).
- [ ] **XLSX parsing** -- `calamine` crate for Excel data extraction from statistical sources.

### Knowledge Management
- [ ] **Temporal knowledge** -- fact volatility classification, freshness checks, confidence decay, temporal succession (old -> new with supersedes edges).
- [ ] **Opportunistic data harvesting** -- store ALL quantitative facts from articles, not just gap-relevant. Knowledge compounds across sessions.
- [ ] **Coreference resolution** -- pronoun resolution (he/she/they -> canonical entity). Rule-based Rust first.

### Intelligence
- [ ] **Disinformation detection** -- contradiction detection, propagation tracking, source credibility erosion. Emergent from temporal + provenance.
- [ ] **OSINT platform** -- troll network detection, social media ingestion (X/Twitter, Telegram, Reddit), real-time event detection.

### Collaboration
- [ ] **Multi-user debate streaming** -- live debate rooms with host/participant/spectator roles, vote aggregation, replay system.
- [ ] **Meshed knowledge** -- federated peer-to-peer graph federation, domain specialist instances, cross-referenced facts.

## Recently Fixed (2026-04-08)

- [x] 42GB RAM explosion on PDF -- NER + RE now chunk large texts before ONNX. Stable at 6.7GB.
- [x] HTML tables stripped by Readability -- extract_html_tables() runs before Readability, pipe tables appended.
- [x] Table-unaware LLM prompt -- detects pipe tables + [Table] markers, adds column-header mapping instruction.
- [x] PDF tables as jumbled text -- space-aligned table detector converts to markdown pipe tables.
- [x] No `extracted_from` edges -- pipeline now creates Fact -> Document edges.
- [x] Stats showed doc_store blob count -- fixed to count Document graph nodes.
- [x] Re-fetch created duplicate docs -- old pending doc deleted after pipeline succeeds.
- [x] Vote triggered next round -- decoupled, only prefills inject text.
- [x] Vote text web-searched -- changed to moderator note.
- [x] SSE PascalCase status bug -- serde serialization instead of Debug format.
- [x] War Room center panel state machine -- replaced old Timeline view.
- [x] Delete button styling, pagination, custom confirm dialog, clickable URLs.
- [x] "FACTS" badge renamed to "NODES".
