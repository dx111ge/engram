/* ============================================
   engram - Mesh Network View
   Peer management, discovery, sync status
   ============================================ */

router.register('/mesh', async () => {
  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-network-wired"></i> Mesh Network</h1>
        <p class="text-secondary" style="margin-top:0.25rem">Collaborative knowledge sharing between engram instances</p>
      </div>
    </div>
    <div id="mesh-content">${loadingHTML('Connecting to mesh...')}</div>
  `);

  await loadMeshView();
});

async function loadMeshView() {
  const container = document.getElementById('mesh-content');

  // Detect whether mesh is enabled by probing identity endpoint
  let meshEnabled = true;
  let identity = null;
  let peers = [];

  try {
    identity = await engram.meshIdentity();
  } catch (err) {
    if (err.message && (err.message.includes('501') || err.message.includes('not enabled'))) {
      meshEnabled = false;
    }
  }

  if (!meshEnabled) {
    container.innerHTML = renderMeshDisabled();
    return;
  }

  // Fetch peers in parallel
  try {
    const result = await engram.meshPeers();
    peers = Array.isArray(result) ? result : (result && result.peers ? result.peers : []);
  } catch (_) {}

  container.innerHTML = renderMeshEnabled(identity, peers);
  bindMeshEvents(peers);
}

// ── Mesh Disabled Info Page ──

function renderMeshDisabled() {
  return `
    <div class="card" style="margin-bottom:1.5rem;text-align:center;padding:2rem">
      <i class="fa-solid fa-network-wired" style="font-size:3rem;color:var(--text-muted);margin-bottom:1rem"></i>
      <h2 style="margin-bottom:0.5rem">Mesh Networking Not Enabled</h2>
      <p style="color:var(--text-secondary);max-width:600px;margin:0 auto 1.5rem">
        Mesh networking allows engram instances to sync knowledge with each other,
        creating a distributed knowledge graph across multiple nodes.
      </p>
    </div>

    <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem;margin-bottom:1.5rem">
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-circle-info"></i> What It Does</h3>
        </div>
        <div style="display:flex;flex-direction:column;gap:0.75rem;font-size:0.9rem;color:var(--text-secondary)">
          <div style="display:flex;gap:0.75rem;align-items:flex-start">
            <i class="fa-solid fa-arrows-rotate" style="color:var(--accent-bright);margin-top:0.15rem;flex-shrink:0"></i>
            <span>Sync facts, relationships, and confidence scores between instances</span>
          </div>
          <div style="display:flex;gap:0.75rem;align-items:flex-start">
            <i class="fa-solid fa-shield-halved" style="color:var(--accent-bright);margin-top:0.15rem;flex-shrink:0"></i>
            <span>Zero-trust security with ed25519 identity, mutual peering, and topic-level ACLs</span>
          </div>
          <div style="display:flex;gap:0.75rem;align-items:flex-start">
            <i class="fa-solid fa-code-merge" style="color:var(--accent-bright);margin-top:0.15rem;flex-shrink:0"></i>
            <span>Automatic conflict resolution with configurable trust-weighted merging</span>
          </div>
          <div style="display:flex;gap:0.75rem;align-items:flex-start">
            <i class="fa-solid fa-magnifying-glass" style="color:var(--accent-bright);margin-top:0.15rem;flex-shrink:0"></i>
            <span>Federated queries across the mesh for distributed knowledge search</span>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-toggle-on"></i> Enable Mesh Networking</h3>
        </div>
        <div style="font-size:0.9rem;color:var(--text-secondary)">
          <p style="margin-bottom:1rem">Mesh networking is available but not yet activated on this instance.</p>
          <div style="font-size:0.8rem;color:var(--text-muted);text-transform:uppercase;letter-spacing:0.05em;margin-bottom:0.5rem">Topology</div>
          <div style="display:flex;flex-direction:column;gap:0.5rem;margin-bottom:1rem">
            <label style="display:flex;gap:0.5rem;align-items:center;cursor:pointer">
              <input type="radio" name="mesh-topology" value="star" checked style="accent-color:var(--accent-bright)">
              <span><strong>Star</strong> -- one hub, many spokes. Simple, centralized sync.</span>
            </label>
            <label style="display:flex;gap:0.5rem;align-items:center;cursor:pointer">
              <input type="radio" name="mesh-topology" value="full" style="accent-color:var(--accent-bright)">
              <span><strong>Full mesh</strong> -- every node connects to every other. Maximum redundancy.</span>
            </label>
            <label style="display:flex;gap:0.5rem;align-items:center;cursor:pointer">
              <input type="radio" name="mesh-topology" value="ring" style="accent-color:var(--accent-bright)">
              <span><strong>Ring</strong> -- each node syncs with two neighbors. Gossip propagation.</span>
            </label>
          </div>
          <button id="mesh-enable-btn" style="padding:0.5rem 1rem;background:var(--accent);color:#fff;border:none;border-radius:var(--radius-sm);cursor:pointer;font-size:0.9rem;display:flex;align-items:center;justify-content:center;gap:0.5rem;width:100%"
            onclick="(async()=>{
              const btn=document.getElementById('mesh-enable-btn');
              btn.disabled=true;btn.innerHTML='<span class=\\'spinner\\'></span> Enabling...';
              const topology=document.querySelector('input[name=mesh-topology]:checked').value;
              try{
                await engram._post('/config',{mesh_enabled:true,mesh_topology:topology});
                showToast('Mesh networking enabled. Restart the server to activate.','success');
                btn.innerHTML='<i class=\\'fa-solid fa-check\\'></i> Enabled';
              }catch(err){
                showToast('Failed to enable mesh: '+err.message,'error');
                btn.disabled=false;btn.innerHTML='<i class=\\'fa-solid fa-power-off\\'></i> Enable Mesh';
              }
            })()"
          >
            <i class="fa-solid fa-power-off"></i> Enable Mesh
          </button>
          <p style="margin-top:0.75rem;font-size:0.8rem;color:var(--text-muted)">
            <i class="fa-solid fa-circle-info" style="margin-right:0.25rem"></i>
            After enabling, restart the engram server to activate mesh endpoints.
          </p>
        </div>
      </div>
    </div>`;
}

// ── Mesh Enabled View ──

function renderMeshEnabled(identity, peers) {
  let html = '';

  // Top row: Identity + Add Peer
  html += `<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem;margin-bottom:1rem">`;

  // Identity card
  const pubKey = identity
    ? (identity.public_key || identity.id || (typeof identity === 'string' ? identity : JSON.stringify(identity)))
    : 'Unknown';
  const shortKey = pubKey.length > 24 ? pubKey.substring(0, 24) + '...' : pubKey;

  html += `
    <div class="card">
      <div class="card-header">
        <h3><i class="fa-solid fa-fingerprint"></i> Identity</h3>
      </div>
      <div style="display:flex;flex-direction:column;gap:0.75rem">
        <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
          <i class="fa-solid fa-key" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
          <span style="font-size:0.85rem;color:var(--text-secondary);min-width:80px">Public Key</span>
          <span style="font-size:0.9rem;font-family:var(--font-mono);word-break:break-all" title="${escapeHtml(pubKey)}">${escapeHtml(shortKey)}</span>
        </div>
        <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0">
          <i class="fa-solid fa-signal" style="color:var(--success);width:20px;text-align:center;flex-shrink:0"></i>
          <span style="font-size:0.85rem;color:var(--text-secondary);min-width:80px">Status</span>
          <span style="font-size:0.9rem;color:var(--success)"><i class="fa-solid fa-circle" style="font-size:0.5rem;vertical-align:middle"></i> Online</span>
        </div>
      </div>
    </div>`;

  // Add Peer card
  html += `
    <div class="card">
      <div class="card-header">
        <h3><i class="fa-solid fa-user-plus"></i> Add Peer</h3>
      </div>
      <div style="display:flex;flex-direction:column;gap:0.75rem">
        <div>
          <label style="font-size:0.8rem;color:var(--text-secondary);display:block;margin-bottom:0.25rem">Endpoint</label>
          <input type="text" id="mesh-peer-endpoint" placeholder="http://host:3030" style="width:100%;padding:0.5rem 0.75rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font-size:0.9rem">
        </div>
        <div>
          <label style="font-size:0.8rem;color:var(--text-secondary);display:block;margin-bottom:0.25rem">Name</label>
          <input type="text" id="mesh-peer-name" placeholder="peer-name" style="width:100%;padding:0.5rem 0.75rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font-size:0.9rem">
        </div>
        <div>
          <label style="font-size:0.8rem;color:var(--text-secondary);display:block;margin-bottom:0.25rem">Trust Level: <span id="mesh-trust-value">0.7</span></label>
          <input type="range" id="mesh-peer-trust" min="0" max="1" step="0.05" value="0.7" style="width:100%;accent-color:var(--accent-bright)">
        </div>
        <button id="mesh-add-peer-btn" style="padding:0.5rem 1rem;background:var(--accent);color:#fff;border:none;border-radius:var(--radius-sm);cursor:pointer;font-size:0.9rem;display:flex;align-items:center;justify-content:center;gap:0.5rem">
          <i class="fa-solid fa-plus"></i> Add Peer
        </button>
      </div>
    </div>`;

  html += `</div>`;

  // Peers table
  html += `
    <div class="card" style="margin-bottom:1rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-users"></i> Connected Peers</h3>
        <button id="mesh-refresh-btn" style="padding:0.35rem 0.75rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-secondary);cursor:pointer;font-size:0.8rem;display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-arrows-rotate"></i> Refresh
        </button>
      </div>
      <div id="mesh-peers-table">
        ${renderPeersTable(peers)}
      </div>
    </div>`;

  // Bottom row: Discovery + Sync Status
  html += `<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem">`;

  // Discovery card
  html += `
    <div class="card">
      <div class="card-header">
        <h3><i class="fa-solid fa-satellite-dish"></i> Discovery</h3>
      </div>
      <div style="display:flex;flex-direction:column;gap:0.75rem">
        <label style="font-size:0.85rem;color:var(--text-secondary)">Find peers by topic:</label>
        <div style="display:flex;gap:0.5rem">
          <input type="text" id="mesh-discover-topic" placeholder="topic..." style="flex:1;padding:0.5rem 0.75rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font-size:0.9rem">
          <button id="mesh-discover-btn" style="padding:0.5rem 0.75rem;background:var(--accent);color:#fff;border:none;border-radius:var(--radius-sm);cursor:pointer;font-size:0.9rem;display:flex;align-items:center;gap:0.4rem">
            <i class="fa-solid fa-search"></i> Discover
          </button>
        </div>
        <div id="mesh-discover-results" style="font-size:0.9rem;color:var(--text-secondary)"></div>
      </div>
    </div>`;

  // Sync Status card
  html += `
    <div class="card">
      <div class="card-header">
        <h3><i class="fa-solid fa-rotate"></i> Sync Status</h3>
      </div>
      <div id="mesh-sync-status" style="display:flex;flex-direction:column;gap:0.75rem">
        <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
          <i class="fa-solid fa-clock" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
          <span style="font-size:0.85rem;color:var(--text-secondary);min-width:80px">Last sync</span>
          <span style="font-size:0.9rem" id="mesh-last-sync">--</span>
        </div>
        <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
          <i class="fa-solid fa-list-check" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
          <span style="font-size:0.85rem;color:var(--text-secondary);min-width:80px">Pending</span>
          <span style="font-size:0.9rem" id="mesh-pending">0 deltas</span>
        </div>
        <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
          <i class="fa-solid fa-triangle-exclamation" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
          <span style="font-size:0.85rem;color:var(--text-secondary);min-width:80px">Conflicts</span>
          <span style="font-size:0.9rem" id="mesh-conflicts">0</span>
        </div>
        <button id="mesh-audit-btn" style="padding:0.5rem 1rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-secondary);cursor:pointer;font-size:0.9rem;display:flex;align-items:center;justify-content:center;gap:0.5rem">
          <i class="fa-solid fa-scroll"></i> View Audit Log
        </button>
      </div>
    </div>`;

  html += `</div>`;

  // Audit log modal area (hidden by default)
  html += `<div id="mesh-audit-panel" style="display:none;margin-top:1rem"></div>`;

  return html;
}

