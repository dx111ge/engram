# Explore Page v2: Smart Search + Detail Modal + Investigation Flow

> **Status: COMPLETED** -- All features implemented. See `design-explore-v3-performance.md` for the subsequent performance pass (LOD, smart scaling, edge bundling, find path redesign, Edit tab, color legend).

## Context
Testing revealed fundamental UX problems: 3 confusing action buttons (Research/Enrich/Ingest), useless search modes, Detail navigating away and losing graph state, sidebar too cramped for node actions. This redesign unifies everything.

## Decision Summary (2026-03-16)
1. Research/Enrich/Ingest merged into one "Investigate" stepped flow inside Detail view
2. Search mode dropdown removed. One smart search box with client-side fuzzy matching
3. Detail view is a modal overlay (graph preserved underneath)
4. Single click on node opens Detail modal. All node actions live there.

---

## 1. Smart Search (replaces 3-mode dropdown)

### Single search box behavior:
1. User types query, hits Enter/Go
2. Client-side normalization: try original, then variations (strip hyphens/spaces/dots)
   - "F16" --> try "F16", "F-16", "F 16"
   - "putin" --> "Putin" (case handled by backend)
3. Send to `/search` endpoint for exact/fuzzy match
4. If results found with good score --> take top result, traverse from it (show 3D graph)
5. If no results --> show results as clickable list panel (NOT 3D nodes), each with label + type + confidence
6. Click any list result --> traverse from that node

### UI:
- Remove `<select>` search mode dropdown entirely
- Single `<input>` + "Go" button (same as now minus the dropdown)
- When showing list results (no graph match): render in sidebar as cards, not in 3D canvas

### Files: `graph.rs` (remove search_mode signal, simplify do_search action)

---

## 2. Detail Modal (replaces Detail page navigation + sidebar actions)

### Opens when:
- Single click any node in 3D graph
- Click "Open" button in sidebar summary
- Right-click > "Details" in context menu
- Right-click > "Investigate" in context menu (auto-starts investigation tab)

### Layout:
Modal overlay using `wizard-modal` pattern (same as system modals), wider (900px).
Graph stays underneath, visible but dimmed. Close button top-right returns to graph.

```
+-------------------------------------------------------+
|  [icon] Entity Name                           [X]     |
|  [TYPE badge]   Confidence: 95%                        |
+-------------------------------------------------------+
|  [Info]  [Connections]  [Investigate]                  |
+-------------------------------------------------------+
|                                                        |
|  (tab content)                                         |
|                                                        |
+-------------------------------------------------------+
```

### Tab: Info
- Properties table (all key-value pairs)
- KB links (Wikidata URL, etc.)
- Provenance (ingest_source, timestamps)

### Tab: Connections
- Outgoing edges list: relationship --> target (clickable, opens that node's Detail)
- Incoming edges list: source --> relationship (clickable)
- Edge count summaries

### Tab: Investigate (the merged Research/Enrich/Ingest flow)
Stepped wizard within the tab:

**Step 1: Gather**
- Auto-runs web search (using canonical_name) + KB lookup (Wikidata property expansion)
- Shows web results as cards (title + snippet)
- Shows KB results as structured properties/relations
- "Skip" button to go to step 2 with just KB results

**Step 2: Review**
- List of discovered entities + relations with checkboxes (like seed wizard entity review)
- Each shows: entity label, type, confidence, source (web/KB)
- User can select/deselect, edit types, skip unwanted items
- "Add custom" row to manually add an entity/relation

**Step 3: Commit**
- Summary: "X entities, Y relations will be added"
- "Commit to Graph" button --> POST /ingest with selected items
- Results shown: facts stored, relations created
- Graph refreshes to show new nodes/edges

### Files:
- New: `crates/engram-ui/src/components/detail_modal.rs`
- Modify: `graph.rs` (replace sidebar detail panel + action buttons with modal trigger)
- Modify: `index.html` (no JS changes needed, modal is pure Rust/HTML)

---

## 3. Sidebar Simplification

### Before (current):
- Controls card (depth, strength, direction, layout, edge labels, temporal)
- Filters card (node types, relationships) -- collapsible
- Detail card (entity info + Research/Enrich/Ingest/Start buttons + research results)

### After:
- Controls card (same)
- Filters card (same, collapsible)
- Node Preview card (when node selected):
  - Entity name + type badge
  - Confidence + edge counts (compact, 2 lines)
  - "Open" button (opens Detail modal)
  - "Set as Start" button
  - NO Research/Enrich/Ingest buttons (moved to Detail modal)

---

## 4. Context Menu Update

### Before:
Expand | Details | Enrich from KB | Set as Start | Find Path To... | Hide Type

### After:
Expand | Open Detail | Set as Start | Find Path To... | Hide Type

"Enrich from KB" removed (lives in Detail modal's Investigate tab).
"Details" renamed to "Open Detail" (clearer).

---

## 5. Client-Side Search Normalization

```javascript
function searchVariations(query) {
  var variations = [query];
  // Try with/without hyphens
  if (query.includes('-')) variations.push(query.replace(/-/g, ''));
  if (query.includes('-')) variations.push(query.replace(/-/g, ' '));
  // Try adding hyphens between letter-number boundaries
  var withHyphen = query.replace(/([a-zA-Z])(\d)/g, '$1-$2');
  if (withHyphen !== query) variations.push(withHyphen);
  // Deduplicate
  return [...new Set(variations)];
}
```

---

## Implementation Order
1. Fix straightforward system page issues (#20-30) -- no design change
2. Remove search mode dropdown, implement smart search
3. Build Detail modal component (Info + Connections tabs)
4. Build Investigation tab (stepped gather/review/commit)
5. Simplify sidebar, update context menu
6. Client-side search normalization

## Verification
1. `cargo build --features all` + `trunk build` -- clean
2. `cargo test --features all --workspace` -- all pass
3. Search "Putin" --> traverses graph, click node --> Detail modal opens
4. Search "F16" --> tries variations, finds "F-16", traverses
5. Search "asdfgh" --> no results, shows "no matches" message
6. In Detail modal: Investigate tab --> web results + KB results --> select --> commit --> graph updates
7. Close Detail modal --> graph still there with all state
