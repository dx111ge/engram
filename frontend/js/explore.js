/* ============================================
   engram - Explore View (Graph + CRUD Modal)
   ============================================ */

let exploreNetwork = null;
let exploreNodes = null;
let exploreEdges = null;

// CRUD modal state
let crudEditMode = false;
let crudEditLabel = null;
let crudSearchTimeout = null;
let crudDeleteSearchTimeout = null;
let crudDeleteSelectedLabel = null;

// vis.js renders to canvas - CSS variables not available, use hex
function exploreNodeColor(c) {
  if (c >= 0.7) return '#00b894';
  if (c >= 0.4) return '#fdcb6e';
  return '#d63031';
}

const QUESTION_PREFIXES = ['what', 'how', 'who', 'where', 'when', 'why', 'is', 'are', 'does', 'do', 'can'];

function isQuestion(text) {
  const lower = text.toLowerCase().trim();
  return QUESTION_PREFIXES.some(p => lower.startsWith(p + ' ') || lower.startsWith(p + '?'));
}

router.register('/explore', () => {
  renderTo(`
    <div style="display:flex;flex-direction:column;height:calc(100vh - 60px);gap:0">

      <div style="display:flex;align-items:center;gap:0.5rem;padding:0.75rem 1rem 0.5rem;flex-shrink:0">
        <div style="position:relative;flex:1">
          <i class="fa-solid fa-magnifying-glass" style="position:absolute;left:0.75rem;top:50%;transform:translateY(-50%);color:var(--text-secondary);font-size:0.85rem"></i>
          <input type="text" id="explore-search" placeholder="Search or ask a question..."
            style="width:100%;padding:0.55rem 0.75rem 0.55rem 2.2rem;background:var(--bg-elevated);border:1px solid var(--border);border-radius:var(--radius);color:var(--text-primary);font-size:0.9rem"
            autofocus>
        </div>
        <button class="btn btn-primary" id="explore-go" style="padding:0.55rem 1rem;white-space:nowrap">
          <i class="fa-solid fa-arrow-right"></i> Go
        </button>
        <button class="btn btn-secondary" id="explore-add-btn" title="Manage facts" style="padding:0.55rem 1rem;white-space:nowrap">
          <i class="fa-solid fa-pen-to-square"></i>
        </button>
      </div>

      <div style="display:flex;flex:1;min-height:0;gap:0;padding:0 1rem 0.75rem">

        <div id="explore-graph-view" style="flex:1;min-width:0;position:relative;background:var(--bg-elevated);border:1px solid var(--border);border-radius:var(--radius)">
          <div class="empty-state" id="explore-graph-empty" style="position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;pointer-events:none">
            <i class="fa-solid fa-diagram-project" style="font-size:2rem;color:var(--text-secondary);margin-bottom:0.5rem"></i>
            <p style="color:var(--text-secondary);margin:0">Search for a fact to explore its connections</p>
            <p style="color:var(--text-muted);font-size:0.8rem;margin:0.25rem 0 0">Click a node to see details. Double-click to expand.</p>
          </div>
        </div>

        <div style="width:280px;flex-shrink:0;margin-left:0.75rem;display:flex;flex-direction:column;gap:0.5rem;overflow-y:auto">

          <div style="background:var(--bg-elevated);border:1px solid var(--border);border-radius:var(--radius);padding:0.6rem 0.75rem">
            <div style="font-size:0.75rem;font-weight:600;color:var(--text-secondary);text-transform:uppercase;letter-spacing:0.04em;margin-bottom:0.5rem">
              <i class="fa-solid fa-sliders" style="margin-right:0.3rem"></i>Controls
            </div>

            <div style="margin-bottom:0.5rem">
              <label style="display:flex;justify-content:space-between;font-size:0.8rem;color:var(--text-secondary);margin-bottom:0.2rem">
                <span>Connection depth</span>
                <span id="explore-depth-val" style="color:var(--text-primary);font-weight:600">2</span>
              </label>
              <input type="range" id="explore-depth" min="1" max="5" value="2" style="width:100%">
            </div>

            <div style="margin-bottom:0.5rem">
              <label style="display:flex;justify-content:space-between;font-size:0.8rem;color:var(--text-secondary);margin-bottom:0.2rem">
                <span>Min strength</span>
                <span id="explore-conf-val" style="color:var(--text-primary);font-weight:600">0.0</span>
              </label>
              <input type="range" id="explore-conf" min="0" max="100" value="0" style="width:100%">
            </div>

            <div style="margin-bottom:0.5rem">
              <label style="display:block;font-size:0.8rem;color:var(--text-secondary);margin-bottom:0.2rem">Direction</label>
              <select id="explore-direction" style="width:100%;padding:0.3rem 0.4rem;font-size:0.8rem;background:var(--bg-card);border:1px solid var(--border);border-radius:var(--radius);color:var(--text-primary)">
                <option value="both">Both directions</option>
                <option value="out">Outgoing only</option>
                <option value="in">Incoming only</option>
              </select>
            </div>

            <div>
              <label style="display:block;font-size:0.8rem;color:var(--text-secondary);margin-bottom:0.2rem">Layout</label>
              <select id="explore-layout" style="width:100%;padding:0.3rem 0.4rem;font-size:0.8rem;background:var(--bg-card);border:1px solid var(--border);border-radius:var(--radius);color:var(--text-primary)">
                <option value="forceAtlas2Based">Force Atlas</option>
                <option value="barnesHut">Barnes Hut</option>
                <option value="repulsion">Repulsion</option>
                <option value="hierarchicalRepulsion">Hierarchical</option>
              </select>
            </div>
          </div>

          <div id="explore-node-panel" style="background:var(--bg-elevated);border:1px solid var(--border);border-radius:var(--radius);padding:0.6rem 0.75rem;flex:1;min-height:0;overflow-y:auto">
            <div style="font-size:0.75rem;font-weight:600;color:var(--text-secondary);text-transform:uppercase;letter-spacing:0.04em;margin-bottom:0.5rem">
              <i class="fa-solid fa-circle-info" style="margin-right:0.3rem"></i>Details
            </div>
            <div id="explore-node-content">
              <p style="font-size:0.8rem;color:var(--text-secondary);margin:0">Click a node in the graph to see its details here.</p>
            </div>
          </div>

        </div>

      </div>

    </div>

    <!-- CRUD Modal -->
    <div class="modal-overlay" id="crud-modal">
      <div class="crud-modal">
        <div class="modal-header">
          <h3><i class="fa-solid fa-pen-to-square"></i> Manage Knowledge</h3>
          <button class="btn-icon" id="crud-close"><i class="fa-solid fa-xmark"></i></button>
        </div>
        <div class="crud-tabs">
          <button class="active" data-tab="create"><i class="fa-solid fa-plus"></i> Create</button>
          <button data-tab="edit"><i class="fa-solid fa-pencil"></i> Edit</button>
          <button data-tab="connect"><i class="fa-solid fa-link"></i> Connect</button>
          <button data-tab="delete"><i class="fa-solid fa-trash-can"></i> Delete</button>
        </div>
        <div class="modal-body" id="crud-body">
        </div>
      </div>
    </div>
  `);

  initExploreGraph();
  setupExploreEvents();
  setupCrudModal();

  // Check for query param
  const params = new URLSearchParams(location.hash.split('?')[1] || '');
  const nodeParam = params.get('node');
  const queryParam = params.get('q');
  if (nodeParam) {
    document.getElementById('explore-search').value = nodeParam;
    exploreExpandNode(nodeParam);
  } else if (queryParam) {
    document.getElementById('explore-search').value = queryParam;
    doExploreSearch(queryParam);
  }
});

