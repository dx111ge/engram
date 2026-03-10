/* ============================================
   engram - Manage Knowledge (CRUD Node Editor)
   ============================================ */

let addEditMode = false;
let addEditLabel = null;
let addSearchTimeout = null;
let addDeleteSearchTimeout = null;

router.register('/add', () => {
  addEditMode = false;
  addEditLabel = null;

  renderTo(`
    <div class="view-header">
      <h1><i class="fa-solid fa-pen-to-square"></i> Manage Knowledge</h1>
    </div>

    <!-- Search bar + New button -->
    <div style="display:flex;gap:0.75rem;margin-bottom:1.25rem;align-items:center">
      <div style="flex:1;position:relative">
        <i class="fa-solid fa-magnifying-glass" style="position:absolute;left:0.9rem;top:50%;transform:translateY(-50%);color:var(--text-muted)"></i>
        <input type="text" id="add-search-input" placeholder="Search existing fact to edit..." style="padding-left:2.75rem;width:100%">
        <div id="add-search-dropdown" style="display:none;position:absolute;top:100%;left:0;right:0;z-index:100;background:var(--bg-card);border:1px solid var(--border);border-top:none;border-radius:0 0 var(--radius-sm) var(--radius-sm);max-height:260px;overflow-y:auto;box-shadow:0 4px 12px var(--shadow)"></div>
      </div>
      <button class="btn btn-primary" id="btn-new-fact" title="New fact">
        <i class="fa-solid fa-plus"></i> New
      </button>
    </div>

    <div style="display:grid;grid-template-columns:1fr 380px;gap:1rem">

      <!-- Left: Create / Edit Fact -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-brain"></i> <span id="add-form-title">Create Fact</span></h3>
          <span id="add-edit-badge" style="display:none;font-size:0.75rem;padding:0.2rem 0.5rem;border-radius:999px;background:rgba(74,158,255,0.2);color:var(--accent-bright);border:1px solid rgba(74,158,255,0.3);font-weight:600;text-transform:uppercase;letter-spacing:0.03em">
            <i class="fa-solid fa-pencil"></i> Editing
          </span>
        </div>

        <div class="form-group">
          <label>Label *</label>
          <input type="text" id="add-entity" placeholder="e.g. Rust, Albert Einstein, Machine Learning">
        </div>
        <div class="form-group">
          <label>Type</label>
          <input type="text" id="add-type" placeholder="person, place, concept, technology...">
        </div>

        <div class="form-group">
          <label>Properties</label>
          <div class="kv-pairs" id="add-props">
            <div class="kv-row">
              <input type="text" placeholder="key">
              <input type="text" placeholder="value">
              <button class="btn-icon add-remove-kv" title="Remove"><i class="fa-solid fa-xmark"></i></button>
            </div>
          </div>
          <button class="btn btn-sm btn-secondary mt-1" id="add-prop-btn">
            <i class="fa-solid fa-plus"></i> Add Property
          </button>
        </div>

        <div class="form-group">
          <label>Source / Author</label>
          <input type="text" id="add-source" placeholder="where you learned this">
        </div>

        <div class="form-group">
          <label>Strength</label>
          <div class="slider-group">
            <label><span>Confidence level</span><span id="add-strength-val">95%</span></label>
            <input type="range" id="add-strength" min="0" max="100" value="95">
          </div>
        </div>

        <div style="display:flex;gap:0.5rem">
          <button class="btn btn-success" id="btn-create-fact" style="flex:1">
            <i class="fa-solid fa-check"></i> <span id="btn-create-label">Create</span>
          </button>
          <button class="btn btn-secondary" id="btn-reset-form">
            <i class="fa-solid fa-rotate-left"></i> Reset
          </button>
        </div>
        <div id="add-result" class="mt-1"></div>
      </div>

      <!-- Right column -->
      <div style="display:flex;flex-direction:column;gap:1rem">

        <!-- Connect Facts -->
        <div class="card">
          <div class="card-header">
            <h3><i class="fa-solid fa-link"></i> Connect Facts</h3>
          </div>
          <div class="form-group">
            <label>From *</label>
            <input type="text" id="conn-from" placeholder="e.g. Albert Einstein">
          </div>
          <div class="form-group">
            <label>Relationship *</label>
            <input type="text" id="conn-rel" placeholder="works at, is part of...">
          </div>
          <div class="form-group">
            <label>To *</label>
            <input type="text" id="conn-to" placeholder="e.g. Princeton University">
          </div>
          <div class="form-group">
            <label>Strength</label>
            <div class="slider-group">
              <label><span>Connection strength</span><span id="conn-strength-val">90%</span></label>
              <input type="range" id="conn-strength" min="0" max="100" value="90">
            </div>
          </div>
          <button class="btn btn-success w-100" id="btn-connect">
            <i class="fa-solid fa-link"></i> Connect
          </button>
          <div id="conn-result" class="mt-1"></div>
        </div>

        <!-- Delete Fact -->
        <div class="card">
          <div class="card-header">
            <h3><i class="fa-solid fa-trash-can"></i> Delete Fact</h3>
          </div>
          <p class="text-secondary" style="font-size:0.85rem;margin-bottom:0.75rem">
            Search for a fact, then confirm deletion.
          </p>
          <div style="display:flex;gap:0.5rem;align-items:flex-start">
            <div style="flex:1;position:relative">
              <input type="text" id="del-search-input" placeholder="Search fact to delete...">
              <div id="del-search-dropdown" style="display:none;position:absolute;top:100%;left:0;right:0;z-index:100;background:var(--bg-card);border:1px solid var(--border);border-top:none;border-radius:0 0 var(--radius-sm) var(--radius-sm);max-height:200px;overflow-y:auto;box-shadow:0 4px 12px var(--shadow)"></div>
            </div>
            <button class="btn btn-danger" id="btn-delete-fact" disabled>
              <i class="fa-solid fa-trash-can"></i> Delete
            </button>
          </div>
          <div id="del-selected" class="mt-1" style="font-size:0.85rem"></div>
          <div id="del-result" class="mt-1"></div>
        </div>

      </div>
    </div>
  `);

  setupAddEvents();
});

