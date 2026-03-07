/* ============================================
   engram - Import View
   ============================================ */

router.register('/import', () => {
  renderTo(`
    <div class="view-header">
      <h1><i class="fa-solid fa-file-import"></i> Import</h1>
    </div>
    <div class="import-sections">
      <!-- Quick Store -->
      <div class="card">
        <div class="card-header"><h3><i class="fa-solid fa-plus-circle"></i> Quick Store</h3></div>
        <div class="form-group">
          <label>Entity Name *</label>
          <input type="text" id="store-entity" placeholder="e.g. PostgreSQL">
        </div>
        <div class="form-group">
          <label>Type (optional)</label>
          <input type="text" id="store-type" placeholder="e.g. database, person, concept">
        </div>
        <div class="form-group">
          <label>Source (optional)</label>
          <input type="text" id="store-source" placeholder="e.g. documentation">
        </div>
        <div class="form-group">
          <label>Confidence</label>
          <div class="slider-group">
            <label><span></span><span id="store-conf-val">0.95</span></label>
            <input type="range" id="store-confidence" min="0" max="100" value="95">
          </div>
        </div>
        <div class="form-group">
          <label>Properties</label>
          <div class="kv-pairs" id="store-props">
            <div class="kv-row">
              <input type="text" placeholder="key">
              <input type="text" placeholder="value">
              <button class="btn-icon" onclick="removeKVRow(this)" title="Remove"><i class="fa-solid fa-xmark"></i></button>
            </div>
          </div>
          <button class="btn btn-sm btn-secondary mt-1" onclick="addKVRow('store-props')">
            <i class="fa-solid fa-plus"></i> Add Property
          </button>
        </div>
        <button class="btn btn-primary w-100" id="btn-store">
          <i class="fa-solid fa-floppy-disk"></i> Store Entity
        </button>
        <div id="store-result" class="mt-1"></div>
      </div>

      <!-- Relationship Builder -->
      <div class="card">
        <div class="card-header"><h3><i class="fa-solid fa-link"></i> Create Relationship</h3></div>
        <div class="form-group">
          <label>From Entity *</label>
          <input type="text" id="rel-from" placeholder="e.g. PostgreSQL">
        </div>
        <div class="form-group">
          <label>Relationship *</label>
          <input type="text" id="rel-type" placeholder="e.g. is_a, uses, created_by">
        </div>
        <div class="form-group">
          <label>To Entity *</label>
          <input type="text" id="rel-to" placeholder="e.g. database">
        </div>
        <div class="form-group">
          <label>Confidence</label>
          <div class="slider-group">
            <label><span></span><span id="rel-conf-val">0.90</span></label>
            <input type="range" id="rel-confidence" min="0" max="100" value="90">
          </div>
        </div>
        <button class="btn btn-primary w-100" id="btn-relate">
          <i class="fa-solid fa-link"></i> Create Relationship
        </button>
        <div id="rel-result" class="mt-1"></div>
      </div>

      <!-- Bulk Import -->
      <div class="card">
        <div class="card-header"><h3><i class="fa-solid fa-layer-group"></i> Bulk Import</h3></div>
        <div class="form-group">
          <label>JSON Array of Entities</label>
          <textarea id="bulk-json" rows="8" placeholder='[
  {"entity": "Node.js", "type": "runtime"},
  {"entity": "Express", "type": "framework"}
]'></textarea>
        </div>
        <button class="btn btn-primary w-100" id="btn-bulk-import">
          <i class="fa-solid fa-upload"></i> Import All
        </button>
        <div id="bulk-result" class="mt-1"></div>
      </div>

      <!-- File Import -->
      <div class="card">
        <div class="card-header"><h3><i class="fa-solid fa-file-lines"></i> File Import</h3></div>
        <div class="dropzone" id="file-dropzone">
          <i class="fa-solid fa-cloud-arrow-up"></i>
          <p>Drop a text file here or click to browse</p>
          <p class="text-muted" style="font-size:0.8rem">Extracts capitalized words and quoted phrases as entities</p>
          <input type="file" id="file-input" accept=".txt,.csv,.json" style="display:none">
        </div>
        <div id="file-entities" class="mt-1" style="max-height:200px;overflow-y:auto"></div>
        <button class="btn btn-primary w-100 mt-1" id="btn-file-import" style="display:none">
          <i class="fa-solid fa-upload"></i> Import Extracted Entities
        </button>
        <div id="file-result" class="mt-1"></div>
      </div>
    </div>
  `);

  setupImportEvents();
});

function addKVRow(containerId) {
  const container = document.getElementById(containerId);
  const row = document.createElement('div');
  row.className = 'kv-row';
  row.innerHTML = `
    <input type="text" placeholder="key">
    <input type="text" placeholder="value">
    <button class="btn-icon" onclick="removeKVRow(this)" title="Remove"><i class="fa-solid fa-xmark"></i></button>
  `;
  container.appendChild(row);
}

function removeKVRow(btn) {
  const container = btn.closest('.kv-pairs');
  btn.closest('.kv-row').remove();
  if (container.children.length === 0) addKVRow(container.id);
}

