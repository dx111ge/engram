# Wizard / UI Fixes TODO (2026-03-14)

Issues found during frontend walkthrough after GLiNER2 migration.

## Issues

1. **Both GLiNER2 model chips highlighted** -- NER step shows FP16 and FP32 both selected because they share the same `repo_id`. Fix: use distinct IDs (e.g., `gliner2-fp16` / `gliner2-fp32`) or include variant in the model ID.

2. **~~Analyze shows no relations~~** -- NOT A BUG. The seed text is analytical/descriptive, not factual. The model correctly finds 0 relations because "Key actors include Russia, Ukraine, NATO" is a list, not a relationship statement. Tested in Python -- same result. Factual text like "Tim Cook is CEO of Apple" produces relations correctly.

3. **Wizard doesn't auto-close after seed ingest** -- After seed text is ingested in the final step, the wizard stays open. Should auto-navigate to the graph/dashboard page when complete.

4. **Facts counter not updating live** -- After ingest completes, the stored facts count doesn't update in the UI until a full browser refresh. Needs reactive signal update or SSE event to refresh the count.

5. **Relations not shown after seed ingest** -- After seeding, only entities (facts) are visible in the graph, no relations/edges. Either KB enrichment didn't find SPARQL relations, or the graph view doesn't render edges from the seed. Need to verify: (a) are relations stored in the brain file, (b) does the graph view query edges.

6. **Wikidata SPARQL source not showing in Sources tab** -- Wizard step configures Wikidata as a KB endpoint, but the Sources page doesn't list it as an active source. Either the wizard doesn't persist the KB endpoint config, or the Sources page doesn't read KB endpoints from config.

## Fixed

(none yet)
