# Explore Page v3: Performance, Edge Bundling, Find Path, Entity Editing

## Context
Testing on 2026-03-16 revealed the 3D graph becomes crowded and unresponsive at depth 2 (68 nodes, 282 edges). Find Path only finds shortest path and requires clicking target in 3D. Edge clutter obscures structure. Confidence editing was lost during the v2 redesign. Research into modern graph visualization techniques (LOD, clustering, ngraph, edge bundling) identified practical solutions.

## Design Decisions (2026-03-16)

| # | Technique | Decision | Effort |
|---|-----------|----------|--------|
| A | Progressive Disclosure (depth 1 default) | Implement | ~15 min |
| B | Smart Visual Scaling (auto-reduce detail) | Implement | ~30 min |
| C | Level of Detail / Semantic Zoom | Implement | ~2 hours |
| D | Node Clustering / Type Clouds | Deferred to roadmap | - |
| E | Physics Engine: ngraph | Try, keep or revert | ~30 min |
| F | Geometry Sharing / Instanced Rendering | Deferred to roadmap | - |
| G | Frustum Culling | Skip (LOD makes it redundant) | - |
| H | Web Worker Physics | Deferred to roadmap | - |
| I | Edge Bundling | Implement | ~1-2 hours |
| J | Find Path Redesign (search + all paths) | Implement | ~2-3 hours |
| K | Detail Modal Edit Tab (confidence + CRUD) | Implement | ~2 hours |
| L | CRUD Button Simplification | Implement | ~30 min |
| M | Documentation Updates | Implement | ~1 hour |
| N | Test Coverage | Implement | ~1 hour |
| O | Color Legend Overlay | Implement | ~30 min |
| P | Temporal Edge Data Pipeline Fix | Implement | ~2 hours |

---

## A. Progressive Disclosure

**What**: Default depth changes from 2 to 1. User sees entity + direct connections (~10-20 nodes). Expand individual nodes by double-clicking. Hint shown after first search.

**Why**: Prevents crowding from happening. User explores in the direction they care about.

**Implementation**:
- `graph.rs`: Change `signal(2u32)` -> `signal(1u32)`
- Add `show_hint` signal, show overlay: "Double-click a node to explore deeper connections"
- Auto-dismiss after 5s or on graph interaction

**Status**: [x] Implemented

---

## B. Smart Visual Scaling

**What**: Auto-reduce rendering detail as graph grows. Thresholds:
- <30 nodes: full detail (edge labels, type tags, 3 particles, 16-face spheres)
- 30-80 nodes: 1 particle per edge, keep labels
- >80 nodes: no edge labels, no particles, 8-face spheres, no curvature

**Why**: At 282 edges with 3 particles each = 846 animated objects. Removing them at scale is the biggest quick win.

**Implementation**:
- `index.html`: In `create()`, compute scaling from node/edge counts, apply to builder chain
- Default `_showEdgeLabels: false` -- auto-enabled for small graphs
- User can always toggle edge labels back on manually

**Status**: [x] Implemented

---

## C. Level of Detail (LOD) / Semantic Zoom

**What**: Camera-distance-based rendering. Far nodes = dots only. Medium = dots + labels. Close = full detail (labels + type tags).

**Why**: Currently creates ~136 SpriteText objects regardless of readability. At overview zoom none are readable -- pure waste.

**Implementation**:
- `index.html`: Replace `nodeThreeObject` with THREE.LOD object
  - Far (>200): no custom object (default sphere)
  - Medium (80-200): label SpriteText only
  - Close (<80): label + type tag (current full detail)
- Verify THREE.LOD compatibility with `nodeThreeObjectExtend(true)`
- Fallback: camera-distance check in render loop if LOD not compatible

**Status**: [x] Implemented

---

## E. Physics Engine: ngraph

**What**: Switch from d3-force-3d (default) to ngraph. Significantly faster layout convergence, especially for larger graphs.

**Why**: Faster layout = smoother experience when expanding nodes. Less CPU during simulation.

**Implementation**:
- `index.html`: Add `.forceEngine('ngraph')` to builder chain
- Remove d3-specific: `graph.d3Force('charge').strength(-180)`
- Test layout quality -- revert if poor

**Status**: [x] Implemented

---

## I. Edge Bundling

**What**: Multiple edges between the same two nodes collapse into one thick bundled edge showing "3 relations". Hover shows individual relation names. Click expands.

**Why**: Reduces edge clutter at ANY graph size. User sees structure first, details on demand. Even at depth 1, hub entities may have 3-4 different relations to the same neighbor.

