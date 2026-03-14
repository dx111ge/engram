# Wizard / UI Fixes TODO (2026-03-14)

Issues found during frontend walkthrough after GLiNER2 migration.

## Issues

1. ~~**Both GLiNER2 model chips highlighted**~~ -- FIXED. Used distinct IDs (`gliner2-fp16` / `gliner2-fp32`).

2. **~~Analyze shows no relations~~** -- NOT A BUG. Seed text is analytical, not factual. Model correctly returns 0 relations.

3. ~~**Wizard doesn't auto-close after seed ingest**~~ -- FIXED. Auto-advances to summary step after successful seed.

4. **Facts counter not updating live** -- By design. Dashboard fetches stats on mount. The dashboard's own "Seed KB" button already triggers refresh. Only the wizard's seed doesn't update it (different page context).

5. **Relations not shown after seed ingest** -- NOT A BUG. Seed text is descriptive, produces 0 relations. KB enrichment via Wikidata SPARQL runs during ingest but requires entity IDs to be resolved first (second ingest pass).

6. ~~**Wikidata SPARQL source not showing in Sources tab**~~ -- FIXED. Dashboard now fetches `/config/kb` and shows KB endpoints alongside ingest sources.

7. **Analyze slower via frontend (~15s) than curl (~5s)** -- Likely cold start: first GLiNER2 call loads the model (~5-8s). Subsequent calls are fast (~1s). Not a bug, but could show a "Loading model..." indicator on first use.

## Remaining (nice-to-have)

- First-use model loading indicator (show "Loading NER model..." on cold start)
- Dashboard stats auto-refresh via SSE events (currently requires page reload after external ingest)
