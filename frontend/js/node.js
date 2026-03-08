/* ============================================
   engram - Node Detail View
   ============================================ */

router.register('/node/:label', async (label) => {
  renderTo(loadingHTML('Loading node...'));

  try {
    const data = await engram.getNode(label);
    renderNodeDetail(data, label);
  } catch (err) {
    renderTo(`
      <div class="view-header">
        <h1><i class="fa-solid fa-circle-exclamation"></i> Node Not Found</h1>
      </div>
      <div class="card">
        ${emptyStateHTML('fa-circle-exclamation', `Could not load node "${escapeHtml(label)}": ${escapeHtml(err.message)}`)}
        <div class="flex-center mt-2">
          <a href="#/" class="btn btn-secondary"><i class="fa-solid fa-arrow-left"></i> Back to Dashboard</a>
        </div>
      </div>
    `);
  }
});

function renderNodeDetail(data, label) {
  const conf = data.confidence ?? 0;

  // Properties table
  let propsHTML;
  const props = data.properties || {};
  if (Object.keys(props).length > 0) {
    propsHTML = `
      <table>
        <thead><tr><th>Key</th><th>Value</th></tr></thead>
        <tbody>
          ${Object.entries(props).map(([k, v]) =>
            `<tr><td><strong>${escapeHtml(k)}</strong></td><td>${escapeHtml(String(v))}</td></tr>`
          ).join('')}
        </tbody>
      </table>`;
  } else {
    propsHTML = '<p class="text-muted">No properties stored for this node.</p>';
  }

  // Edges
  const edgesFrom = data.edges_from || [];
  const edgesTo = data.edges_to || [];

  const edgeFromHTML = edgesFrom.length > 0
    ? `<ul class="edge-list">${edgesFrom.map(e => `
        <li>
          <i class="fa-solid fa-arrow-right"></i>
          <span class="edge-rel">${escapeHtml(e.relationship)}</span>
          <i class="fa-solid fa-arrow-right" style="font-size:0.7rem;color:var(--text-muted)"></i>
          <a href="#/node/${encodeURIComponent(e.to)}">${escapeHtml(e.to)}</a>
          <span class="text-muted" style="font-size:0.8rem;margin-left:auto">${((e.confidence || 0) * 100).toFixed(0)}%</span>
        </li>`).join('')}</ul>`
    : '<p class="text-muted">No outgoing edges.</p>';

  const edgeToHTML = edgesTo.length > 0
    ? `<ul class="edge-list">${edgesTo.map(e => `
        <li>
          <i class="fa-solid fa-arrow-left"></i>
          <a href="#/node/${encodeURIComponent(e.from)}">${escapeHtml(e.from)}</a>
          <i class="fa-solid fa-arrow-right" style="font-size:0.7rem;color:var(--text-muted)"></i>
          <span class="edge-rel">${escapeHtml(e.relationship)}</span>
          <span class="text-muted" style="font-size:0.8rem;margin-left:auto">${((e.confidence || 0) * 100).toFixed(0)}%</span>
        </li>`).join('')}</ul>`
    : '<p class="text-muted">No incoming edges.</p>';

  renderTo(`
    <div class="node-detail-layout">
      <div class="node-heading">
        <h1><i class="fa-solid fa-circle-nodes"></i> ${escapeHtml(data.label || label)}</h1>
        <div class="node-meta">
          ${tierBadge(conf)}
          ${data.type ? `<span class="badge badge-active">${escapeHtml(data.type)}</span>` : ''}
        </div>
      </div>

      <div class="grid-2 mb-2">
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-chart-simple"></i> Confidence</h3></div>
          ${confidenceBar(conf)}
        </div>
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-arrows-left-right"></i> Connections</h3></div>
          <div class="flex-between">
            <div style="text-align:center">
              <div style="font-size:1.5rem;font-weight:700;color:var(--accent-bright)">${edgesFrom.length}</div>
              <div class="text-muted" style="font-size:0.8rem">Outgoing</div>
            </div>
            <div style="text-align:center">
              <div style="font-size:1.5rem;font-weight:700;color:var(--accent-bright)">${edgesTo.length}</div>
              <div class="text-muted" style="font-size:0.8rem">Incoming</div>
            </div>
            <a href="#/graph?node=${encodeURIComponent(data.label || label)}" class="btn btn-sm btn-primary">
              <i class="fa-solid fa-diagram-project"></i> View Graph
            </a>
          </div>
        </div>
      </div>

      <div class="card mb-2">
        <div class="card-header"><h3><i class="fa-solid fa-table-list"></i> Properties</h3></div>
        <div class="table-wrap">${propsHTML}</div>
      </div>

      <div class="grid-2 mb-2">
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-arrow-right-from-bracket"></i> Outgoing Edges</h3></div>
          ${edgeFromHTML}
        </div>
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-arrow-right-to-bracket"></i> Incoming Edges</h3></div>
          ${edgeToHTML}
        </div>
      </div>

      <div class="card mb-2">
        <div class="card-header"><h3><i class="fa-solid fa-wrench"></i> Learning Actions</h3></div>
        <div class="grid-2">
          <div>
            <div class="form-group">
              <label>Reinforce</label>
              <div class="form-row">
                <div class="form-group" style="margin-bottom:0">
                  <input type="text" id="reinforce-source" placeholder="Source (optional)">
                </div>
                <button class="btn btn-success" id="btn-reinforce">
                  <i class="fa-solid fa-arrow-up"></i> Reinforce
                </button>
              </div>
            </div>
          </div>
          <div>
            <div class="form-group">
              <label>Correct</label>
              <div class="form-row">
                <div class="form-group" style="margin-bottom:0">
                  <input type="text" id="correct-reason" placeholder="Reason for correction">
                </div>
                <button class="btn btn-danger" id="btn-correct">
                  <i class="fa-solid fa-arrow-down"></i> Correct
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="card">
        <button class="btn btn-danger" id="btn-delete-node">
          <i class="fa-solid fa-trash"></i> Delete Node
        </button>
        <span class="text-muted" style="margin-left:0.5rem;font-size:0.85rem">This performs a soft delete.</span>
      </div>
    </div>
  `);

  // Event handlers
  document.getElementById('btn-reinforce').addEventListener('click', async () => {
    const source = document.getElementById('reinforce-source').value.trim();
    try {
      await engram.reinforce({ entity: data.label || label, source: source || undefined });
      showToast(`Reinforced "${data.label || label}"`, 'success');
      location.hash = `#/node/${encodeURIComponent(data.label || label)}`;
    } catch (err) {
      showToast(`Reinforce failed: ${err.message}`, 'error');
    }
  });

  document.getElementById('btn-correct').addEventListener('click', async () => {
    const reason = document.getElementById('correct-reason').value.trim();
    if (!reason) {
      showToast('Please provide a reason for the correction', 'error');
      return;
    }
    try {
      await engram.correct({ entity: data.label || label, reason });
      showToast(`Corrected "${data.label || label}"`, 'success');
      location.hash = `#/node/${encodeURIComponent(data.label || label)}`;
    } catch (err) {
      showToast(`Correction failed: ${err.message}`, 'error');
    }
  });

  document.getElementById('btn-delete-node').addEventListener('click', async () => {
    if (!confirm(`Are you sure you want to delete "${data.label || label}"? This is a soft delete.`)) return;
    try {
      await engram.deleteNode(data.label || label);
      showToast(`Deleted "${data.label || label}"`, 'success');
      location.hash = '#/';
    } catch (err) {
      showToast(`Delete failed: ${err.message}`, 'error');
    }
  });
}