function getKVPairs(containerId) {
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

function setupImportEvents() {
  // Confidence sliders
  document.getElementById('store-confidence').addEventListener('input', (e) => {
    document.getElementById('store-conf-val').textContent = (e.target.value / 100).toFixed(2);
  });
  document.getElementById('rel-confidence').addEventListener('input', (e) => {
    document.getElementById('rel-conf-val').textContent = (e.target.value / 100).toFixed(2);
  });

  // Store entity
  document.getElementById('btn-store').addEventListener('click', async () => {
    const entity = document.getElementById('store-entity').value.trim();
    if (!entity) { showToast('Entity name is required', 'error'); return; }

    const type = document.getElementById('store-type').value.trim() || undefined;
    const source = document.getElementById('store-source').value.trim() || undefined;
    const confidence = parseInt(document.getElementById('store-confidence').value) / 100;
    const properties = getKVPairs('store-props');

    try {
      const result = await engram.store({ entity, type, source, confidence, properties: Object.keys(properties).length > 0 ? properties : undefined });
      document.getElementById('store-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Stored node #${result.node_id}: ${escapeHtml(result.label)}</div>`;
      showToast(`Stored "${entity}"`, 'success');
      document.getElementById('store-entity').value = '';
    } catch (err) {
      showToast(`Store failed: ${err.message}`, 'error');
    }
  });

  // Create relationship
  document.getElementById('btn-relate').addEventListener('click', async () => {
    const from = document.getElementById('rel-from').value.trim();
    const to = document.getElementById('rel-to').value.trim();
    const relationship = document.getElementById('rel-type').value.trim();
    if (!from || !to || !relationship) { showToast('All relationship fields are required', 'error'); return; }

    const confidence = parseInt(document.getElementById('rel-confidence').value) / 100;

    try {
      const result = await engram.relate({ from, to, relationship, confidence });
      document.getElementById('rel-result').innerHTML =
        `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Created: ${escapeHtml(from)} -[${escapeHtml(relationship)}]-> ${escapeHtml(to)}</div>`;
      showToast('Relationship created', 'success');
    } catch (err) {
      showToast(`Relate failed: ${err.message}`, 'error');
    }
  });

  // Bulk import
  document.getElementById('btn-bulk-import').addEventListener('click', async () => {
    const textarea = document.getElementById('bulk-json');
    let entities;
    try {
      entities = JSON.parse(textarea.value.trim());
      if (!Array.isArray(entities)) throw new Error('Must be a JSON array');
    } catch (err) {
      showToast(`Invalid JSON: ${err.message}`, 'error');
      return;
    }

    const resultDiv = document.getElementById('bulk-result');
    resultDiv.innerHTML = `<span class="spinner"></span> Importing ${entities.length} entities...`;
    let success = 0, failed = 0;

    for (const ent of entities) {
      try {
        await engram.store(ent);
        success++;
      } catch (_) {
        failed++;
      }
    }

    resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem">
      <i class="fa-solid fa-check"></i> Imported ${success}/${entities.length} entities${failed > 0 ? ` (${failed} failed)` : ''}
    </div>`;
    showToast(`Bulk import: ${success} succeeded, ${failed} failed`, success > 0 ? 'success' : 'error');
  });

  // File import
  const dropzone = document.getElementById('file-dropzone');
  const fileInput = document.getElementById('file-input');

  dropzone.addEventListener('click', () => fileInput.click());
  dropzone.addEventListener('dragover', (e) => { e.preventDefault(); dropzone.classList.add('dragover'); });
  dropzone.addEventListener('dragleave', () => dropzone.classList.remove('dragover'));
  dropzone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropzone.classList.remove('dragover');
    if (e.dataTransfer.files.length > 0) processFile(e.dataTransfer.files[0]);
  });
  fileInput.addEventListener('change', () => {
    if (fileInput.files.length > 0) processFile(fileInput.files[0]);
  });
}

let extractedEntities = [];

function processFile(file) {
  const reader = new FileReader();
  reader.onload = (e) => {
    const text = e.target.result;
    // Extract entities: capitalized words (2+ chars), quoted phrases
    const capitalized = new Set();
    const capRegex = /\b([A-Z][a-zA-Z]{2,}(?:\s[A-Z][a-zA-Z]{2,})*)\b/g;
    const quotedRegex = /"([^"]{2,})"/g;

    let m;
    while ((m = capRegex.exec(text)) !== null) {
      // Skip common English words
      const skip = ['The', 'This', 'That', 'These', 'Those', 'What', 'Where', 'When', 'Why', 'How',
        'And', 'But', 'For', 'Not', 'All', 'Can', 'Had', 'Her', 'Was', 'One', 'Our', 'Out'];
      if (!skip.includes(m[1])) capitalized.add(m[1]);
    }
    while ((m = quotedRegex.exec(text)) !== null) {
      capitalized.add(m[1]);
    }

    extractedEntities = Array.from(capitalized);
    const container = document.getElementById('file-entities');

    if (extractedEntities.length === 0) {
      container.innerHTML = '<p class="text-muted">No entities extracted from file.</p>';
      document.getElementById('btn-file-import').style.display = 'none';
      return;
    }

    container.innerHTML = `
      <p class="text-secondary mb-1" style="font-size:0.85rem">${extractedEntities.length} entities extracted:</p>
      ${extractedEntities.map(e => `<span class="badge badge-active" style="margin:0.15rem">${escapeHtml(e)}</span>`).join('')}
    `;
    document.getElementById('btn-file-import').style.display = '';

    document.getElementById('btn-file-import').onclick = async () => {
      const resultDiv = document.getElementById('file-result');
      resultDiv.innerHTML = `<span class="spinner"></span> Importing ${extractedEntities.length} entities...`;
      let success = 0, failed = 0;

      for (const entity of extractedEntities) {
        try {
          await engram.store({ entity });
          success++;
        } catch (_) {
          failed++;
        }
      }

      resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem">
        <i class="fa-solid fa-check"></i> Imported ${success}/${extractedEntities.length}${failed > 0 ? ` (${failed} failed)` : ''}
      </div>`;
      showToast(`File import: ${success} succeeded`, 'success');
    };
  };
  reader.readAsText(file);
}