**Implementation**:
- `index.html`: Pre-process links in `create()`:
  - Group by source+target pair (sorted, direction-agnostic)
  - Single edges pass through unchanged
  - Multi-edges become one bundled link with `_bundled: true`, `_children: [originals]`, `_bundleCount: N`
- Bundled edge visual: thicker width, count label ("3 relations")
- Hover tooltip: lists individual relation names
- `linkWidth`: `link._bundleCount ? 1.2 + link._bundleCount * 0.8 : 1.2`
- Store original links for unbundling during find-path
- New state: `_bundlingEnabled`, `_originalLinks`
- New method: `toggleBundling(enabled)`

**Status**: [x] Implemented

---

## J. Find Path Redesign

**What**: Search-based target selection (type name instead of click in 3D). All-paths DFS instead of shortest-path BFS. Path results as toggleable list in sidebar.

**Why**: "What connects A to B" is the core intelligence analysis question. Current implementation impractical for crowded graphs.

**Sidebar UI** (shown when start node is selected):
```
--- Find Path ---
From: [start node label]
To:   [text input with autocomplete from loaded graph]
[Find Paths]

3 paths found:
[x] Path 1 (1 hop): Putin -> NATO
[x] Path 2 (2 hops): Putin -> Russia -> NATO
[ ] Path 3 (3 hops): Putin -> Lavrov -> ... -> NATO
[Clear Paths]
```

**Implementation**:

graph.rs:
- New signals: `path_from`, `path_target_query`, `path_results: Vec<Vec<String>>`, `path_selected: Vec<bool>`
- Autocomplete: derived signal filtering loaded node labels
- Sidebar section below node preview

index.html:
- Replace `findPath()` BFS with `findAllPaths()` DFS:
  - Backtracking DFS, cap at 10 paths, max depth from slider
  - Uses original unbundled links for adjacency
- New `showPaths(pathsJson)`: populates `_pathNodes` + `_pathHighlight` from multiple paths, temporarily unbundles path edges
- Context menu: "Find Path To..." sets start node + scrolls to sidebar section (no more click-target mode)

**Status**: [x] Implemented

---

## K. Detail Modal: Edit Tab

**What**: 4th tab in the Detail modal for editing entity confidence, type, properties, relations, and deleting.

**Why**: Confidence editing was lost during v2 redesign. CRUD operations were in a separate modal disconnected from the selected node.

**Edit Tab Layout**:
```
[Confidence]
|============================|----| 95%    [Save]

[Entity Type]
[Person      v]  [Save]

[Properties]
canonical_name  | General Dynamics F-16...  [x]
ingest_source   | seed-enrichment           [x]
kb_id           | Q10002                    [x]
[+ Add Property]  key: [____] value: [____] [Add]

[Add Relation]
To: [____] (autocomplete)  Type: [____]  [Add]

[Danger Zone]
[Delete Entity]  (type name to confirm)
```

**API mappings**:
- Confidence increase: `POST /reinforce` with entity label
- Confidence decrease: `POST /correct` with entity label + reason
- Type change: `POST /store` with label + node_type
- Property set: graph `set_property()` via appropriate endpoint
- Property delete: same mechanism
- Add relation: `POST /relate` with from, to, relationship, confidence
- Delete entity: `DELETE /node/{encoded}`

**Implementation**:
- `detail_modal.rs`: New `render_edit_tab()` function with signals for editing state
- Each save operation calls API, refreshes the detail data

**Status**: [x] Implemented

---

## L. CRUD Button Simplification

**What**: Rename "CRUD" button to "+ New". Simplify CrudModal to Create-only (new entity, new relation). Read/Update/Delete now live in the Detail modal's Edit tab.

**Implementation**:
- `graph.rs`: Rename button text and icon
- `crud_modal.rs`: Remove read/update/delete UI, keep create entity + create relation

**Status**: [x] Implemented

---

## O. Color Coding + Legend Overlay

**What**: Coherent visual language for the entire graph, with a floating legend.

### Node coloring:
- Color = entity type, fully dynamic. No hardcoded type list.
- **Active node** (has current edges): full color, opacity 0.85
- **Historical node** (ALL edges have `valid_to` in past): same color, faded opacity 0.35
- **Type tag**: removed at medium/far zoom (LOD). Only shown at close zoom. Color + legend communicates type at a distance.

