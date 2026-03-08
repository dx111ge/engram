/* ============================================
   engram - Graph Explorer View
   ============================================ */

let graphNetwork = null;
let graphNodes = null;
let graphEdges = null;

// vis.js renders to canvas, so CSS variables don't work — use hex directly
function graphNodeColor(c) {
  if (c >= 0.7) return '#00b894';
  if (c >= 0.4) return '#fdcb6e';
  return '#d63031';
}

router.register('/graph', () => {
  renderTo(`
    <div class="graph-controls">
      <input type="text" id="graph-search" placeholder="Enter node label to explore...">
      <button class="btn btn-primary" id="graph-go"><i class="fa-solid fa-search"></i> Explore</button>
      <button class="btn btn-secondary" id="graph-clear"><i class="fa-solid fa-eraser"></i> Clear</button>
    </div>
    <div class="graph-container">
      <div class="graph-canvas" id="graph-canvas">
        <div class="empty-state" id="graph-empty">
          <i class="fa-solid fa-diagram-project"></i>
          <p>Enter a node label above to start exploring</p>
          <p class="text-muted" style="font-size:0.85rem">Click a node to see details. Double-click to expand.</p>
        </div>
      </div>
      <div class="graph-sidebar">
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-sliders"></i> Controls</h3></div>
          <div class="slider-group">
            <label>Depth <span id="depth-val">2</span></label>
            <input type="range" id="graph-depth" min="1" max="5" value="2">
          </div>
          <div class="slider-group">
            <label>Min Confidence <span id="conf-val">0.0</span></label>
            <input type="range" id="graph-conf" min="0" max="100" value="0">
          </div>
          <div class="form-group">
            <label>Layout</label>
            <select id="graph-layout">
              <option value="forceAtlas2Based">Force Atlas</option>
              <option value="barnesHut">Barnes Hut</option>
              <option value="repulsion">Repulsion</option>
              <option value="hierarchicalRepulsion">Hierarchical</option>
            </select>
          </div>
        </div>
        <div class="card" id="node-detail-panel">
          <div class="card-header"><h3><i class="fa-solid fa-circle-info"></i> Node Details</h3></div>
          <div id="node-detail-content">
            <p class="text-muted" style="font-size:0.85rem">Click a node to see its details here.</p>
          </div>
        </div>
      </div>
    </div>
  `);

  initGraph();
  setupGraphEvents();

  // Check if there's a pre-set query (e.g. from search click)
  const params = new URLSearchParams(location.hash.split('?')[1] || '');
  const startNode = params.get('node');
  if (startNode) {
    document.getElementById('graph-search').value = startNode;
    expandNode(startNode);
  }
});

function initGraph() {
  graphNodes = new vis.DataSet();
  graphEdges = new vis.DataSet();

  const container = document.getElementById('graph-canvas');
  const data = { nodes: graphNodes, edges: graphEdges };
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

  graphNetwork = new vis.Network(container, data, options);

  graphNetwork.on('click', (params) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      const node = graphNodes.get(nodeId);
      showNodeSidebar(node);
    }
  });

  graphNetwork.on('doubleClick', (params) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      const node = graphNodes.get(nodeId);
      expandNode(node.label);
    }
  });
}

function setupGraphEvents() {
  const searchInput = document.getElementById('graph-search');
  const goBtn = document.getElementById('graph-go');
  const clearBtn = document.getElementById('graph-clear');
  const depthSlider = document.getElementById('graph-depth');
  const confSlider = document.getElementById('graph-conf');
  const layoutSelect = document.getElementById('graph-layout');

  const doSearch = () => {
    const label = searchInput.value.trim();
    if (label) expandNode(label);
  };

  goBtn.addEventListener('click', doSearch);
  searchInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') doSearch(); });

  clearBtn.addEventListener('click', () => {
    graphNodes.clear();
    graphEdges.clear();
    const emptyEl = document.getElementById('graph-empty');
    if (emptyEl) emptyEl.style.display = '';
    document.getElementById('node-detail-content').innerHTML =
      '<p class="text-muted" style="font-size:0.85rem">Click a node to see its details here.</p>';
  });

  depthSlider.addEventListener('input', () => {
    document.getElementById('depth-val').textContent = depthSlider.value;
  });
  confSlider.addEventListener('input', () => {
    document.getElementById('conf-val').textContent = (confSlider.value / 100).toFixed(2);
  });

  layoutSelect.addEventListener('change', () => {
    const solver = layoutSelect.value;
    graphNetwork.setOptions({
      physics: { solver },
      layout: solver === 'hierarchicalRepulsion'
        ? { hierarchical: { direction: 'UD', sortMethod: 'directed' } }
        : { hierarchical: false },
    });
  });
}