function initExploreGraph() {
  exploreNodes = new vis.DataSet();
  exploreEdges = new vis.DataSet();

  const container = document.getElementById('explore-graph-view');
  const data = { nodes: exploreNodes, edges: exploreEdges };
  const options = {
    nodes: {
      shape: 'dot',
      font: { color: '#e6e6e6', size: 14, face: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif' },
      borderWidth: 2,
      shadow: { enabled: true, size: 5, color: 'rgba(0,0,0,0.3)' },
    },
    edges: {
      color: { color: '#3a4a6c', highlight: '#4a9eff', hover: '#4a9eff' },
      font: { color: '#a0a0b8', size: 11, strokeWidth: 0, face: '-apple-system, sans-serif' },
      arrows: { to: { enabled: true, scaleFactor: 0.6 } },
      smooth: { type: 'continuous' },
    },
    physics: {
      solver: 'forceAtlas2Based',
      forceAtlas2Based: { gravitationalConstant: -80, springLength: 120 },
      stabilization: { iterations: 100 },
    },
    interaction: {
      hover: true,
      tooltipDelay: 200,
      navigationButtons: false,
      keyboard: true,
    },
    layout: { improvedLayout: true },
  };

  exploreNetwork = new vis.Network(container, data, options);

  exploreNetwork.on('click', (params) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      const node = exploreNodes.get(nodeId);
      showExploreNodeSidebar(node);
    }
  });

  exploreNetwork.on('doubleClick', (params) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      const node = exploreNodes.get(nodeId);
      exploreExpandNode(node.label);
    }
  });
}

function setupExploreEvents() {
  const searchInput = document.getElementById('explore-search');
  const goBtn = document.getElementById('explore-go');
  const depthSlider = document.getElementById('explore-depth');
  const confSlider = document.getElementById('explore-conf');
  const layoutSelect = document.getElementById('explore-layout');

  const doGo = () => {
    const text = searchInput.value.trim();
    if (!text) return;
    doExploreSearch(text);
  };

  goBtn.addEventListener('click', doGo);
  searchInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') doGo(); });

  depthSlider.addEventListener('input', () => {
    document.getElementById('explore-depth-val').textContent = depthSlider.value;
  });
  confSlider.addEventListener('input', () => {
    document.getElementById('explore-conf-val').textContent = (confSlider.value / 100).toFixed(1);
  });

  layoutSelect.addEventListener('change', () => {
    const solver = layoutSelect.value;
    exploreNetwork.setOptions({
      physics: { solver },
      layout: solver === 'hierarchicalRepulsion'
        ? { hierarchical: { direction: 'UD', sortMethod: 'directed' } }
        : { hierarchical: false },
    });
  });
}