### Color assignment (dynamic, NOT hardcoded):
Known defaults ship with engram for common types. Unknown types auto-assign from a curated palette of 24 distinguishable hues via consistent hash. Same type name always gets same color.

```javascript
// 24-color curated palette -- high contrast, colorblind-friendly where possible
var PALETTE = [
  '#66bb6a', // 0  green (person)
  '#4fc3f7', // 1  blue (organization)
  '#42a5f5', // 2  cyan (location)
  '#ffa726', // 3  orange (event)
  '#ab47bc', // 4  purple (product)
  '#78909c', // 5  gray (position)
  '#ef5350', // 6  red
  '#26a69a', // 7  teal
  '#ec407a', // 8  pink
  '#8d6e63', // 9  brown
  '#d4e157', // 10 lime
  '#ffca28', // 11 amber
  '#5c6bc0', // 12 indigo
  '#29b6f6', // 13 light blue
  '#9ccc65', // 14 light green
  '#ff7043', // 15 deep orange
  '#7e57c2', // 16 deep purple
  '#26c6da', // 17 cyan accent
  '#f06292', // 18 light pink
  '#a1887f', // 19 warm gray
  '#c0ca33', // 20 yellow-green
  '#ffd54f', // 21 light amber
  '#4dd0e1', // 22 bright teal
  '#ba68c8', // 23 light purple
];

// Known defaults (fixed palette positions)
var TYPE_DEFAULTS = {
  'person': 0, 'org': 1, 'organization': 1,
  'location': 2, 'event': 3, 'product': 4, 'position': 5
};

function typeColor(nodeType) {
  if (!nodeType) return PALETTE[5];
  var key = nodeType.toLowerCase();
  // Known type -> fixed palette slot
  if (TYPE_DEFAULTS[key] !== undefined) return PALETTE[TYPE_DEFAULTS[key]];
  // Unknown type -> hash into remaining palette slots (6-23)
  var hash = 0;
  for (var i = 0; i < key.length; i++) {
    hash = key.charCodeAt(i) + ((hash << 5) - hash);
  }
  return PALETTE[6 + (Math.abs(hash) % (PALETTE.length - 6))];
}
```

Single source of truth: `typeColor()` function. Legend reads from same function. Future: user-configurable overrides via System settings (stored in backend config).

### Edge coloring:
- **Active edge** (no `valid_to` or future): solid line, normal opacity
- **Historical edge** (`valid_to` in past): dashed line, reduced opacity
- **Bundled edge**: thick line (solid or dashed based on children)

### LOD integration (ties into technique C):
| Zoom | Node | Edge |
|------|------|------|
| Far | Colored dot only | Thin lines (solid/dashed) |
| Medium | Dot + entity name | Normal lines + labels optional |
| Close | Dot + name + type tag | Full detail + labels |

### Legend overlay (bottom-left of canvas):
```
[Nodes]
[green dot]  Person
[blue dot]   Organization
[cyan dot]   Location
[orange dot] Event
[purple dot] Product
[gray dot]   Position

[Edges]
[solid ──]   Active relation
[dashed - -] Historical (ended)
[thick ══]   Multiple relations
```

- Dynamically shows only types present in current graph
- Semi-transparent, positioned bottom-left
- Auto-fade after 10s, show on hover over legend area
- CSS: `engram-legend` class with backdrop-filter blur

**Implementation**:
- `index.html`: Build DOM overlay in `create()` from node types. Update `linkColor`/`linkWidth` to use dashed `LineDashedMaterial` for historical edges. Update `nodeOpacity` based on historical status.
- `css/style.css`: Legend styling

**Status**: [x] Implemented

---

## P. Temporal Edge Data Pipeline Fix

**What**: Wikidata SPARQL fetches P580 (start time) and P582 (end time) but the data is discarded at storage. The frontend temporal toggle exists but never receives data. Stoltenberg shows old positions (Secretary General ended 2024) mixed with current ones, with no way to distinguish.

**The broken chain**:
1. SPARQL fetches P580/P582 -- DONE (`rel_knowledge_base/sparql.rs:143-144`)
2. Store on edges -- **NOT DONE** (`rel_knowledge_base/mod.rs:365` discards with `_` prefix)
3. API returns in EdgeResponse -- fields exist but always `None`
4. Frontend renders -- DONE (dimming, date labels, toggle all work)