function setupAddEvents() {
  // --- Strength sliders ---
  document.getElementById('add-strength').addEventListener('input', (e) => {
    document.getElementById('add-strength-val').textContent = e.target.value + '%';
  });
  document.getElementById('conn-strength').addEventListener('input', (e) => {
    document.getElementById('conn-strength-val').textContent = e.target.value + '%';
  });

  // --- Property key-value pairs ---
  document.getElementById('add-prop-btn').addEventListener('click', () => {
    addAddKVRow('add-props');
  });
  document.getElementById('add-props').addEventListener('click', (e) => {
    const removeBtn = e.target.closest('.add-remove-kv');
    if (removeBtn) {
      const container = document.getElementById('add-props');
      removeBtn.closest('.kv-row').remove();
      if (container.children.length === 0) addAddKVRow('add-props');
    }
  });

  // --- Top search bar (edit existing) ---
  const searchInput = document.getElementById('add-search-input');
  const searchDropdown = document.getElementById('add-search-dropdown');

  searchInput.addEventListener('input', () => {
    clearTimeout(addSearchTimeout);
    const q = searchInput.value.trim();
    if (!q) { searchDropdown.style.display = 'none'; return; }
    addSearchTimeout = setTimeout(async () => {
      try {
        const results = await engram.search({ query: q });
        const items = results.results || results.entities || results || [];
        if (!Array.isArray(items) || items.length === 0) {
          searchDropdown.innerHTML = '<div style="padding:0.75rem;color:var(--text-muted);font-size:0.85rem">No results found</div>';
          searchDropdown.style.display = 'block';
          return;
        }
        searchDropdown.innerHTML = items.slice(0, 15).map(item => {
          const label = item.label || item.entity || item;
          const type = item.node_type || item.type || '';
          const conf = item.confidence != null ? Math.round(item.confidence * 100) + '%' : '';
          return `<div class="add-search-item" data-label="${escapeHtml(typeof label === 'string' ? label : String(label))}" style="padding:0.5rem 0.75rem;cursor:pointer;display:flex;align-items:center;justify-content:space-between;gap:0.5rem;border-bottom:1px solid var(--border);transition:background 0.15s">
            <div>
              <span style="font-weight:500">${escapeHtml(typeof label === 'string' ? label : String(label))}</span>
              ${type ? '<span style="font-size:0.75rem;color:var(--text-muted);margin-left:0.4rem;text-transform:uppercase">' + escapeHtml(type) + '</span>' : ''}
            </div>
            ${conf ? '<span style="font-size:0.75rem;color:var(--text-secondary)">' + conf + '</span>' : ''}
          </div>`;
        }).join('');
        searchDropdown.style.display = 'block';
      } catch (err) {
        searchDropdown.innerHTML = '<div style="padding:0.75rem;color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ' + escapeHtml(err.message) + '</div>';
        searchDropdown.style.display = 'block';
      }
    }, 300);
  });

  searchDropdown.addEventListener('click', (e) => {
    const item = e.target.closest('.add-search-item');
    if (item) {
      const label = item.getAttribute('data-label');
      searchDropdown.style.display = 'none';
      searchInput.value = label;
      loadNodeForEdit(label);
    }
  });

  // Close dropdown on outside click
  document.addEventListener('click', (e) => {
    if (!e.target.closest('#add-search-input') && !e.target.closest('#add-search-dropdown')) {
      searchDropdown.style.display = 'none';
    }
    if (!e.target.closest('#del-search-input') && !e.target.closest('#del-search-dropdown')) {
      const dd = document.getElementById('del-search-dropdown');
      if (dd) dd.style.display = 'none';
    }
  });

  // Hover effect for search items (delegated)
  searchDropdown.addEventListener('mouseover', (e) => {
    const item = e.target.closest('.add-search-item');
    if (item) item.style.background = 'var(--bg-hover)';
  });
  searchDropdown.addEventListener('mouseout', (e) => {
    const item = e.target.closest('.add-search-item');
    if (item) item.style.background = '';
  });

  // --- New button ---
  document.getElementById('btn-new-fact').addEventListener('click', () => {
    resetAddForm();
  });

  // --- Reset button ---
  document.getElementById('btn-reset-form').addEventListener('click', () => {
    if (addEditMode) {
      // Re-load the node data
      loadNodeForEdit(addEditLabel);
    } else {
      resetAddForm();
    }
  });

  // --- Create / Save button ---
  document.getElementById('btn-create-fact').addEventListener('click', async () => {
    const entity = document.getElementById('add-entity').value.trim();
    if (!entity) { showToast('Please enter a label', 'error'); return; }

    const type = document.getElementById('add-type').value.trim() || undefined;
    const source = document.getElementById('add-source').value.trim() || undefined;
    const confidence = parseInt(document.getElementById('add-strength').value) / 100;
    const properties = getAddKVPairs('add-props');

    const btn = document.getElementById('btn-create-fact');
    btn.disabled = true;

    try {
      const result = await engram.store({
        entity, type, source, confidence,
        properties: Object.keys(properties).length > 0 ? properties : undefined,
      });

      const action = addEditMode ? 'Updated' : 'Created';
      document.getElementById('add-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> ${action}: ${escapeHtml(result.label || entity)}</div>`;
      showToast('Fact ' + action.toLowerCase(), 'success');

      if (!addEditMode) {
        document.getElementById('add-entity').value = '';
      }
    } catch (err) {
      showToast('Could not save: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- Connect button ---
  document.getElementById('btn-connect').addEventListener('click', async () => {
    const from = document.getElementById('conn-from').value.trim();
    const relationship = document.getElementById('conn-rel').value.trim();
    const to = document.getElementById('conn-to').value.trim();
    if (!from || !relationship || !to) {
      showToast('Please fill in all three fields', 'error');
      return;
    }

    const confidence = parseInt(document.getElementById('conn-strength').value) / 100;
    const btn = document.getElementById('btn-connect');
    btn.disabled = true;

    try {
      await engram.relate({ from, to, relationship, confidence });
      document.getElementById('conn-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Connected: ${escapeHtml(from)} &rarr; ${escapeHtml(relationship)} &rarr; ${escapeHtml(to)}</div>`;
      showToast('Facts connected', 'success');
    } catch (err) {
      showToast('Could not connect: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- Delete search ---
  const delInput = document.getElementById('del-search-input');
  const delDropdown = document.getElementById('del-search-dropdown');
  const delBtn = document.getElementById('btn-delete-fact');
  let delSelectedLabel = null;

  delInput.addEventListener('input', () => {
    clearTimeout(addDeleteSearchTimeout);
    delSelectedLabel = null;
    delBtn.disabled = true;
    document.getElementById('del-selected').innerHTML = '';
    const q = delInput.value.trim();
    if (!q) { delDropdown.style.display = 'none'; return; }
    addDeleteSearchTimeout = setTimeout(async () => {
      try {
        const results = await engram.search({ query: q });
        const items = results.results || results.entities || results || [];
        if (!Array.isArray(items) || items.length === 0) {
          delDropdown.innerHTML = '<div style="padding:0.75rem;color:var(--text-muted);font-size:0.85rem">No results found</div>';
          delDropdown.style.display = 'block';
          return;
        }
        delDropdown.innerHTML = items.slice(0, 10).map(item => {
          const label = item.label || item.entity || item;
          return `<div class="del-search-item" data-label="${escapeHtml(typeof label === 'string' ? label : String(label))}" style="padding:0.5rem 0.75rem;cursor:pointer;border-bottom:1px solid var(--border);transition:background 0.15s;font-size:0.9rem">
            <i class="fa-solid fa-circle-dot" style="color:var(--error);margin-right:0.4rem;font-size:0.7rem"></i>
            ${escapeHtml(typeof label === 'string' ? label : String(label))}
          </div>`;
        }).join('');
        delDropdown.style.display = 'block';
      } catch (err) {
        delDropdown.innerHTML = '<div style="padding:0.75rem;color:var(--error);font-size:0.85rem">' + escapeHtml(err.message) + '</div>';
        delDropdown.style.display = 'block';
      }
    }, 300);
  });

  delDropdown.addEventListener('click', (e) => {
    const item = e.target.closest('.del-search-item');
    if (item) {
      delSelectedLabel = item.getAttribute('data-label');
      delInput.value = delSelectedLabel;
      delDropdown.style.display = 'none';
      delBtn.disabled = false;
      document.getElementById('del-selected').innerHTML =
        `<span style="color:var(--warning)"><i class="fa-solid fa-triangle-exclamation"></i> Ready to delete: <strong>${escapeHtml(delSelectedLabel)}</strong></span>`;
    }
  });

  delDropdown.addEventListener('mouseover', (e) => {
    const item = e.target.closest('.del-search-item');
    if (item) item.style.background = 'var(--bg-hover)';
  });
  delDropdown.addEventListener('mouseout', (e) => {
    const item = e.target.closest('.del-search-item');
    if (item) item.style.background = '';
  });

  delBtn.addEventListener('click', async () => {
    if (!delSelectedLabel) return;

    const confirmed = confirm('Are you sure you want to delete "' + delSelectedLabel + '"? This cannot be undone.');
    if (!confirmed) return;

    delBtn.disabled = true;
    const resultDiv = document.getElementById('del-result');

    try {
      await engram.deleteNode(delSelectedLabel);
      resultDiv.innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Deleted: ${escapeHtml(delSelectedLabel)}</div>`;
      showToast('Fact deleted: ' + delSelectedLabel, 'success');
      delInput.value = '';
      document.getElementById('del-selected').innerHTML = '';
      delSelectedLabel = null;

      // If we were editing the deleted node, reset the form
      if (addEditMode && addEditLabel === delSelectedLabel) {
        resetAddForm();
      }
    } catch (err) {
      resultDiv.innerHTML =
        `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Could not delete: ' + err.message, 'error');
      delBtn.disabled = false;
    }
  });
}

async function loadNodeForEdit(label) {
  const resultDiv = document.getElementById('add-result');
  resultDiv.innerHTML = loadingHTML('Loading node...');

  try {
    const node = await engram.getNode(label);

    // Switch to edit mode
    addEditMode = true;
    addEditLabel = label;

    document.getElementById('add-form-title').textContent = 'Edit Fact';
    document.getElementById('add-edit-badge').style.display = '';
    document.getElementById('btn-create-label').textContent = 'Save Changes';

    // Populate fields
    document.getElementById('add-entity').value = node.label || label;
    document.getElementById('add-type').value = node.node_type || node.type || '';
    document.getElementById('add-source').value = node.source || '';

    const conf = node.confidence != null ? Math.round(node.confidence * 100) : 95;
    document.getElementById('add-strength').value = conf;
    document.getElementById('add-strength-val').textContent = conf + '%';

    // Populate properties
    const propsContainer = document.getElementById('add-props');
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
          <button class="btn-icon add-remove-kv" title="Remove"><i class="fa-solid fa-xmark"></i></button>
        `;
        propsContainer.appendChild(row);
      });
    } else {
      addAddKVRow('add-props');
    }

    resultDiv.innerHTML =
      `<div style="color:var(--accent-bright);font-size:0.85rem"><i class="fa-solid fa-pen-to-square"></i> Loaded for editing: ${escapeHtml(label)}</div>`;
  } catch (err) {
    resultDiv.innerHTML =
      `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> Could not load: ${escapeHtml(err.message)}</div>`;
    showToast('Could not load node: ' + err.message, 'error');
  }
}

function resetAddForm() {
  addEditMode = false;
  addEditLabel = null;

  document.getElementById('add-form-title').textContent = 'Create Fact';
  document.getElementById('add-edit-badge').style.display = 'none';
  document.getElementById('btn-create-label').textContent = 'Create';

  document.getElementById('add-entity').value = '';
  document.getElementById('add-type').value = '';
  document.getElementById('add-source').value = '';
  document.getElementById('add-strength').value = 95;
  document.getElementById('add-strength-val').textContent = '95%';
  document.getElementById('add-search-input').value = '';
  document.getElementById('add-result').innerHTML = '';

  const propsContainer = document.getElementById('add-props');
  propsContainer.innerHTML = '';
  addAddKVRow('add-props');
}

function addAddKVRow(containerId) {
  const container = document.getElementById(containerId);
  const row = document.createElement('div');
  row.className = 'kv-row';
  row.innerHTML = `
    <input type="text" placeholder="key">
    <input type="text" placeholder="value">
    <button class="btn-icon add-remove-kv" title="Remove"><i class="fa-solid fa-xmark"></i></button>
  `;
  container.appendChild(row);
}

function getAddKVPairs(containerId) {
  const container = document.getElementById(containerId);
  const props = {};
  container.querySelectorAll('.kv-row').forEach(row => {
    const inputs = row.querySelectorAll('input');
    const k = inputs[0].value.trim();
    const v = inputs[1].value.trim();
    if (k && v) props[k] = v;
  });
  return props;
}