async function doExploreSearch(text) {
  if (isQuestion(text)) {
    await doExploreAsk(text);
  } else {
    await doExploreTextSearch(text);
  }
}

async function doExploreAsk(question) {
  const emptyEl = document.getElementById('explore-graph-empty');

  try {
    const result = await engram.ask({ question });
    if (!result.results || result.results.length === 0) {
      showToast('No answer found for that question. Try rephrasing or searching with keywords.', 'info');
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';

    if (result.interpretation) {
      showToast(result.interpretation, 'info');
    }

    for (const r of result.results) {
      if (r.label) {
        await exploreExpandNode(r.label);
      }
    }
  } catch (err) {
    showToast('Question failed: ' + err.message, 'error');
  }
}

async function doExploreTextSearch(query) {
  try {
    const data = await engram.search({ query, limit: 50 });
    const results = data.results || [];

    if (results.length === 0) {
      showToast('No results found. Try different keywords or ask a question.', 'info');
      return;
    }

    if (results.length > 0 && results[0].label) {
      await exploreExpandNode(results[0].label);
    }

    if (results.length > 1) {
      showToast(`Found ${results.length} results. Showing top result in graph.`, 'info');
    }
  } catch (err) {
    showToast('Search failed: ' + err.message, 'error');
  }
}

async function exploreExpandNode(label) {
  const depth = parseInt(document.getElementById('explore-depth').value);
  const minConf = parseInt(document.getElementById('explore-conf').value) / 100;
  const direction = document.getElementById('explore-direction').value;

  try {
    const result = await engram.query({ start: label, depth, min_confidence: minConf, direction });
    const emptyEl = document.getElementById('explore-graph-empty');
    if (emptyEl) emptyEl.style.display = 'none';

    if (!result.nodes || result.nodes.length === 0) {
      showToast(`No connections found for "${label}"`, 'info');
      return;
    }

    // Add nodes
    for (const n of result.nodes) {
      const size = 10 + (n.confidence || 0.5) * 25;
      const color = exploreNodeColor(n.confidence || 0.5);
      const existing = exploreNodes.get(n.node_id);
      const sLabel = strengthLabel(n.confidence || 0);
      const nodeData = {
        id: n.node_id,
        label: n.label,
        size,
        color: { background: color, border: color, highlight: { background: '#4a9eff', border: '#4a9eff' } },
        title: `${n.label}\nStrength: ${sLabel} (${((n.confidence || 0) * 100).toFixed(0)}%)`,
        confidence: n.confidence,
        depth: n.depth,
        nodeLabel: n.label,
      };
      if (existing) {
        exploreNodes.update(nodeData);
      } else {
        exploreNodes.add(nodeData);
      }
    }

    // Add edges
    for (const e of result.edges) {
      const fromNode = result.nodes.find(n => n.label === e.from);
      const toNode = result.nodes.find(n => n.label === e.to);
      if (!fromNode || !toNode) continue;

      const edgeId = `${fromNode.node_id}-${e.relationship}-${toNode.node_id}`;
      if (!exploreEdges.get(edgeId)) {
        const eLabel = strengthLabel(e.confidence || 0);
        exploreEdges.add({
          id: edgeId,
          from: fromNode.node_id,
          to: toNode.node_id,
          label: e.relationship,
          title: `${e.relationship} -- ${eLabel} (${((e.confidence || 0) * 100).toFixed(0)}%)`,
        });
      }
    }

    exploreNetwork.fit({ animation: { duration: 500 } });
  } catch (err) {
    showToast(`Could not explore "${label}": ${err.message}`, 'error');
  }
}

async function showExploreNodeSidebar(node) {
  const panel = document.getElementById('explore-node-content');
  panel.innerHTML = loadingHTML('Loading details...');

  try {
    const data = await engram.getNode(node.nodeLabel || node.label);
    let propsHTML = '';
    if (data.properties && Object.keys(data.properties).length > 0) {
      propsHTML = Object.entries(data.properties)
        .map(([k, v]) => `<div class="prop-row"><span class="prop-key">${escapeHtml(k)}</span><span>${escapeHtml(String(v))}</span></div>`)
        .join('');
    } else {
      propsHTML = '<p style="font-size:0.8rem;color:var(--text-secondary);margin:0">No additional details</p>';
    }

    const conf = data.confidence ?? node.confidence ?? 0;
    panel.innerHTML = `
      <div style="margin-bottom:0.5rem">
        <strong style="font-size:0.95rem">${escapeHtml(data.label || node.label)}</strong>
        ${strengthBadge(conf)}
      </div>
      ${confidenceBar(conf)}
      <div style="margin-top:0.5rem">
        <div style="font-size:0.75rem;color:var(--text-secondary);margin-bottom:0.2rem;font-weight:600">Properties</div>
        <div class="node-info-panel">${propsHTML}</div>
      </div>
      <div style="margin-top:0.5rem;display:flex;gap:0.4rem;flex-wrap:wrap">
        <a href="#/node/${encodeURIComponent(data.label || node.label)}" class="btn btn-sm btn-primary">
          <i class="fa-solid fa-arrow-up-right-from-square"></i> Full Details
        </a>
        <button class="btn btn-sm btn-secondary" onclick="exploreExpandNode('${escapeHtml(data.label || node.label)}')">
          <i class="fa-solid fa-expand"></i> Expand
        </button>
      </div>
    `;
  } catch (err) {
    panel.innerHTML = `<p style="font-size:0.8rem;color:var(--text-secondary)">Could not load details: ${escapeHtml(err.message)}</p>`;
  }
}


/* ============================================
   CRUD Modal
   ============================================ */

function setupCrudModal() {
  const modal = document.getElementById('crud-modal');
  const addBtn = document.getElementById('explore-add-btn');
  const closeBtn = document.getElementById('crud-close');

  // Open modal
  addBtn.addEventListener('click', () => {
    crudEditMode = false;
    crudEditLabel = null;
    crudDeleteSelectedLabel = null;
    modal.classList.add('visible');
    renderCrudTab('create');
  });

  // Close modal
  closeBtn.addEventListener('click', () => {
    modal.classList.remove('visible');
  });

  // Click overlay to close
  modal.addEventListener('click', (e) => {
    if (e.target === modal) modal.classList.remove('visible');
  });

  // Escape key to close
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && modal.classList.contains('visible')) {
      modal.classList.remove('visible');
    }
  });

  // Tab switching
  document.querySelectorAll('.crud-tabs button').forEach(btn => {
    btn.addEventListener('click', () => {
      switchCrudTab(btn.dataset.tab);
    });
  });

  // Render initial tab
  renderCrudTab('create');
}

