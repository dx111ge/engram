/* ============================================
   engram - Explore View (Graph-first layout)
   ============================================ */

let exploreNetwork = null;
let exploreNodes = null;
let exploreEdges = null;

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
  `);

  initExploreGraph();
  setupExploreEvents();

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
  // Show results as graph nodes when possible
  const emptyEl = document.getElementById('explore-graph-empty');

  try {
    const result = await engram.ask({ question });
    if (!result.results || result.results.length === 0) {
      showToast('No answer found for that question. Try rephrasing or searching with keywords.', 'info');
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';

    // Show interpretation as a toast
    if (result.interpretation) {
      showToast(result.interpretation, 'info');
    }

    // Add answer nodes to graph
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

    // Expand the first result (most relevant) into the graph
    if (results.length > 0 && results[0].label) {
      await exploreExpandNode(results[0].label);
    }

    // If multiple results, show a toast with count
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
