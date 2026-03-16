# Chat System: Intelligence Analyst Workbench

## Status: Complete (2026-03-16)

## Design Decisions

| Decision | Choice |
|----------|--------|
| Scope | Knowledge chat only -- Explore + Insights pages. No chat on Home/System/Security |
| UX | Floating panel (current), session-only history |
| Context | Auto-context injection + tool calling. Visible "Retrieving context..." step |
| Temporal | Always include temporal data in tool responses and system prompt |
| Writes | Read + Write with batch confirmation (proposed actions shown as checklist) |
| Streaming | Deferred. Full response for now |
| Persona | Configurable via System page. Default: intelligence analyst. Always suggests follow-up questions |
| Page context | Inject current page state (selected node in Explore, current assessment in Insights) |

## Tool Inventory

### Existing tools (keep, enhanced with temporal data)
- `engram_store` -- Store fact/entity (write confirmation)
- `engram_relate` -- Create relationship (temporal params, write confirmation)
- `engram_query` -- Traverse from entity (temporal bounds in results)
- `engram_search` -- Full-text search
- `engram_similar` -- Semantic similarity
- `engram_explain` -- Entity provenance (temporal edges, edge properties)
- `engram_reinforce` -- Increase confidence (write confirmation)
- `engram_correct` -- Mark fact wrong (write confirmation)
- `engram_prove` -- Find evidence
- `engram_gaps` -- Knowledge gap scan (domain/topic filter)

### New tools: Temporal Queries
- `engram_temporal_query` -- Query edges by time range
- `engram_timeline` -- Chronological events for entity
- `engram_current_state` -- Only current (non-expired) relations

### New tools: Compare & Analytics
- `engram_compare` -- Side-by-side entity comparison
- `engram_shortest_path` -- Shortest path between entities
- `engram_most_connected` -- Top-N by edge count
- `engram_isolated` -- Nodes with few/no connections

### New tools: Ingest & Investigation
- `engram_ingest_text` -- Full NER+RE pipeline on text
- `engram_investigate` -- Web search + NER + ingest
- `engram_changes` -- What changed since timestamp
- `engram_watch` -- Monitor entity for changes

### New tools: Assessment & Reasoning
- `engram_assess_create` -- Create assessment
- `engram_assess_query` -- Get assessment details + evidence
- `engram_assess_evidence` -- Add evidence to assessment
- `engram_what_if` -- Simulate confidence cascade
- `engram_influence_path` -- Find indirect connections

### New tools: Action Engine
- `engram_rule_create` -- Create action rule
- `engram_rule_list` -- List active rules
- `engram_run_inference` -- Trigger inference
- `engram_schedule` -- Create/list scheduled tasks

### New tools: Reporting & Export
- `engram_briefing` -- Generate structured briefing
- `engram_export_subgraph` -- Export entity neighborhood
- `engram_entity_timeline` -- Chronological narrative

### New tools: Source Management
- `engram_sources_list` -- List configured sources
- `engram_source_trigger` -- Trigger immediate fetch
- `engram_source_coverage` -- Topic coverage for source

## Architecture

### Chat Flow
```
User types message
  -> 1. CONTEXT RETRIEVAL (visible "Retrieving context..." step)
     - Extract entities from user message
     - Search graph for matching entities
     - If on Explore: include selected node context
     - If on Insights: include current assessment context
  -> 2. BUILD PROMPT (system prompt + context + tools + history)
  -> 3. LLM CALL (POST /proxy/llm)
  -> 4. TOOL EXECUTION LOOP (max 5 rounds)
     - READ tools: execute immediately
     - WRITE tools: collect into pending batch
  -> 5. WRITE CONFIRMATION (if any writes pending)
     - Show batch as checklist
     - User selects which to apply
  -> 6. DISPLAY RESPONSE + follow-up suggestions
```

### System Prompt Structure
```
You are an intelligence analyst assistant for the engram knowledge graph.

CONTEXT (auto-injected):
- Current graph contains {node_count} entities, {edge_count} relations
- User is on the {current_page} page
- {selected entity context if any}
- {retrieved entities with temporal bounds}

CAPABILITIES:
You have access to {N} tools for querying, analyzing, and modifying the knowledge graph.

BEHAVIOR:
- Cite confidence levels when reporting facts
- Flag low-confidence (<40%) data explicitly
- After answering, suggest 2-3 follow-up questions or actions
- Distinguish current vs historical using temporal bounds
- Never invent facts -- suggest investigation if unknown
```

## Implementation Tracking

| Phase | Status | Tests | Docs |
|-------|--------|-------|------|
| 1. Foundation | [x] | [x] | [x] |
| 2. Enhanced Existing Tools | [x] | [x] | [x] |
| 3. New Tool Backend | [x] | [x] | [x] |
| 4. Assessment & Reasoning | [x] | [x] | [x] |
| 5. Action & Reporting | [x] | [x] | [x] |
| 6. Polish | [x] | [x] | [x] |

## Phase Completion Criteria
Each phase is complete when:
1. All code changes compile cleanly (`cargo check --features all`)
2. `trunk build` succeeds
3. Phase-specific tests pass
4. No file exceeds 500 lines
5. Documentation updated per phase
6. `cargo test --features all --workspace` passes (no regressions)
