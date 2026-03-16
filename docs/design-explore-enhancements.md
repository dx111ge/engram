# Explore Page Enhancement Plan

## Context
The Explore/Graph page shows a 3D force-directed graph with basic search and depth controls. Nodes are colored by confidence (not type), there's no filtering, no way to highlight the searched entity, and no context menu. This limits analysis -- you can't answer "show me only persons" or "what connects Putin to NATO" without manually scanning the graph.

## Design: 8 Enhancements in 3 Phases

### Phase 1: Quick Wins

**1. Node color by entity type** (foundation)
- Replace confidence-based coloring with type-based: person=#66bb6a, org=#4fc3f7, location=#42a5f5, event=#ffa726, product=#ab47bc, position=#78909c
- Confidence stays as node SIZE (already 4.0 + conf * 6.0)
- Change: `index.html` JS bridge `confColor()` → `typeColor()` (~10 lines)

**2. Entity type filter chips (DYNAMIC -- never hardcoded)**
- Derived from actual graph data: scan `nodes` signal, count per `node_type`
- Shows whatever types exist: "person (12)", "module (8)", "class (15)" -- adapts to any domain
- Colored dots auto-assigned from a palette (consistent hash of type name → color)
- Client-side instant filtering via `.nodeVisibility()` -- no API call
- New JS bridge method: `filter(hiddenTypes, hiddenRels)`
- New signals: `hidden_types`, `type_counts` derived, new GraphCanvas props
- Place in sidebar below Controls as "Filters" card

**3. Relation type filter chips (DYNAMIC -- from actual edges)**
- Derived from actual edges: scan `edges` signal, count per `label`
- Shows whatever relations exist: "related_to (25)", "imports (8)", "depends_on (12)"
- Adapts to codebase graphs, finance graphs, any domain
- Toggle to hide/show edge types, shares `filter()` JS method