async function expandNode(label) {
  const depth = parseInt(document.getElementById('graph-depth').value);
  const minConf = parseInt(document.getElementById('graph-conf').value) / 100;

  try {
    const result = await engram.query({ start: label, depth, min_confidence: minConf });
    const emptyEl = document.getElementById('graph-empty');
    if (emptyEl) emptyEl.style.display = 'none';

    if (!result.nodes || result.nodes.length === 0) {
      showToast(`No results found for "${label}"`, 'info');
      return;
    }

    // Add nodes
    for (const n of result.nodes) {
      const size = 10 + (n.confidence || 0.5) * 25;
      const color = graphNodeColor(n.confidence || 0.5);
      const existing = graphNodes.get(n.node_id);
      const nodeData = {
        id: n.node_id,
        label: n.label,
        size,
        color: { background: color, border: color, highlight: { background: '#4a9eff', border: '#4a9eff' } },
        title: `${n.label}\nConfidence: ${((n.confidence || 0) * 100).toFixed(0)}%`,
        confidence: n.confidence,
        depth: n.depth,
        nodeLabel: n.label,
      };
      if (existing) {
        graphNodes.update(nodeData);
      } else {
        graphNodes.add(nodeData);
      }
    }

    // Add edges
    for (const e of result.edges) {
      // Find node IDs by label
      const fromNode = result.nodes.find(n => n.label === e.from);
      const toNode = result.nodes.find(n => n.label === e.to);
      if (!fromNode || !toNode) continue;

      const edgeId = `${fromNode.node_id}-${e.relationship}-${toNode.node_id}`;
      if (!graphEdges.get(edgeId)) {
        graphEdges.add({
          id: edgeId,
          from: fromNode.node_id,
          to: toNode.node_id,
          label: e.relationship,
          title: `${e.relationship} (${((e.confidence || 0) * 100).toFixed(0)}%)`,
        });
      }
    }

    graphNetwork.fit({ animation: { duration: 500 } });
  } catch (err) {
    showToast(`Query failed: ${err.message}`, 'error');
  }
}

async function showNodeSidebar(node) {
  const panel = document.getElementById('node-detail-content');
  panel.innerHTML = loadingHTML('Loading node...');

  try {
    const data = await engram.getNode(node.nodeLabel || node.label);
    let propsHTML = '';
    if (data.properties && Object.keys(data.properties).length > 0) {
      propsHTML = Object.entries(data.properties)
        .map(([k, v]) => `<div class="prop-row"><span class="prop-key">${escapeHtml(k)}</span><span>${escapeHtml(String(v))}</span></div>`)
        .join('');
    } else {
      propsHTML = '<p class="text-muted" style="font-size:0.85rem">No properties</p>';
    }

    const conf = data.confidence ?? node.confidence ?? 0;
    panel.innerHTML = `
      <div style="margin-bottom:0.75rem">
        <strong style="font-size:1.1rem">${escapeHtml(data.label || node.label)}</strong>
        ${tierBadge(conf)}
      </div>
      ${confidenceBar(conf)}
      <div class="mt-2">
        <h4 style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.3rem">Properties</h4>
        <div class="node-info-panel">${propsHTML}</div>
      </div>
      <div class="mt-2" style="display:flex;gap:0.5rem;flex-wrap:wrap">
        <a href="#/node/${encodeURIComponent(data.label || node.label)}" class="btn btn-sm btn-primary">
          <i class="fa-solid fa-arrow-up-right-from-square"></i> Full Details
        </a>
        <button class="btn btn-sm btn-secondary" onclick="expandNode('${escapeHtml(data.label || node.label)}')">
          <i class="fa-solid fa-expand"></i> Expand
        </button>
      </div>
    `;
  } catch (err) {
    panel.innerHTML = `<p class="text-muted">Could not load node details: ${escapeHtml(err.message)}</p>`;
  }
}
