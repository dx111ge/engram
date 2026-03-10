# Intel Dashboard Demo Script

## Voiceover Script & Timing

Each scene has: what's shown on screen, suggested narration, and duration.

---

### Scene 1: Opening (5s)
**Screen:** Dashboard loads, empty state, "Select a target country"
**Narration:** "Engram Intel is a browser-based geopolitical intelligence platform powered by a local knowledge graph, live news feeds, and AI analysis."

### Scene 2: Select Country (3s)
**Screen:** Click Russia dropdown, assessments appear
**Narration:** "Select a target country. The dashboard loads country data from Wikidata, World Bank, and Wikipedia automatically."

### Scene 3: Country Profile (8s)
**Screen:** Expand Borders, Organizations, Leaders, Economic Indicators
**Narration:** "Real-time country profile: bordering nations, international organization memberships, current leaders, GDP, inflation, military spending -- all from live public APIs."

### Scene 4: Create Assessment 1 (5s)
**Screen:** Type "Korea-style armistice with DMZ along current frontlines", select Military, 1 Year, confidence 0.55
**Narration:** "Create intelligence assessments. Each one is a hypothesis with a probability. Here: a Korea-style frozen armistice."

### Scene 5: Create Assessment 2 (5s)
**Screen:** Type "Frozen conflict: frontlines become de facto borders", select Conflict, 1 Year, confidence 0.50
**Narration:** "A second scenario: the frontlines simply freeze into de facto borders without formal agreement."

### Scene 6: Create Assessment 3 (5s)
**Screen:** Type "Ukraine recovers all territory including Crimea", select Military, 1 Year, confidence 0.15
**Narration:** "And the most ambitious scenario: full Ukrainian territorial recovery including Crimea. Starting at just 15%."

### Scene 7: Three Assessments Visible (3s)
**Screen:** All three cards visible with probability bars
**Narration:** "Three competing scenarios, each with an initial analyst confidence. Now let's feed them real data."

### Scene 8: Ingest News (show progress) (15s fast-forward)
**Screen:** Click "Ingest Now", show GDELT spinning, then Google News, then Web Search status
**Narration:** "Ingest pulls from three sources: GDELT global news database, Google News RSS, and SearXNG web search -- all filtered to the last 30 days. Each article is classified by topic and linked to relevant assessments."

### Scene 9: After Ingest (5s)
**Screen:** Show updated probabilities, evidence counts, news feed
**Narration:** "After ingesting 250+ articles, the probabilities shift based on real evidence. The WASM engine recalculates Bayesian probabilities in real-time. Every shift is traceable."

### Scene 10: News Feed (5s)
**Screen:** Scroll through Latest Ingested News showing article titles, domains, topic tags
**Narration:** "Every ingested article is classified by topic -- sanctions, conflict, negotiations, military -- and tagged with a source reliability tier."

### Scene 11: Engram Storage (3s)
**Screen:** Point to "engram (485 nodes)" in top right
**Narration:** "All findings are stored in Engram's knowledge graph. The more you research, the smarter the system gets."

### Scene 12: Single Assessment Ask (20s fast-forward)
**Screen:** Click on an assessment card, type a question, show 6-phase pipeline progress
**Narration:** "Deep investigation on any assessment. The system runs a 6-phase research pipeline: entity extraction, GDELT search, Google News, Wikipedia, web search, and knowledge graph traversal. Hundreds of facts compiled into a dossier, then analyzed by a local LLM."

### Scene 13: LLM Response with Sources (5s)
**Screen:** Show the analysis text with [1] [2] citation numbers, then the clickable Sources Cited section
**Narration:** "The AI response cites specific sources. Every claim links back to an article you can verify. No black boxes."

### Scene 14: Cross-Assessment Analysis (20s fast-forward)
**Screen:** Type "How does losing Starlink access impact all assessments?", click Analyze All, show pipeline, then impact cards
**Narration:** "Cross-assessment analysis. Ask a single question and see how it impacts every active scenario. The system runs the full research pipeline, then the LLM analyzes each assessment independently."

### Scene 15: Impact Cards (5s)
**Screen:** Show the three impact cards with directional arrows (some up, some down)
**Narration:** "Impact cards show probability shifts per assessment. Losing Starlink hurts Russia's military, so armistice becomes less likely, but frozen conflict becomes more likely. Each impact is backed by evidence from the research dossier."

### Scene 16: Closing (5s)
**Screen:** Full dashboard view
**Narration:** "Engram Intel: live data, Bayesian reasoning, AI analysis, full source traceability. All running locally -- no cloud, no API keys for core features. Your intelligence, your data, your machine."

---

**Total runtime:** ~2 minutes (with fast-forwards during pipeline waits)
