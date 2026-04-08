# Engram Open Issues & Roadmap

Last updated: 2026-04-08

## High Priority

- [ ] **GLiNER2 ONNX runs on CPU** -- 6GB RAM baseline, should use RTX 5070 via DirectML/CUDA in `ort` crate. 10-50x speedup, tensors to VRAM instead of system RAM. Check `crates/engram-ingest/Cargo.toml` + `gliner2_backend.rs` session creation.
- [ ] **Multi-language search not wired into debate** -- briefing + agent research always use `language=en`. Languages detected correctly (`en,ru,uk`) but not passed to web search calls. Missing Russian/Ukrainian primary sources. Fix in `research.rs` briefing search + agent research.
- [ ] **Evidence board always shows 0** -- War Room evidence panel not populating from SSE `turn_complete` evidence data. Needs investigation.

## Moderate Priority

### Debate
- [ ] **Inject question flow untested** -- only vote tested with new center panel state machine. Inject text + send button needs verification.
- [ ] **Doc nodes missing metadata** -- some web fetches create docs with empty title/URL. Need empty-string guard in `create_pending_document_node`.
- [ ] **No language property on Document nodes** -- `create_pending_document_node` doesn't set language even though search language is known.

### Sources / System UX
- [ ] **Sources page disconnect** -- Sources page (`/sources`) is empty, disconnected from System page's Sources & Integrations section.
- [ ] **Wizard should open on System page** -- add-source wizard should open in-place, not navigate to empty Sources page.
- [ ] **Unify Sources and System pages** -- merge Sources content into System page.

### War Room
- [ ] **Help content** -- question mark button needs meaningful content about War Room, how to vote/boost/inject.

### NER / Pipeline
- [ ] **NER category learning** -- verify gazetteer learns from confirmed graph entities. Does feedback loop (graph -> gazetteer -> NER) work? Test with overlapping documents.

## Low Priority

- [ ] **Data link wrong MIME** -- debate stores CSV/JSON with parent page MIME (`text/html`) instead of data file MIME. Low impact, gap closure still works.
- [ ] **Data link truncated at 8000 chars** -- debate caps data links for speed. Fine for gap closure, not for full processing.
- [ ] **No language on documents** -- all docs show `lang=-` in UI. Pipeline doesn't propagate detected language to Document node.
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
