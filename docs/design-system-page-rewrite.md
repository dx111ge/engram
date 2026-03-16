# System Page Rewrite: Accordion to Card Grid + Modal

## Context
The current `crates/engram-ui/src/pages/system.rs` is 2533 lines with 9 CollapsibleSection accordion panels. This design is hard to scan and wastes vertical space. Rewrite to a card grid with modal editing.

## New Structure
1. **Main system.rs** (~200 lines): Tab bar (System | Mesh) + 3x2 card grid + modal state management
2. Organized with clear section comments: `// -- Card: Embeddings --`, `// -- Modal: Embeddings --`, etc.
3. Keep everything in ONE file for now (system.rs). Sub-modules are a follow-up.

## Design

### Layout
- **System tab**: 6 summary cards in a 3x2 CSS grid (3 columns on desktop, 1 on mobile)
- **Mesh tab**: "Coming Soon" placeholder with mesh icon and brief description
- Each card shows: icon, title, 1-2 line status summary, colored status dot, "Configure" button
- Clicking "Configure" / "Manage" opens a modal overlay (reuse the wizard-modal pattern from onboarding_wizard.rs)
- Modal contains the full settings UI (moved from the current collapsible content)

### The 6 Cards

**Row 1 (Pipeline):**

1. **Embeddings** - icon: `fa-circle-nodes`, status: "ONNX Local | 384D" or provider name, green dot if configured
   - Modal: provider selection (wizard-style cards), model selection, ONNX install, quantization toggle (int8 checkbox), reindex button

2. **NER & Relations** - icon: `fa-tags`, status: "GLiNER2 FP16 | Threshold 0.85", green dot
   - Modal: NER provider cards (Builtin, GLiNER2, LLM), model download, RE threshold slider, relation type presets (General/Custom), custom JSON import

3. **Language Model** - icon: `fa-comments`, status: "qwen2.5:7b | Ollama", green dot
   - Modal: provider cards (wizard-style: Ollama, LM Studio, OpenAI, vLLM, etc.), model selection, API key, system prompt textarea, temperature slider, thinking model toggle

**Row 2 (Administration):**

4. **Connection** - icon: `fa-plug`, status: "127.0.0.1:3030 | Connected", green dot
   - Modal: API URL input, Test button with result (node/edge count), Save button

5. **Secrets** - icon: `fa-key`, status: "2 keys stored" or "No secrets", lock icon
   - Modal: list of secret keys, add/delete, masked values

6. **Database** - icon: `fa-database`, status: "68 facts, 282 connections"
   - Modal: stats display, Import JSON-LD button, Export JSON-LD button, Reset Database button (with confirmation), Rerun Onboarding Wizard button

### Mesh Tab
- "Coming Soon" placeholder
- Mesh icon + brief description of planned features: peer discovery, federated query, knowledge profiles
- "This feature is under development" message

## Modal Pattern

Use a single signal for modal state:
```rust
let (modal_open, set_modal_open) = signal(String::new());
// "" = no modal, "embedding"/"ner"/"llm"/"connection"/"secrets"/"database" = that modal
```

Modal template (from onboarding_wizard.rs):
```rust
<div class=move || if modal_open.get() == "embedding" { "modal-overlay active" } else { "modal-overlay" }>
    <div class="wizard-modal">
        // modal content
        <button on:click=move |_| set_modal_open.set(String::new())>Close</button>
    </div>
</div>
```

## What to Keep vs Change

- **KEEP**: all signals, actions, API calls, provider presets, model lists, ONNX install logic, NER model download, LLM system prompt, secrets CRUD, import/export, reset
- **CHANGE**: layout from CollapsibleSection accordion to card grid + modals
- **ADD**: Relation Extraction section (threshold slider + relation type config from wizard's STEP_REL), Mesh tab, Rerun Wizard button
- **REMOVE**: CollapsibleSection import and usage

## CSS

```css
.system-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1rem;
}
.system-card {
    background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 8px;
    padding: 1.25rem;
    cursor: pointer;
    transition: all 0.2s;
    border-left: 3px solid var(--accent);
}
.system-card:hover {
    background: rgba(255,255,255,0.06);
    border-color: rgba(255,255,255,0.15);
}
.system-card h4 { margin: 0 0 0.5rem 0; }
.system-card .card-status { font-size: 0.85rem; color: rgba(255,255,255,0.6); }
.system-card .status-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; }
.system-card .status-dot.green { background: #66bb6a; }
.system-card .status-dot.amber { background: #ffa726; }
.system-card .status-dot.gray { background: #78909c; }
.system-tabs { display: flex; gap: 0; border-bottom: 2px solid rgba(255,255,255,0.1); margin-bottom: 1.5rem; }
.system-tab {
    padding: 0.75rem 1.5rem;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    color: rgba(255,255,255,0.5);
}
.system-tab.active { border-bottom-color: var(--accent); color: white; }
.system-tab .coming-soon {
    font-size: 0.65rem;
    background: rgba(255,255,255,0.1);
    padding: 1px 6px;
    border-radius: 8px;
    margin-left: 6px;
}
```

## Connection Test Fix

The connection test currently shows "0 nodes, 0 edges". Fix it to properly parse the stats response. The test hits the configured API URL's `/health` endpoint but should also hit `/stats` with the auth token to get real node/edge counts.

## Implementation Notes

- Do NOT create new files. Rewrite system.rs in place.
- Must compile with `trunk build` (Leptos WASM).
- Card summary status should derive from existing config/stats resources.
- Organize code with clear section comments for each card and modal.

## Implementation Status (updated 2026-03-16)

### Completed:
- 6-card grid layout with status summaries (shows model names)
- Tab bar (System | Mesh) with Mesh "Coming Soon"
- Modal pattern with wizard-modal-header/body CSS
- Wizard-style card grids for Embedding and LLM provider selection (Quality/Privacy/Cost)
- NER card-based provider selection (Built-in vs GLiNER)
- Close button top-right on all modals
- Save actions auto-close modals on success
- Config sync: providers/models pre-selected from stored config
- NLI references renamed to REL/Relation Extraction
- Coreference Resolution marked "Coming Soon" (not implemented)
- Relation Templates updated for GLiNER2 terminology
- LLM model chips (preset + fetched from Ollama/vLLM)
- LLM test endpoint fixed (/proxy/llm)
- Temperature display fixed (no floating point noise)
- Database modal: "Rerun Onboarding Wizard" button added
- Card statuses show model names (e.g. "ONNX Local | multilingual-e5-small")

### Remaining (follow-up):
- System page modals still differ from onboarding wizard in deeper ways (wizard uses stepped flow, model download progress, etc.)
- Consider sharing wizard step components between onboarding and system modals

## Verification
1. `trunk build` -- clean compile
2. System tab shows 6 cards in 3x2 grid
3. Click any card -- modal opens with full settings
4. All existing functionality works: provider selection, model download, ONNX install, test buttons, secrets CRUD, import/export, reset
5. Mesh tab shows "Coming Soon" placeholder
6. Connection test shows real node/edge counts
7. Mobile: cards stack to single column