function switchCrudTab(tabName) {
  document.querySelectorAll('.crud-tabs button').forEach(b => {
    b.classList.toggle('active', b.dataset.tab === tabName);
  });
  renderCrudTab(tabName);
}

function renderCrudTab(tabName) {
  const body = document.getElementById('crud-body');
  if (!body) return;

  switch (tabName) {
    case 'create':
      renderCreateTab(body);
      break;
    case 'edit':
      renderEditTab(body);
      break;
    case 'connect':
      renderConnectTab(body);
      break;
    case 'delete':
      renderDeleteTab(body);
      break;
  }
}


/* --- Create Tab --- */

function renderCreateTab(container) {
  container.innerHTML = `
    <div class="form-group">
      <label>Label <span style="color:var(--error)">*</span></label>
      <input type="text" id="crud-entity" placeholder="e.g. Rust, Albert Einstein, Machine Learning">
    </div>
    <div class="form-group">
      <label>Type</label>
      <input type="text" id="crud-type" placeholder="person, place, concept, technology...">
    </div>
    <div class="form-group">
      <label>Properties</label>
      <div class="kv-pairs" id="crud-props">
        <div class="kv-row">
          <input type="text" placeholder="key">
          <input type="text" placeholder="value">
          <button class="btn-icon crud-remove-kv" title="Remove"><i class="fa-solid fa-xmark"></i></button>
        </div>
      </div>
      <button class="btn btn-sm btn-secondary mt-1" id="crud-add-prop-btn">
        <i class="fa-solid fa-plus"></i> Add Property
      </button>
    </div>
    <div class="form-group">
      <label>Source / Author</label>
      <input type="text" id="crud-source" placeholder="where you learned this">
    </div>
    <div class="form-group">
      <label>Strength</label>
      <div class="slider-group">
        <label><span>Confidence level</span><span id="crud-strength-val">95%</span></label>
        <input type="range" id="crud-strength" min="0" max="100" value="95">
      </div>
    </div>
    <div style="display:flex;gap:0.5rem">
      <button class="btn btn-success" id="crud-create-btn" style="flex:1">
        <i class="fa-solid fa-check"></i> Create
      </button>
    </div>
    <div id="crud-create-result" class="mt-1"></div>
  `;

  setupCreateTabEvents();
}