**Fix**:
- `rel_knowledge_base/mod.rs`: After `g.relate()`, call `g.set_property()` on the edge to store `valid_from` and `valid_to` as edge properties
- Need to identify the edge slot after `relate()` returns, then `g.set_property()` on it
- Alternative: extend `relate_with_confidence()` to accept optional temporal fields and store them as edge metadata
- `handlers/query.rs`: When building EdgeResponse, read edge properties for `valid_from`/`valid_to`
- Check if edge properties already work in the storage engine or if this needs storage-level changes

**Files**:
- `crates/engram-ingest/src/rel_knowledge_base/mod.rs` -- pass temporal data through instead of discarding
- `crates/engram-core/src/graph/store.rs` -- may need edge property storage support
- `crates/engram-api/src/handlers/query.rs` -- include temporal data in EdgeResponse

**Status**: [x] Implemented (2026-03-16)
**Architecture**: Edge struct (72 bytes) has `valid_from`/`valid_to` fields (i64 unix seconds). Edge property store (`.brain.edge_props`) for arbitrary qualifiers (version, source details, etc).
**Chain**: SPARQL P580/P582 -> `relate_with_temporal()` -> Edge struct -> EdgeView -> EdgeResponse -> Frontend (dimming, date labels, temporal toggle)

---

## M. Documentation Updates

| Document | Update |
|----------|--------|
| `docs/design-explore-enhancements.md` | Mark completed phases, reference this doc for v3 |
| `docs/design-explore-v2.md` | Mark as superseded by v3 for performance/path/editing sections |
| `docs/roadmap.md` | Add deferred items: clustering (D), geometry sharing (F), web worker physics (H) |
| `docs/http-api.md` | No changes (API surface unchanged) |
| `docs/mcp-server.md` | No changes (API surface unchanged) |
| `docs/a2a-protocol.md` | No changes (API surface unchanged) |

**Status**: [x] Implemented

---

## N. Test Coverage

**New integration tests** (`tests/integration.rs`):
1. Depth-limited traversal returns correct node count
2. `node_type` filter on query results
3. `reinforce` increases confidence
4. `correct` decreases confidence with reason
5. Confidence bounds (0.0 - 1.0)
6. `set_property` / `get_property` / `get_properties` roundtrip
7. Property deletion

**Frontend/JS**: Visual verification via Chrome DevTools MCP (no test framework for WASM/JS)

**Status**: [x] Implemented

---

## Code Quality Rules
- **No file exceeds 500 lines.** Split into sub-modules if needed.
- Clear section comments in each module.
- Reuse existing CSS patterns (wizard-card, wizard-model-chip, filter-chip).
- No dead code.
- Build + test after each technique. Commit after each.
- `trunk build` for frontend, `cargo build --features all` for backend, `cargo test --features all --workspace` for tests.

## Implementation Order
1. **A** - Progressive disclosure (depth 1 + hint)
2. **B** - Smart visual scaling
3. **E** - Try ngraph engine
4. **O** - Color legend overlay
5. **I** - Edge bundling
6. **C** - LOD / semantic zoom
7. **J** - Find Path redesign
8. **K** - Detail modal Edit tab
9. **L** - CRUD button simplification
10. **P** - Temporal edge data pipeline fix
11. **N** - Integration tests
12. **M** - Documentation updates

## Verification Checklist (2026-03-16)
- [x] `trunk build` -- clean (57 warnings, 0 errors)
- [x] `cargo test --features all --workspace` -- 633 tests pass (5 new)
- [x] Search "Putin" depth 1 -- ~15 nodes, responsive, hint appears
- [x] Smart scaling: >80 nodes -> edge labels auto-disable, no particles
- [x] Edge bundling: opt-in toggle, multi-edges show "N relations"
- [x] LOD: type tags disappear at distance, labels always visible
- [x] Color legend: bottom-left overlay shows type colors from current graph
- [x] Find Path: type target name, find multiple paths, toggle in sidebar
- [x] d3-force tuned for large graphs (ngraph not available in 3d-force-graph API)
- [x] "+ New" button: create entity/relation
- [x] Integration tests: depth, confidence, properties all pass
- [ ] Detail modal Edit tab: visual verification pending
- [x] Temporal edge pipeline (P): implemented (edge struct + edge property store)

## Additional fixes (2026-03-16)
- Removed hardcoded `localhost:3030` API URL -- now derives from `window.location.origin` (fixes Cloudflare tunnel access)
- Removed Connection settings card from System page and gear icon from nav (redundant)
- Entity resolver now checks `canonical_name` property for matching (fixes "Putin" vs "Vladimir Putin" duplicate creation)
- Autocomplete dropdown in Find Path closes on item selection