**4. Highlight search result node**
- Start node: golden glow sphere (THREE.js), gold label (#ffd700), 1.5x size
- Pass `startNodeId` to JS bridge as 6th param in create()
- Camera auto-focuses after layout settles
- New signal: `start_node: Option<String>`

### Phase 2: Medium Effort

**5. Search mode selector**
- Dropdown next to search: "Entity" | "Type" | "Relation"
- Entity: current behavior (search → traverse)
- Type: query all nodes of that type (add `node_type: Option<String>` to QueryRequest -- small backend change)
- Relation: show all edges of a relationship type

**6. Edge label toggle**
- Checkbox in Controls: "Show edge labels" (default off)
- New JS bridge method: `toggleEdgeLabels(show)`

### Phase 3: Advanced

**7. Right-click context menu**
- HTML overlay div at mouse coords
- Actions: Expand, Hide, Pin/Unpin, Show only connections, Find path to...
- JS-only for display, Rust callback for API actions

**8. Path finding between two nodes**
- Client-side BFS on loaded graph data
- Highlight path edges in gold
- Triggered from context menu → click target

### Phase 4: Node Interaction & Actions

**9. Click-to-highlight + center**
- Clicking any node: golden highlight ring (same as search result), camera smoothly centers on it
- Previously selected node loses highlight (only one active at a time)
- Selected node label turns gold, size pulses briefly
- JS bridge: `highlightNode(nodeId)` method -- adds glow, centers camera, removes previous highlight

**10. Node action buttons in detail panel**
- When a node is selected, the sidebar detail panel shows action buttons:
  - "Research" -- triggers web search for this entity + area of interest, shows results in a sub-panel or modal. Feeds back into graph as new connections.
  - "Enrich from KB" -- re-runs Wikidata entity linking + property expansion for just this entity. Adds new discovered nodes/edges to the graph live.
  - "Ingest more" -- opens a text input pre-filled with the entity name. User can paste relevant text, which gets ingested with this entity as context. New entities/relations appear in the graph.
  - "Edit" -- edit entity type, confidence, properties inline
- Actions call backend endpoints (existing: `/ingest`, `/query`, `/proxy/search`) and merge results into the current graph view without full reload

### Phase 5: Temporal Facts

**11. Temporal edge properties in enrichment**
- Extend `property_expansion()` SPARQL to fetch P580 (start time) + P582 (end time) qualifiers for P39 (position held) and other temporal properties
- Store as edge properties: `valid_from`, `valid_to`
- Edges with `valid_to` set = historical, without = current/active
- Backend: extend SPARQL query in `rel_knowledge_base.rs`, store via `g.set_property()` on edges (or edge metadata)

**12. Temporal visualization in Explore**
- New filter toggle: "Current only" / "All (including historical)"
- Historical edges: dashed line rendering, lower opacity (0.3)
- Historical nodes: dimmed if ALL their edges are historical
- Edge tooltip shows date range: "2014-2024" for historical, "2012-present" for active
- JS bridge: `setTemporalMode(currentOnly)` method that adjusts link/node rendering

## Files Modified

| File | Changes |
|------|---------|
| `crates/engram-ui/index.html` | typeColor(), filter(), toggleEdgeLabels(), findPath(), context menu, start node highlight |
| `crates/engram-ui/src/pages/graph.rs` | Filter signals, search mode, edge toggle, start_node, filter chips UI |
| `crates/engram-ui/src/components/graph_canvas.rs` | New props, separate Effects per feature |
| `crates/engram-ui/css/style.css` | Filter chip + context menu styles |
| `crates/engram-api/src/types.rs` | `node_type: Option<String>` on QueryRequest (Phase 2) |
| `crates/engram-api/src/handlers.rs` | Type-based query in query() handler (Phase 2) |
| `crates/engram-ingest/src/rel_knowledge_base.rs` | Extend property_expansion SPARQL for P580/P582 (Phase 5) |

## Implementation Status (updated 2026-03-16)

### Completed (Phase 1-4):
1. typeColor in JS bridge -- DONE
2. Entity type filter chips + JS filter method -- DONE (cascading: hides edges + orphaned nodes)
3. Relation type filter chips -- DONE (cascading: hides orphaned nodes)
4. Start node highlight (golden glow + camera) -- DONE
5. Edge label toggle -- DONE
6. ~~Search mode selector~~ -- REMOVED (replaced by smart search, see design-explore-v2.md)
7. Context menu -- DONE (Expand, Open Detail, Set as Start, Find Path To, Hide Type)
8. Path finding (client-side BFS) -- DONE (isolates path: only path nodes/edges visible)
9. Click-to-highlight + center -- DONE (opens Detail modal)
10. ~~Node action buttons~~ -- MOVED to Detail modal (see design-explore-v2.md)
11. Temporal SPARQL enrichment (P580/P582) -- DONE (backend)
12. Temporal visualization toggle -- DONE (dashed/dimmed historical edges)

### v2 Redesign (2026-03-16):
- Smart search: single search box with client-side fuzzy matching (F16 -> F-16)
- Detail modal: full entity view with Info/Connections/Investigate tabs
- Sidebar simplified: compact preview + "Open" button
- See `design-explore-v2.md` for full spec

## Key Architecture Decisions
- **Filtering is 100% client-side** via `.nodeVisibility()` / `.linkVisibility()` -- no API calls on toggle
- **ALL filters are dynamic, NEVER hardcoded** -- entity types, relation types, temporal ranges are all derived from the current graph result. No hardcoded list of "person, org, location". If the graph has "module", "class", "function" types (codebase), those appear as filter chips. If it has "version", "branch" relationships, those appear in relation filters. The filter chips are built from `type_counts` and `rel_counts` derived signals that scan the actual nodes/edges.
- **Temporal model is generic** -- `valid_from` / `valid_to` applies to ANY edge, not just Wikidata positions. For codebases: version ranges, deprecation dates. For products: release/EOL. For positions: start/end date. The enrichment pipeline stores these as edge properties whenever the source provides them (Wikidata P580/P582, structured imports, user-provided dates).
- **JS bridge** grows from 4 to ~8 methods: create, update, recenter, destroy, filter, toggleEdgeLabels, findPath, clearHighlight
- **GraphCanvas props** expand to ~8 (hidden_types, hidden_rels, start_node, show_edge_labels) -- each gets its own Effect
- **Context menu** is pure DOM/JS, Rust callbacks only for API actions
- **`create()` call** changes from `call5` to `apply` with Array (6+ args)

### Future Phases (separate plans)

**Phase 6: Detail Panel Rework** (separate plan)
- Redesign the node detail sidebar for richer information display
- Show properties, edge lists, canonical names, Wikidata links
- Inline editing of entity type, confidence, properties
- History/provenance view: who added this, when, from what source
- Related assessments / confidence timeline

**Phase 7: Chat Integration Rework** (separate plan)
- Review existing chat functionality and improve
- Context-aware: chat knows about the current graph view and selected node
- "Explain this connection" / "What do you know about X" queries
- LLM tool-calling integration for graph queries within chat
- Conversation history persistence

## Verification
1. `cargo build --features all` + `trunk build` -- clean compile
2. `cargo test --features all --workspace` -- 628 tests pass
3. Search "Putin" -- golden glow, persons green, orgs blue
4. Toggle type chips -- instant hide/show
5. Toggle relation chips -- edges hide/show
6. "Show edge labels" -- names on edges
7. Right-click node -- context menu
8. "Find path to..." → click target → gold path
9. Click any node → golden highlight, camera centers, previous selection clears
10. Click node → detail panel shows Research/Enrich/Ingest buttons → Research finds new connections
11. Search "Stoltenberg" → "Secretary General of NATO" edge shown dashed (historical, ended 2024)
12. Toggle "Current only" → historical edges disappear