function renderPeersTable(peers) {
  if (!peers || peers.length === 0) {
    return emptyStateHTML('fa-users-slash', 'No peers connected yet. Add a peer above to start syncing knowledge.');
  }

  let rows = '';
  for (const p of peers) {
    const name = escapeHtml(p.name || 'unnamed');
    const endpoint = escapeHtml(p.endpoint || '--');
    const trust = p.trust != null ? p.trust : 0;
    const trustPct = Math.round(trust * 100);
    const status = p.status || (p.online ? 'active' : 'inactive');
    const statusColor = (status === 'active' || p.online) ? 'var(--success)' : 'var(--text-muted)';
    const peerKey = escapeHtml(p.public_key || p.key || p.id || p.endpoint || p.name || '');

    rows += `
      <tr>
        <td style="font-size:0.85rem;font-weight:500">${name}</td>
        <td style="font-size:0.85rem;font-family:var(--font-mono);color:var(--text-secondary)">${endpoint}</td>
        <td style="min-width:100px">
          <div style="display:flex;align-items:center;gap:0.5rem">
            <div style="flex:1;height:6px;background:var(--bg-input);border-radius:3px;overflow:hidden">
              <div style="width:${trustPct}%;height:100%;background:${confidenceColor(trust)};border-radius:3px"></div>
            </div>
            <span style="font-size:0.8rem;color:var(--text-muted);min-width:30px">${trustPct}%</span>
          </div>
        </td>
        <td>
          <span style="color:${statusColor};font-size:0.85rem">
            <i class="fa-solid fa-circle" style="font-size:0.4rem;vertical-align:middle;margin-right:0.25rem"></i>${escapeHtml(status)}
          </span>
        </td>
        <td>
          <button class="mesh-remove-peer" data-key="${peerKey}" style="padding:0.25rem 0.5rem;background:none;border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-muted);cursor:pointer;font-size:0.8rem;display:flex;align-items:center;gap:0.3rem" title="Remove peer">
            <i class="fa-solid fa-trash-can"></i> Remove
          </button>
        </td>
      </tr>`;
  }

  return `
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Name</th>
            <th>Endpoint</th>
            <th>Trust</th>
            <th>Status</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

// ── Event Bindings ──

function bindMeshEvents(peers) {
  // Trust slider live value
  const trustSlider = document.getElementById('mesh-peer-trust');
  const trustValue = document.getElementById('mesh-trust-value');
  if (trustSlider && trustValue) {
    trustSlider.addEventListener('input', () => {
      trustValue.textContent = trustSlider.value;
    });
  }

  // Add peer
  const addBtn = document.getElementById('mesh-add-peer-btn');
  if (addBtn) {
    addBtn.addEventListener('click', async () => {
      const endpoint = document.getElementById('mesh-peer-endpoint').value.trim();
      const name = document.getElementById('mesh-peer-name').value.trim();
      const trust = parseFloat(document.getElementById('mesh-peer-trust').value);

      if (!endpoint) {
        showToast('Endpoint is required', 'error');
        return;
      }

      addBtn.disabled = true;
      addBtn.innerHTML = '<span class="spinner"></span> Adding...';

      try {
        await engram._post('/mesh/peers', { endpoint, name, trust });
        showToast('Peer added successfully', 'success');
        await refreshPeers();
        // Clear form
        document.getElementById('mesh-peer-endpoint').value = '';
        document.getElementById('mesh-peer-name').value = '';
        document.getElementById('mesh-peer-trust').value = '0.7';
        if (trustValue) trustValue.textContent = '0.7';
      } catch (err) {
        showToast('Failed to add peer: ' + err.message, 'error');
      } finally {
        addBtn.disabled = false;
        addBtn.innerHTML = '<i class="fa-solid fa-plus"></i> Add Peer';
      }
    });
  }

  // Refresh
  const refreshBtn = document.getElementById('mesh-refresh-btn');
  if (refreshBtn) {
    refreshBtn.addEventListener('click', async () => {
      refreshBtn.disabled = true;
      refreshBtn.innerHTML = '<span class="spinner"></span>';
      await refreshPeers();
      refreshBtn.disabled = false;
      refreshBtn.innerHTML = '<i class="fa-solid fa-arrows-rotate"></i> Refresh';
    });
  }

  // Remove peer buttons
  bindRemoveButtons();

  // Discover
  const discoverBtn = document.getElementById('mesh-discover-btn');
  if (discoverBtn) {
    discoverBtn.addEventListener('click', async () => {
      const topic = document.getElementById('mesh-discover-topic').value.trim();
      if (!topic) {
        showToast('Enter a topic to discover peers', 'error');
        return;
      }

      const resultsDiv = document.getElementById('mesh-discover-results');
      resultsDiv.innerHTML = loadingHTML('Searching...');

      try {
        const results = await engram.meshDiscover(topic);
        const items = Array.isArray(results) ? results : (results && results.peers ? results.peers : []);
        if (items.length === 0) {
          resultsDiv.innerHTML = '<div style="color:var(--text-muted);padding:0.5rem 0"><i class="fa-solid fa-circle-info"></i> No peers found for this topic.</div>';
        } else {
          let listHtml = '<div style="display:flex;flex-direction:column;gap:0.5rem;margin-top:0.25rem">';
          for (const item of items) {
            const pName = escapeHtml(item.name || item.endpoint || item.id || 'Unknown');
            const pEndpoint = item.endpoint ? escapeHtml(item.endpoint) : '';
            listHtml += `
              <div style="display:flex;align-items:center;justify-content:space-between;padding:0.5rem;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm)">
                <div>
                  <div style="font-weight:500;font-size:0.85rem">${pName}</div>
                  ${pEndpoint ? '<div style="font-size:0.8rem;color:var(--text-muted);font-family:var(--font-mono)">' + pEndpoint + '</div>' : ''}
                </div>
                <i class="fa-solid fa-circle-check" style="color:var(--success)"></i>
              </div>`;
          }
          listHtml += '</div>';
          resultsDiv.innerHTML = listHtml;
        }
      } catch (err) {
        resultsDiv.innerHTML = '<div style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ' + escapeHtml(err.message) + '</div>';
      }
    });
  }

  // Audit log
  const auditBtn = document.getElementById('mesh-audit-btn');
  if (auditBtn) {
    auditBtn.addEventListener('click', async () => {
      const panel = document.getElementById('mesh-audit-panel');
      if (panel.style.display !== 'none') {
        panel.style.display = 'none';
        return;
      }

      panel.innerHTML = `<div class="card">${loadingHTML('Loading audit log...')}</div>`;
      panel.style.display = 'block';

      try {
        const audit = await engram._fetch('/mesh/audit');
        const entries = Array.isArray(audit) ? audit : (audit && audit.entries ? audit.entries : []);
        if (entries.length === 0) {
          panel.innerHTML = `<div class="card">${emptyStateHTML('fa-scroll', 'No audit entries yet.')}</div>`;
        } else {
          let tableHtml = `
            <div class="card">
              <div class="card-header">
                <h3><i class="fa-solid fa-scroll"></i> Audit Log</h3>
                <button id="mesh-audit-close" style="padding:0.25rem 0.5rem;background:none;border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-muted);cursor:pointer;font-size:0.8rem">
                  <i class="fa-solid fa-xmark"></i> Close
                </button>
              </div>
              <div class="table-wrap">
                <table>
                  <thead><tr><th>Time</th><th>Event</th><th>Peer</th><th>Details</th></tr></thead>
                  <tbody>`;
          for (const entry of entries.slice(-50).reverse()) {
            const time = entry.timestamp || entry.time || '--';
            const event = escapeHtml(entry.event || entry.action || entry.type || '--');
            const peer = escapeHtml(entry.peer || entry.source || '--');
            const details = escapeHtml(entry.details || entry.message || entry.description || '--');
            tableHtml += `<tr><td style="font-size:0.8rem;white-space:nowrap">${escapeHtml(String(time))}</td><td style="font-size:0.85rem">${event}</td><td style="font-size:0.85rem;font-family:var(--font-mono)">${peer}</td><td style="font-size:0.85rem;color:var(--text-secondary)">${details}</td></tr>`;
          }
          tableHtml += '</tbody></table></div></div>';
          panel.innerHTML = tableHtml;

          document.getElementById('mesh-audit-close').addEventListener('click', () => {
            panel.style.display = 'none';
          });
        }
      } catch (err) {
        panel.innerHTML = `<div class="card"><div style="color:var(--error);padding:1rem"><i class="fa-solid fa-circle-exclamation"></i> Failed to load audit log: ${escapeHtml(err.message)}</div></div>`;
      }
    });
  }

  // Load sync status
  loadSyncStatus();
}

async function refreshPeers() {
  try {
    const result = await engram.meshPeers();
    const peers = Array.isArray(result) ? result : (result && result.peers ? result.peers : []);
    const tableDiv = document.getElementById('mesh-peers-table');
    if (tableDiv) {
      tableDiv.innerHTML = renderPeersTable(peers);
      bindRemoveButtons();
    }
  } catch (err) {
    showToast('Failed to refresh peers: ' + err.message, 'error');
  }
}

function bindRemoveButtons() {
  document.querySelectorAll('.mesh-remove-peer').forEach(btn => {
    btn.addEventListener('click', async () => {
      const key = btn.getAttribute('data-key');
      if (!key) return;

      btn.disabled = true;
      btn.innerHTML = '<span class="spinner"></span>';

      try {
        await engram._fetch(`/mesh/peers/${encodeURIComponent(key)}`, { method: 'DELETE' });
        showToast('Peer removed', 'success');
        await refreshPeers();
      } catch (err) {
        showToast('Failed to remove peer: ' + err.message, 'error');
        btn.disabled = false;
        btn.innerHTML = '<i class="fa-solid fa-trash-can"></i> Remove';
      }
    });
  });
}

async function loadSyncStatus() {
  // Try to get sync info from audit log or heartbeat
  try {
    const audit = await engram._fetch('/mesh/audit');
    const entries = Array.isArray(audit) ? audit : (audit && audit.entries ? audit.entries : []);
    if (entries.length > 0) {
      const last = entries[entries.length - 1];
      const lastTime = last.timestamp || last.time;
      if (lastTime) {
        const el = document.getElementById('mesh-last-sync');
        if (el) {
          const ago = timeAgo(lastTime);
          el.textContent = ago;
        }
      }

      // Count pending/conflicts from entries
      let conflicts = 0;
      for (const e of entries) {
        if (e.event === 'conflict' || e.type === 'conflict') conflicts++;
      }
      const conflictEl = document.getElementById('mesh-conflicts');
      if (conflictEl) conflictEl.textContent = String(conflicts);
    }
  } catch (_) {
    // Audit endpoint may not have data yet
  }
}

function timeAgo(timestamp) {
  try {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now - date;
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 60) return diffSec + 's ago';
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return diffMin + ' min ago';
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return diffHr + 'h ago';
    const diffDay = Math.floor(diffHr / 24);
    return diffDay + 'd ago';
  } catch (_) {
    return String(timestamp);
  }
}