function setupCreateTabEvents() {
  // Strength slider
  document.getElementById('crud-strength').addEventListener('input', (e) => {
    document.getElementById('crud-strength-val').textContent = e.target.value + '%';
  });

  // Add property row
  document.getElementById('crud-add-prop-btn').addEventListener('click', () => {
    crudAddKVRow('crud-props');
  });

  // Remove property row
  document.getElementById('crud-props').addEventListener('click', (e) => {
    const removeBtn = e.target.closest('.crud-remove-kv');
    if (removeBtn) {
      const kvContainer = document.getElementById('crud-props');
      removeBtn.closest('.kv-row').remove();
      if (kvContainer.children.length === 0) crudAddKVRow('crud-props');
    }
  });

  // Create button
  document.getElementById('crud-create-btn').addEventListener('click', async () => {
    const entity = document.getElementById('crud-entity').value.trim();
    if (!entity) { showToast('Please enter a label', 'error'); return; }

    const type = document.getElementById('crud-type').value.trim() || undefined;
    const source = document.getElementById('crud-source').value.trim() || undefined;
    const confidence = parseInt(document.getElementById('crud-strength').value) / 100;
    const properties = crudGetKVPairs('crud-props');

    const btn = document.getElementById('crud-create-btn');
    btn.disabled = true;

    try {
      const result = await engram.store({
        entity, type, source, confidence,
        properties: Object.keys(properties).length > 0 ? properties : undefined,
      });

      document.getElementById('crud-create-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Created: ${escapeHtml(result.label || entity)}</div>`;
      showToast('Fact created: ' + (result.label || entity), 'success');

      // Clear the label field for next entry
      document.getElementById('crud-entity').value = '';

      // Refresh graph if the node might be visible
      refreshGraphAfterCrud(result.label || entity);
    } catch (err) {
      showToast('Could not create: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });
}


/* --- Edit Tab --- */

function renderEditTab(container) {
  crudEditMode = false;
  crudEditLabel = null;

  container.innerHTML = `
    <div class="form-group">
      <label>Search for a fact to edit</label>
      <div style="position:relative">
        <i class="fa-solid fa-magnifying-glass" style="position:absolute;left:0.75rem;top:50%;transform:translateY(-50%);color:var(--text-muted);font-size:0.8rem"></i>
        <input type="text" id="crud-edit-search" placeholder="Type to search..." style="padding-left:2.2rem">
        <div id="crud-edit-dropdown" style="display:none;position:absolute;top:100%;left:0;right:0;z-index:200;background:var(--bg-card);border:1px solid var(--border);border-top:none;border-radius:0 0 var(--radius-sm) var(--radius-sm);max-height:200px;overflow-y:auto;box-shadow:0 4px 12px var(--shadow)"></div>
      </div>
    </div>
    <div id="crud-edit-form" style="display:none">
      <div style="margin-bottom:0.75rem">
        <span style="font-size:0.75rem;padding:0.2rem 0.5rem;border-radius:999px;background:rgba(74,158,255,0.2);color:var(--accent-bright);border:1px solid rgba(74,158,255,0.3);font-weight:600;text-transform:uppercase;letter-spacing:0.03em">
          <i class="fa-solid fa-pencil"></i> Editing: <span id="crud-edit-label-text"></span>
        </span>
      </div>
      <div class="form-group">
        <label>Label <span style="color:var(--error)">*</span></label>
        <input type="text" id="crud-edit-entity">
      </div>
      <div class="form-group">
        <label>Type</label>
        <input type="text" id="crud-edit-type">
      </div>
      <div class="form-group">
        <label>Properties</label>
        <div class="kv-pairs" id="crud-edit-props">
          <div class="kv-row">
            <input type="text" placeholder="key">
            <input type="text" placeholder="value">
            <button class="btn-icon crud-remove-kv" title="Remove"><i class="fa-solid fa-xmark"></i></button>
          </div>
        </div>
        <button class="btn btn-sm btn-secondary mt-1" id="crud-edit-add-prop-btn">
          <i class="fa-solid fa-plus"></i> Add Property
        </button>
      </div>
      <div class="form-group">
        <label>Source / Author</label>
        <input type="text" id="crud-edit-source">
      </div>
      <div class="form-group">
        <label>Strength</label>
        <div class="slider-group">
          <label><span>Confidence level</span><span id="crud-edit-strength-val">95%</span></label>
          <input type="range" id="crud-edit-strength" min="0" max="100" value="95">
        </div>
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-success" id="crud-edit-save-btn" style="flex:1">
          <i class="fa-solid fa-check"></i> Save Changes
        </button>
      </div>
      <div id="crud-edit-result" class="mt-1"></div>
    </div>
  `;

  setupEditTabEvents();
}

function setupEditTabEvents() {
  const searchInput = document.getElementById('crud-edit-search');
  const dropdown = document.getElementById('crud-edit-dropdown');

  // Search with debounce
  searchInput.addEventListener('input', () => {
    clearTimeout(crudSearchTimeout);
    const q = searchInput.value.trim();
    if (!q) { dropdown.style.display = 'none'; return; }
    crudSearchTimeout = setTimeout(async () => {
      try {
        const results = await engram.search({ query: q });
        const items = results.results || results.entities || results || [];
        if (!Array.isArray(items) || items.length === 0) {
          dropdown.innerHTML = '<div style="padding:0.75rem;color:var(--text-muted);font-size:0.85rem">No results found</div>';
          dropdown.style.display = 'block';
          return;
        }
        dropdown.innerHTML = items.slice(0, 15).map(item => {
          const label = item.label || item.entity || item;
          const type = item.node_type || item.type || '';
          const conf = item.confidence != null ? Math.round(item.confidence * 100) + '%' : '';
          return `<div class="crud-search-item" data-label="${escapeHtml(typeof label === 'string' ? label : String(label))}" style="padding:0.5rem 0.75rem;cursor:pointer;display:flex;align-items:center;justify-content:space-between;gap:0.5rem;border-bottom:1px solid var(--border);transition:background 0.15s">
            <div>
              <span style="font-weight:500">${escapeHtml(typeof label === 'string' ? label : String(label))}</span>
              ${type ? '<span style="font-size:0.75rem;color:var(--text-muted);margin-left:0.4rem;text-transform:uppercase">' + escapeHtml(type) + '</span>' : ''}
            </div>
            ${conf ? '<span style="font-size:0.75rem;color:var(--text-secondary)">' + conf + '</span>' : ''}
          </div>`;
        }).join('');
        dropdown.style.display = 'block';
      } catch (err) {
        dropdown.innerHTML = '<div style="padding:0.75rem;color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ' + escapeHtml(err.message) + '</div>';
        dropdown.style.display = 'block';
      }
    }, 300);
  });

  // Select item from dropdown
  dropdown.addEventListener('click', (e) => {
    const item = e.target.closest('.crud-search-item');
    if (item) {
      const label = item.getAttribute('data-label');
      dropdown.style.display = 'none';
      searchInput.value = label;
      loadNodeForCrudEdit(label);
    }
  });

  // Hover effects
  dropdown.addEventListener('mouseover', (e) => {
    const item = e.target.closest('.crud-search-item');
    if (item) item.style.background = 'var(--bg-hover)';
  });
  dropdown.addEventListener('mouseout', (e) => {
    const item = e.target.closest('.crud-search-item');
    if (item) item.style.background = '';
  });

  // Close dropdown on click outside
  document.getElementById('crud-body').addEventListener('click', (e) => {
    if (!e.target.closest('#crud-edit-search') && !e.target.closest('#crud-edit-dropdown')) {
      dropdown.style.display = 'none';
    }
  });
}

async function loadNodeForCrudEdit(label) {
  const form = document.getElementById('crud-edit-form');
  form.style.display = 'none';

  try {
    const node = await engram.getNode(label);

    crudEditMode = true;
    crudEditLabel = label;

    // Show form and populate
    form.style.display = '';
    document.getElementById('crud-edit-label-text').textContent = label;
    document.getElementById('crud-edit-entity').value = node.label || label;
    document.getElementById('crud-edit-type').value = node.node_type || node.type || '';
    document.getElementById('crud-edit-source').value = node.source || '';

    const conf = node.confidence != null ? Math.round(node.confidence * 100) : 95;
    document.getElementById('crud-edit-strength').value = conf;
    document.getElementById('crud-edit-strength-val').textContent = conf + '%';

    // Populate properties
    const propsContainer = document.getElementById('crud-edit-props');
    propsContainer.innerHTML = '';
    const props = node.properties || {};
    const keys = Object.keys(props);
    if (keys.length > 0) {
      keys.forEach(k => {
        const row = document.createElement('div');
        row.className = 'kv-row';
        row.innerHTML = `
          <input type="text" placeholder="key" value="${escapeHtml(k)}">
          <input type="text" placeholder="value" value="${escapeHtml(String(props[k]))}">
          <button class="btn-icon crud-remove-kv" title="Remove"><i class="fa-solid fa-xmark"></i></button>
        `;
        propsContainer.appendChild(row);
      });
    } else {
      crudAddKVRow('crud-edit-props');
    }

    // Wire up edit-specific events
    setupEditFormEvents();
  } catch (err) {
    showToast('Could not load node: ' + err.message, 'error');
  }
}

function setupEditFormEvents() {
  // Strength slider
  const strengthSlider = document.getElementById('crud-edit-strength');
  strengthSlider.addEventListener('input', (e) => {
    document.getElementById('crud-edit-strength-val').textContent = e.target.value + '%';
  });

  // Add property row
  document.getElementById('crud-edit-add-prop-btn').addEventListener('click', () => {
    crudAddKVRow('crud-edit-props');
  });

  // Remove property row
  document.getElementById('crud-edit-props').addEventListener('click', (e) => {
    const removeBtn = e.target.closest('.crud-remove-kv');
    if (removeBtn) {
      const kvContainer = document.getElementById('crud-edit-props');
      removeBtn.closest('.kv-row').remove();
      if (kvContainer.children.length === 0) crudAddKVRow('crud-edit-props');
    }
  });

  // Save button
  document.getElementById('crud-edit-save-btn').addEventListener('click', async () => {
    const entity = document.getElementById('crud-edit-entity').value.trim();
    if (!entity) { showToast('Please enter a label', 'error'); return; }

    const type = document.getElementById('crud-edit-type').value.trim() || undefined;
    const source = document.getElementById('crud-edit-source').value.trim() || undefined;
    const confidence = parseInt(document.getElementById('crud-edit-strength').value) / 100;
    const properties = crudGetKVPairs('crud-edit-props');

    const btn = document.getElementById('crud-edit-save-btn');
    btn.disabled = true;

    try {
      const result = await engram.store({
        entity, type, source, confidence,
        properties: Object.keys(properties).length > 0 ? properties : undefined,
      });

      document.getElementById('crud-edit-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Updated: ${escapeHtml(result.label || entity)}</div>`;
      showToast('Fact updated: ' + (result.label || entity), 'success');

      refreshGraphAfterCrud(result.label || entity);
    } catch (err) {
      showToast('Could not save: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });
}


/* --- Connect Tab --- */

function renderConnectTab(container) {
  container.innerHTML = `
    <div class="form-group">
      <label>From <span style="color:var(--error)">*</span></label>
      <input type="text" id="crud-conn-from" placeholder="e.g. Albert Einstein">
    </div>
    <div class="form-group">
      <label>Relationship <span style="color:var(--error)">*</span></label>
      <input type="text" id="crud-conn-rel" placeholder="works at, is part of, invented...">
    </div>
    <div class="form-group">
      <label>To <span style="color:var(--error)">*</span></label>
      <input type="text" id="crud-conn-to" placeholder="e.g. Princeton University">
    </div>
    <div class="form-group">
      <label>Strength</label>
      <div class="slider-group">
        <label><span>Connection strength</span><span id="crud-conn-strength-val">90%</span></label>
        <input type="range" id="crud-conn-strength" min="0" max="100" value="90">
      </div>
    </div>
    <div style="display:flex;gap:0.5rem">
      <button class="btn btn-success" id="crud-conn-btn" style="flex:1">
        <i class="fa-solid fa-link"></i> Connect
      </button>
    </div>
    <div id="crud-conn-result" class="mt-1"></div>
  `;

  setupConnectTabEvents();
}

function setupConnectTabEvents() {
  // Strength slider
  document.getElementById('crud-conn-strength').addEventListener('input', (e) => {
    document.getElementById('crud-conn-strength-val').textContent = e.target.value + '%';
  });

  // Connect button
  document.getElementById('crud-conn-btn').addEventListener('click', async () => {
    const from = document.getElementById('crud-conn-from').value.trim();
    const relationship = document.getElementById('crud-conn-rel').value.trim();
    const to = document.getElementById('crud-conn-to').value.trim();

    if (!from || !relationship || !to) {
      showToast('Please fill in all three fields', 'error');
      return;
    }

    const confidence = parseInt(document.getElementById('crud-conn-strength').value) / 100;
    const btn = document.getElementById('crud-conn-btn');
    btn.disabled = true;

    try {
      await engram.relate({ from, to, relationship, confidence });
      document.getElementById('crud-conn-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Connected: ${escapeHtml(from)} &rarr; ${escapeHtml(relationship)} &rarr; ${escapeHtml(to)}</div>`;
      showToast('Facts connected', 'success');

      refreshGraphAfterCrud(from);
      refreshGraphAfterCrud(to);
    } catch (err) {
      showToast('Could not connect: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });
}


/* --- Delete Tab --- */

function renderDeleteTab(container) {
  crudDeleteSelectedLabel = null;

  container.innerHTML = `
    <div class="form-group">
      <label>Search for a fact to delete</label>
      <div style="position:relative">
        <i class="fa-solid fa-magnifying-glass" style="position:absolute;left:0.75rem;top:50%;transform:translateY(-50%);color:var(--text-muted);font-size:0.8rem"></i>
        <input type="text" id="crud-del-search" placeholder="Type to search..." style="padding-left:2.2rem">
        <div id="crud-del-dropdown" style="display:none;position:absolute;top:100%;left:0;right:0;z-index:200;background:var(--bg-card);border:1px solid var(--border);border-top:none;border-radius:0 0 var(--radius-sm) var(--radius-sm);max-height:200px;overflow-y:auto;box-shadow:0 4px 12px var(--shadow)"></div>
      </div>
    </div>
    <div id="crud-del-selected" style="margin-bottom:0.75rem;font-size:0.85rem"></div>
    <div style="display:flex;gap:0.5rem">
      <button class="btn btn-danger" id="crud-del-btn" style="flex:1" disabled>
        <i class="fa-solid fa-trash-can"></i> Delete
      </button>
    </div>
    <div id="crud-del-result" class="mt-1"></div>
  `;

  setupDeleteTabEvents();
}

function setupDeleteTabEvents() {
  const searchInput = document.getElementById('crud-del-search');
  const dropdown = document.getElementById('crud-del-dropdown');
  const delBtn = document.getElementById('crud-del-btn');

  // Search with debounce
  searchInput.addEventListener('input', () => {
    clearTimeout(crudDeleteSearchTimeout);
    crudDeleteSelectedLabel = null;
    delBtn.disabled = true;
    document.getElementById('crud-del-selected').innerHTML = '';
    const q = searchInput.value.trim();
    if (!q) { dropdown.style.display = 'none'; return; }
    crudDeleteSearchTimeout = setTimeout(async () => {
      try {
        const results = await engram.search({ query: q });
        const items = results.results || results.entities || results || [];
        if (!Array.isArray(items) || items.length === 0) {
          dropdown.innerHTML = '<div style="padding:0.75rem;color:var(--text-muted);font-size:0.85rem">No results found</div>';
          dropdown.style.display = 'block';
          return;
        }
        dropdown.innerHTML = items.slice(0, 10).map(item => {
          const label = item.label || item.entity || item;
          return `<div class="crud-del-item" data-label="${escapeHtml(typeof label === 'string' ? label : String(label))}" style="padding:0.5rem 0.75rem;cursor:pointer;border-bottom:1px solid var(--border);transition:background 0.15s;font-size:0.9rem">
            <i class="fa-solid fa-circle-dot" style="color:var(--error);margin-right:0.4rem;font-size:0.7rem"></i>
            ${escapeHtml(typeof label === 'string' ? label : String(label))}
          </div>`;
        }).join('');
        dropdown.style.display = 'block';
      } catch (err) {
        dropdown.innerHTML = '<div style="padding:0.75rem;color:var(--error);font-size:0.85rem">' + escapeHtml(err.message) + '</div>';
        dropdown.style.display = 'block';
      }
    }, 300);
  });

  // Select item from dropdown
  dropdown.addEventListener('click', (e) => {
    const item = e.target.closest('.crud-del-item');
    if (item) {
      crudDeleteSelectedLabel = item.getAttribute('data-label');
      searchInput.value = crudDeleteSelectedLabel;
      dropdown.style.display = 'none';
      delBtn.disabled = false;
      document.getElementById('crud-del-selected').innerHTML =
        `<span style="color:var(--warning)"><i class="fa-solid fa-triangle-exclamation"></i> Ready to delete: <strong>${escapeHtml(crudDeleteSelectedLabel)}</strong></span>`;
    }
  });

  // Hover effects
  dropdown.addEventListener('mouseover', (e) => {
    const item = e.target.closest('.crud-del-item');
    if (item) item.style.background = 'var(--bg-hover)';
  });
  dropdown.addEventListener('mouseout', (e) => {
    const item = e.target.closest('.crud-del-item');
    if (item) item.style.background = '';
  });

  // Close dropdown on click outside
  document.getElementById('crud-body').addEventListener('click', (e) => {
    if (!e.target.closest('#crud-del-search') && !e.target.closest('#crud-del-dropdown')) {
      dropdown.style.display = 'none';
    }
  });

  // Delete button
  delBtn.addEventListener('click', async () => {
    if (!crudDeleteSelectedLabel) return;

    const confirmed = confirm('Are you sure you want to delete "' + crudDeleteSelectedLabel + '"? This cannot be undone.');
    if (!confirmed) return;

    delBtn.disabled = true;

    try {
      await engram.deleteNode(crudDeleteSelectedLabel);
      document.getElementById('crud-del-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Deleted: ${escapeHtml(crudDeleteSelectedLabel)}</div>`;
      showToast('Fact deleted: ' + crudDeleteSelectedLabel, 'success');

      // Remove from graph if visible
      removeNodeFromGraph(crudDeleteSelectedLabel);

      searchInput.value = '';
      document.getElementById('crud-del-selected').innerHTML = '';
      crudDeleteSelectedLabel = null;
    } catch (err) {
      document.getElementById('crud-del-result').innerHTML =
        `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Could not delete: ' + err.message, 'error');
      delBtn.disabled = false;
    }
  });
}


/* --- CRUD Helpers --- */

function crudAddKVRow(containerId) {
  const kvContainer = document.getElementById(containerId);
  const row = document.createElement('div');
  row.className = 'kv-row';
  row.innerHTML = `
    <input type="text" placeholder="key">
    <input type="text" placeholder="value">
    <button class="btn-icon crud-remove-kv" title="Remove"><i class="fa-solid fa-xmark"></i></button>
  `;
  kvContainer.appendChild(row);
}

function crudGetKVPairs(containerId) {
  const kvContainer = document.getElementById(containerId);
  const props = {};
  kvContainer.querySelectorAll('.kv-row').forEach(row => {
    const inputs = row.querySelectorAll('input');
    const k = inputs[0].value.trim();
    const v = inputs[1].value.trim();
    if (k && v) props[k] = v;
  });
  return props;
}

function refreshGraphAfterCrud(label) {
  if (!exploreNodes || !exploreNetwork) return;

  // Check if any node with this label is currently in the graph
  const existing = exploreNodes.get({
    filter: (item) => item.label === label || item.nodeLabel === label
  });

  if (existing.length > 0) {
    // Re-expand to refresh the graph data
    exploreExpandNode(label);
  }
}

function removeNodeFromGraph(label) {
  if (!exploreNodes || !exploreEdges || !exploreNetwork) return;

  const toRemove = exploreNodes.get({
    filter: (item) => item.label === label || item.nodeLabel === label
  });

  for (const node of toRemove) {
    // Remove connected edges
    const connectedEdges = exploreNetwork.getConnectedEdges(node.id);
    if (connectedEdges && connectedEdges.length > 0) {
      exploreEdges.remove(connectedEdges);
    }
    // Remove the node
    exploreNodes.remove(node.id);
  }
}
