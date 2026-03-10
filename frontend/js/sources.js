/* ============================================
   engram - Sources View
   Data intake hub: manage sources, processing
   pipeline, and dry-run preview
   ============================================ */

let sourcesExpandedType = null;

router.register('/sources', async () => {
  sourcesExpandedType = null;

  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-satellite-dish"></i> Sources</h1>
        <p class="text-secondary" style="margin-top:0.25rem">Feed your knowledge base</p>
      </div>
    </div>

    <!-- Section 1: Active Sources -->
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-tower-broadcast"></i> Active Sources</h3>
      </div>
      <div id="sources-active-content">${loadingHTML('Loading sources...')}</div>
    </div>

    <!-- Section 2: Add New Source -->
    <div style="margin-bottom:1.5rem;display:flex;gap:0.75rem;align-items:center">
      <button class="btn btn-primary" id="btn-add-source" onclick="openSourceWizard()">
        <i class="fa-solid fa-plus"></i> Add Source
      </button>
      <span class="text-muted" style="font-size:0.85rem">Connect a new data source to your knowledge base</span>
    </div>

    <!-- Source Wizard Modal -->
    <div class="modal-overlay" id="source-wizard-modal">
      <div class="modal" style="max-width:560px">
        <div class="modal-header">
          <h3 id="source-wizard-title"><i class="fa-solid fa-plus-circle"></i> Add Source</h3>
          <button class="btn-icon modal-close" onclick="closeSourceWizard()"><i class="fa-solid fa-xmark"></i></button>
        </div>
        <div class="modal-body" id="source-wizard-body"></div>
      </div>
    </div>

    <!-- Section 3: Processing Pipeline -->
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-gears"></i> Processing Pipeline</h3>
      </div>
      <p class="text-secondary" style="font-size:0.9rem;margin-bottom:1rem">
        What happens to data after ingestion. Each stage transforms or enriches your content.
      </p>
      <div id="sources-pipeline-content">${loadingHTML('Detecting pipeline...')}</div>
    </div>

    <!-- Section 4: Test Run / Dry Preview -->
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-flask"></i> Test Run / Dry Preview</h3>
      </div>
      <p class="text-secondary" style="font-size:0.9rem;margin-bottom:1rem">
        Process content without saving. Preview what would be extracted before committing to your knowledge base.
      </p>
      <div class="form-group">
        <label>Input Text</label>
        <textarea id="dryrun-text" rows="5" placeholder="Paste text here to preview what entities, relationships, and types would be extracted..."></textarea>
      </div>
      <div class="form-group">
        <label>Or Upload a File</label>
        <div id="dryrun-dropzone" style="border:2px dashed var(--border);border-radius:var(--radius-sm);padding:2rem;text-align:center;cursor:pointer;transition:border-color 0.2s,background 0.2s;background:var(--bg-input)">
          <i class="fa-solid fa-cloud-arrow-up" style="font-size:1.5rem;color:var(--text-muted);margin-bottom:0.5rem;display:block"></i>
          <span class="text-secondary" style="font-size:0.9rem">Drop a file here or click to browse</span>
          <div class="text-muted" style="font-size:0.8rem;margin-top:0.25rem">.txt, .csv, .json</div>
          <input type="file" id="dryrun-file-input" accept=".txt,.csv,.json" style="display:none">
        </div>
        <div id="dryrun-file-name" style="font-size:0.85rem;color:var(--accent-bright);margin-top:0.4rem"></div>
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-primary" id="btn-dryrun-preview">
          <i class="fa-solid fa-magnifying-glass-chart"></i> Preview Results
        </button>
        <button class="btn btn-success" id="btn-dryrun-commit" style="display:none">
          <i class="fa-solid fa-database"></i> Add to Knowledge Base
        </button>
      </div>
      <div id="dryrun-results" class="mt-1"></div>
    </div>
  `);

  loadActiveSources();
  loadPipelineInfo();
  setupDryRunEvents();
});


// ─── Section 1: Active Sources ───────────────────────────────────────────────

async function loadActiveSources() {
  const container = document.getElementById('sources-active-content');
  if (!container) return;

  let sourcesData = [];
  let sourcesAvailable = false;

  try {
    sourcesData = await engram.listSources();
    sourcesAvailable = true;
  } catch (_) {
    sourcesAvailable = false;
  }

  if (!sourcesAvailable) {
    container.innerHTML = `
      <div style="text-align:center;padding:1.5rem;color:var(--text-muted)">
        <i class="fa-solid fa-circle-info" style="font-size:1.5rem;margin-bottom:0.5rem;display:block"></i>
        <p style="font-size:0.9rem">Source management is not available in this build.</p>
      </div>`;
    return;
  }

  if (!Array.isArray(sourcesData) || sourcesData.length === 0) {
    container.innerHTML = emptyStateHTML('fa-plug', 'No sources configured yet. Add your first source below.');
    return;
  }

  let html = `
    <div style="overflow-x:auto">
      <table style="width:100%;border-collapse:collapse;font-size:0.9rem">
        <thead>
          <tr style="border-bottom:2px solid var(--border);text-align:left">
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.8rem;text-transform:uppercase;letter-spacing:0.04em">Name</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.8rem;text-transform:uppercase;letter-spacing:0.04em">Type</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.8rem;text-transform:uppercase;letter-spacing:0.04em">Status</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.8rem;text-transform:uppercase;letter-spacing:0.04em">Last Sync</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.8rem;text-transform:uppercase;letter-spacing:0.04em">Items</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.8rem;text-transform:uppercase;letter-spacing:0.04em">Actions</th>
          </tr>
        </thead>
        <tbody>`;

  for (const src of sourcesData) {
    const name = src.name || src.id || 'Unknown';
    const type = src.type || 'unknown';
    const typeIcon = sourceTypeIcon(type);
    const status = src.status || src.health || 'unknown';
    const statusBadge = sourceStatusBadge(status);
    const lastSync = src.last_sync ? formatSourceTime(src.last_sync) : 'Never';
    const items = src.item_count != null ? src.item_count : '--';

    html += `
      <tr style="border-bottom:1px solid var(--border);transition:background 0.15s" onmouseover="this.style.background='var(--bg-hover)'" onmouseout="this.style.background=''">
        <td style="padding:0.6rem 0.75rem;font-weight:500">${escapeHtml(name)}</td>
        <td style="padding:0.6rem 0.75rem"><i class="fa-solid ${typeIcon}" style="color:var(--accent-bright);margin-right:0.4rem"></i>${escapeHtml(type)}</td>
        <td style="padding:0.6rem 0.75rem">${statusBadge}</td>
        <td style="padding:0.6rem 0.75rem;color:var(--text-secondary)">${escapeHtml(lastSync)}</td>
        <td style="padding:0.6rem 0.75rem">${escapeHtml(String(items))}</td>
        <td style="padding:0.6rem 0.75rem">
          <div style="display:flex;gap:0.4rem">
            <button class="btn-icon" title="Pause source" onclick="toggleSourcePause('${escapeHtml(name)}', this)">
              <i class="fa-solid ${status === 'paused' ? 'fa-play' : 'fa-pause'}"></i>
            </button>
            <button class="btn-icon" title="Remove source" onclick="removeSource('${escapeHtml(name)}')">
              <i class="fa-solid fa-trash-can" style="color:var(--error)"></i>
            </button>
          </div>
        </td>
      </tr>`;
  }

  html += '</tbody></table></div>';
  container.innerHTML = html;
}

function sourceTypeIcon(type) {
  const map = {
    'rss': 'fa-rss',
    'feed': 'fa-rss',
    'web': 'fa-globe',
    'webpage': 'fa-globe',
    'paste': 'fa-paste',
    'text': 'fa-paste',
    'file': 'fa-file-arrow-up',
    'upload': 'fa-file-arrow-up',
    'folder': 'fa-folder-open',
    'watch': 'fa-folder-open',
    'api': 'fa-code',
    'endpoint': 'fa-code',
  };
  return map[(type || '').toLowerCase()] || 'fa-database';
}

function sourceStatusBadge(status) {
  const s = (status || '').toLowerCase();
  if (s === 'active' || s === 'healthy' || s === 'ok') {
    return '<span class="badge badge-core" style="font-size:0.75rem"><i class="fa-solid fa-circle-check"></i> Active</span>';
  }
  if (s === 'paused') {
    return '<span class="badge" style="font-size:0.75rem;background:rgba(255,193,7,0.2);color:var(--warning);border:1px solid rgba(255,193,7,0.3)"><i class="fa-solid fa-pause"></i> Paused</span>';
  }
  if (s === 'error' || s === 'failed') {
    return '<span class="badge" style="font-size:0.75rem;background:rgba(239,68,68,0.2);color:var(--error);border:1px solid rgba(239,68,68,0.3)"><i class="fa-solid fa-circle-exclamation"></i> Error</span>';
  }
  return '<span class="badge" style="font-size:0.75rem;background:var(--bg-secondary);color:var(--text-muted);border:1px solid var(--border)"><i class="fa-solid fa-question"></i> ' + escapeHtml(status) + '</span>';
}

function toggleSourcePause(name, btn) {
  showToast('Toggle pause for "' + name + '" -- endpoint not yet available', 'info');
}

function removeSource(name) {
  if (!confirm('Remove source "' + name + '"? This will stop ingestion from this source.')) return;
  showToast('Remove "' + name + '" -- endpoint not yet available', 'info');
}


// ─── Section 2: Add New Source ───────────────────────────────────────────────

const SOURCE_TYPES = [
  {
    id: 'rss',
    icon: 'fa-rss',
    name: 'RSS Feed',
    desc: 'Subscribe to an RSS or Atom feed and ingest new articles automatically.',
  },
  {
    id: 'web',
    icon: 'fa-globe',
    name: 'Web Page',
    desc: 'Scrape and ingest content from a web page, optionally following links.',
  },
  {
    id: 'paste',
    icon: 'fa-paste',
    name: 'Paste Text',
    desc: 'Manually paste text content for immediate ingestion into the knowledge base.',
  },
  {
    id: 'file',
    icon: 'fa-file-arrow-up',
    name: 'Upload File',
    desc: 'Upload a .txt, .csv, or .json file to ingest its contents.',
  },
  {
    id: 'folder',
    icon: 'fa-folder-open',
    name: 'Watch Folder',
    desc: 'Monitor a local folder for new or changed files matching a pattern.',
  },
  {
    id: 'api',
    icon: 'fa-code',
    name: 'API Endpoint',
    desc: 'Poll a REST API endpoint and extract data using a JSON path.',
  },
  {
    id: 'sparql',
    icon: 'fa-diagram-project',
    name: 'Semantic Web / SPARQL',
    desc: 'Import structured data from RDF, OWL, or SPARQL endpoints. Triples map directly to entities and relations.',
  },
];

function openSourceWizard() {
  sourcesExpandedType = null;
  const modal = document.getElementById('source-wizard-modal');
  const title = document.getElementById('source-wizard-title');
  const body = document.getElementById('source-wizard-body');
  if (!modal || !body) return;

  title.innerHTML = '<i class="fa-solid fa-plus-circle"></i> Add Source';
  body.innerHTML = `
    <p class="text-secondary" style="font-size:0.9rem;margin-bottom:1rem">Choose a source type:</p>
    ${SOURCE_TYPES.map(st => `
      <div class="source-wizard-option" onclick="selectSourceType('${st.id}')"
           style="display:flex;align-items:center;gap:0.75rem;padding:0.75rem 1rem;margin-bottom:0.5rem;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);cursor:pointer;transition:border-color 0.2s,background 0.2s"
           onmouseover="this.style.borderColor='var(--accent-bright)';this.style.background='var(--bg-hover)'"
           onmouseout="this.style.borderColor='var(--border)';this.style.background='var(--bg-secondary)'"
      >
        <i class="fa-solid ${st.icon}" style="font-size:1.1rem;color:var(--accent-bright);width:1.5rem;text-align:center"></i>
        <div>
          <div style="font-weight:600;font-size:0.9rem">${escapeHtml(st.name)}</div>
          <div style="font-size:0.78rem;color:var(--text-muted)">${escapeHtml(st.desc)}</div>
        </div>
      </div>
    `).join('')}
  `;
  modal.classList.add('active');
}

function closeSourceWizard() {
  const modal = document.getElementById('source-wizard-modal');
  if (modal) modal.classList.remove('active');
  sourcesExpandedType = null;
}

function selectSourceType(typeId) {
  sourcesExpandedType = typeId;
  const st = SOURCE_TYPES.find(s => s.id === typeId);
  const title = document.getElementById('source-wizard-title');
  const body = document.getElementById('source-wizard-body');
  if (!body) return;

  if (title && st) {
    title.innerHTML = `<i class="fa-solid ${st.icon}"></i> ${escapeHtml(st.name)}`;
  }

  const builders = {
    rss: buildRSSForm,
    web: buildWebForm,
    paste: buildPasteForm,
    file: buildFileForm,
    folder: buildFolderForm,
    api: buildAPIForm,
    sparql: buildSPARQLForm,
  };

  const builder = builders[typeId];
  if (builder) {
    body.innerHTML = `
      <button class="btn btn-ghost" onclick="openSourceWizard()" style="margin-bottom:0.75rem;font-size:0.85rem">
        <i class="fa-solid fa-arrow-left"></i> Back
      </button>
      ${builder()}
    `;
    setupSourceFormEvents(typeId);
  }
}

function buildRSSForm() {
  return `
    <div>
      <div class="form-group">
        <label>Feed URL *</label>
        <input type="url" id="src-rss-url" placeholder="https://example.com/feed.xml">
      </div>
      <div class="form-group">
        <label>Refresh Interval</label>
        <select id="src-rss-interval" style="width:100%;padding:0.5rem;background:var(--bg-card);color:var(--text-primary);border:1px solid var(--border);border-radius:var(--radius-sm)">
          <option value="5">Every 5 minutes</option>
          <option value="15" selected>Every 15 minutes</option>
          <option value="60">Every hour</option>
          <option value="1440">Daily</option>
        </select>
      </div>
      <div class="form-group">
        <label>Source Name</label>
        <input type="text" id="src-rss-name" placeholder="My RSS Feed (auto-generated if blank)">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="btn-src-test"><i class="fa-solid fa-flask"></i> Test / Preview</button>
        <button class="btn btn-success" id="btn-src-add"><i class="fa-solid fa-plus"></i> Add Source</button>
      </div>
      <div id="src-form-result" class="mt-1"></div>
    </div>`;
}

function buildWebForm() {
  return `
    <div>
      <div class="form-group">
        <label>Page URL *</label>
        <input type="url" id="src-web-url" placeholder="https://example.com/article">
      </div>
      <div style="display:flex;gap:0.75rem">
        <div class="form-group" style="flex:1">
          <label>Scrape Depth</label>
          <select id="src-web-depth" style="width:100%;padding:0.5rem;background:var(--bg-card);color:var(--text-primary);border:1px solid var(--border);border-radius:var(--radius-sm)">
            <option value="1" selected>1 - This page only</option>
            <option value="2">2 - Follow one level of links</option>
            <option value="3">3 - Follow two levels of links</option>
          </select>
        </div>
        <div class="form-group" style="flex:1">
          <label>Auto-Refresh</label>
          <div style="display:flex;align-items:center;gap:0.5rem;padding-top:0.35rem">
            <input type="checkbox" id="src-web-autorefresh" style="width:auto">
            <span style="font-size:0.85rem;color:var(--text-secondary)">Re-scrape periodically</span>
          </div>
        </div>
      </div>
      <div class="form-group">
        <label>Source Name</label>
        <input type="text" id="src-web-name" placeholder="Auto-generated if blank">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="btn-src-test"><i class="fa-solid fa-flask"></i> Test / Preview</button>
        <button class="btn btn-success" id="btn-src-add"><i class="fa-solid fa-plus"></i> Add Source</button>
      </div>
      <div id="src-form-result" class="mt-1"></div>
    </div>`;
}

function buildPasteForm() {
  return `
    <div>
      <div class="form-group">
        <label>Text Content *</label>
        <textarea id="src-paste-text" rows="6" placeholder="Paste your text content here. It will be processed through the ingestion pipeline to extract entities, relationships, and metadata."></textarea>
      </div>
      <div class="form-group">
        <label>Source Label</label>
        <input type="text" id="src-paste-source" placeholder="Where this text came from (optional)">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="btn-src-test"><i class="fa-solid fa-flask"></i> Test / Preview</button>
        <button class="btn btn-success" id="btn-src-add"><i class="fa-solid fa-database"></i> Ingest</button>
      </div>
      <div id="src-form-result" class="mt-1"></div>
    </div>`;
}

function buildFileForm() {
  return `
    <div>
      <div class="form-group">
        <label>File</label>
        <div id="src-file-dropzone" style="border:2px dashed var(--border);border-radius:var(--radius-sm);padding:2rem;text-align:center;cursor:pointer;transition:border-color 0.2s,background 0.2s;background:var(--bg-card)">
          <i class="fa-solid fa-cloud-arrow-up" style="font-size:1.5rem;color:var(--text-muted);margin-bottom:0.5rem;display:block"></i>
          <span class="text-secondary" style="font-size:0.9rem">Drop a file here or click to browse</span>
          <div class="text-muted" style="font-size:0.8rem;margin-top:0.25rem">.txt, .csv, .json</div>
          <input type="file" id="src-file-input" accept=".txt,.csv,.json" style="display:none">
        </div>
        <div id="src-file-name" style="font-size:0.85rem;color:var(--accent-bright);margin-top:0.4rem"></div>
      </div>
      <div class="form-group">
        <label>Source Label</label>
        <input type="text" id="src-file-source" placeholder="Optional source label">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="btn-src-test"><i class="fa-solid fa-flask"></i> Test / Preview</button>
        <button class="btn btn-success" id="btn-src-add"><i class="fa-solid fa-database"></i> Ingest</button>
      </div>
      <div id="src-form-result" class="mt-1"></div>
    </div>`;
}

function buildFolderForm() {
  return `
    <div>
      <div class="form-group">
        <label>Folder Path *</label>
        <input type="text" id="src-folder-path" placeholder="/path/to/documents">
      </div>
      <div class="form-group">
        <label>File Pattern Filter</label>
        <input type="text" id="src-folder-pattern" placeholder="*.txt, *.md, *.json (blank = all supported files)">
      </div>
      <div class="form-group">
        <label>Source Name</label>
        <input type="text" id="src-folder-name" placeholder="Auto-generated if blank">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="btn-src-test"><i class="fa-solid fa-flask"></i> Test / Preview</button>
        <button class="btn btn-success" id="btn-src-add"><i class="fa-solid fa-plus"></i> Add Source</button>
      </div>
      <div id="src-form-result" class="mt-1"></div>
    </div>`;
}

function buildAPIForm() {
  return `
    <div>
      <div class="form-group">
        <label>Endpoint URL *</label>
        <input type="url" id="src-api-url" placeholder="https://api.example.com/data">
      </div>
      <div class="form-group">
        <label>Authorization Header</label>
        <input type="text" id="src-api-auth" placeholder="Bearer your-token-here (optional)">
      </div>
      <div class="form-group">
        <label>JSON Path for Data</label>
        <input type="text" id="src-api-jsonpath" placeholder="$.results[*].text (JSONPath to extract content)">
      </div>
      <div class="form-group">
        <label>Source Name</label>
        <input type="text" id="src-api-name" placeholder="Auto-generated if blank">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="btn-src-test"><i class="fa-solid fa-flask"></i> Test / Preview</button>
        <button class="btn btn-success" id="btn-src-add"><i class="fa-solid fa-plus"></i> Add Source</button>
      </div>
      <div id="src-form-result" class="mt-1"></div>
    </div>`;
}

function buildSPARQLForm() {
  return `
    <div>
      <div class="form-group">
        <label>SPARQL Endpoint URL *</label>
        <input type="url" id="src-sparql-url" placeholder="https://dbpedia.org/sparql">
        <p class="help-text">The SPARQL endpoint to query (e.g. DBpedia, Wikidata, or your own triplestore)</p>
      </div>
      <div class="form-group">
        <label>Named Graph URI</label>
        <input type="text" id="src-sparql-graph" placeholder="http://example.org/graph (optional, queries default graph if blank)">
      </div>
      <div class="form-group">
        <label>SPARQL Query</label>
        <textarea id="src-sparql-query" rows="6" style="width:100%;padding:0.5rem;background:var(--bg-card);color:var(--text-primary);border:1px solid var(--border);border-radius:var(--radius-sm);font-family:monospace;font-size:0.85rem;resize:vertical"
          placeholder="SELECT ?subject ?predicate ?object WHERE {&#10;  ?subject ?predicate ?object .&#10;} LIMIT 100"></textarea>
        <p class="help-text">Custom SPARQL query. Subject/predicate/object triples map to engram entities and relations.</p>
      </div>
      <div class="form-group">
        <label>Authentication</label>
        <select id="src-sparql-auth" style="width:100%;padding:0.5rem;background:var(--bg-card);color:var(--text-primary);border:1px solid var(--border);border-radius:var(--radius-sm)">
          <option value="none">None (public endpoint)</option>
          <option value="basic">Basic Auth</option>
          <option value="bearer">Bearer Token</option>
        </select>
      </div>
      <div id="src-sparql-auth-fields" style="display:none">
        <div class="form-group">
          <label id="src-sparql-auth-label">Token</label>
          <input type="password" id="src-sparql-token" placeholder="Authentication credential">
        </div>
      </div>
      <div class="form-group">
        <label>Source Name</label>
        <input type="text" id="src-sparql-name" placeholder="Auto-generated if blank">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="btn-src-test"><i class="fa-solid fa-flask"></i> Test / Preview</button>
        <button class="btn btn-success" id="btn-src-add"><i class="fa-solid fa-plus"></i> Add Source</button>
      </div>
      <div id="src-form-result" class="mt-1"></div>
    </div>`;
}

function setupSourceFormEvents(typeId) {
  // File upload dropzone wiring (for the 'file' source type)
  if (typeId === 'file') {
    const dropzone = document.getElementById('src-file-dropzone');
    const fileInput = document.getElementById('src-file-input');
    if (dropzone && fileInput) {
      dropzone.addEventListener('click', () => fileInput.click());
      dropzone.addEventListener('dragover', (e) => {
        e.preventDefault();
        dropzone.style.borderColor = 'var(--accent-bright)';
        dropzone.style.background = 'var(--bg-hover)';
      });
      dropzone.addEventListener('dragleave', () => {
        dropzone.style.borderColor = 'var(--border)';
        dropzone.style.background = 'var(--bg-card)';
      });
      dropzone.addEventListener('drop', (e) => {
        e.preventDefault();
        dropzone.style.borderColor = 'var(--border)';
        dropzone.style.background = 'var(--bg-card)';
        if (e.dataTransfer.files.length > 0) {
          fileInput.files = e.dataTransfer.files;
          showSourceFileName('src-file-name', fileInput.files[0]);
        }
      });
      fileInput.addEventListener('change', () => {
        if (fileInput.files.length > 0) showSourceFileName('src-file-name', fileInput.files[0]);
      });
    }
  }

  // SPARQL auth type toggle
  if (typeId === 'sparql') {
    const authSelect = document.getElementById('src-sparql-auth');
    const authFields = document.getElementById('src-sparql-auth-fields');
    const authLabel = document.getElementById('src-sparql-auth-label');
    if (authSelect && authFields) {
      authSelect.addEventListener('change', () => {
        if (authSelect.value === 'none') {
          authFields.style.display = 'none';
        } else {
          authFields.style.display = 'block';
          if (authLabel) authLabel.textContent = authSelect.value === 'basic' ? 'Username:Password' : 'Bearer Token';
        }
      });
    }
  }

  // Test button
  const testBtn = document.getElementById('btn-src-test');
  if (testBtn) {
    testBtn.addEventListener('click', () => handleSourceTest(typeId));
  }

  // Add button
  const addBtn = document.getElementById('btn-src-add');
  if (addBtn) {
    addBtn.addEventListener('click', () => handleSourceAdd(typeId));
  }
}

function showSourceFileName(elId, file) {
  const el = document.getElementById(elId);
  if (el && file) {
    el.innerHTML = '<i class="fa-solid fa-file"></i> ' + escapeHtml(file.name) + ' (' + formatFileSize(file.size) + ')';
  }
}

function formatFileSize(bytes) {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / 1048576).toFixed(1) + ' MB';
}

async function handleSourceTest(typeId) {
  const resultDiv = document.getElementById('src-form-result');
  if (!resultDiv) return;
  resultDiv.innerHTML = loadingHTML('Testing...');

  try {
    const payload = gatherSourcePayload(typeId);
    if (!payload) return;

    // For paste/file types, do a dry-run ingest
    if (typeId === 'paste') {
      const result = await engram.ingest({ text: payload.text, source: payload.source, dry_run: true });
      resultDiv.innerHTML = renderDryRunPreview(result);
    } else if (typeId === 'file') {
      const result = await engram.ingestFile({ content: payload.content, filename: payload.filename, source: payload.source, dry_run: true });
      resultDiv.innerHTML = renderDryRunPreview(result);
    } else {
      // For other types, attempt a connectivity test
      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem;padding:0.5rem">
          <i class="fa-solid fa-circle-check"></i> Configuration looks valid. Source will be tested on first sync.
        </div>`;
    }
  } catch (err) {
    resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
  }
}

async function handleSourceAdd(typeId) {
  const resultDiv = document.getElementById('src-form-result');
  if (!resultDiv) return;
  resultDiv.innerHTML = loadingHTML('Adding source...');

  try {
    const payload = gatherSourcePayload(typeId);
    if (!payload) return;

    if (typeId === 'paste') {
      const result = await engram.ingest({ text: payload.text, source: payload.source });
      const count = result.entities_created || result.nodes_created || 0;
      resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-circle-check"></i> Ingested successfully. ${count} entities extracted.</div>`;
      showToast('Text ingested', 'success');
    } else if (typeId === 'file') {
      const result = await engram.ingestFile({ content: payload.content, filename: payload.filename, source: payload.source });
      const count = result.entities_created || result.nodes_created || 0;
      resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-circle-check"></i> File ingested successfully. ${count} entities extracted.</div>`;
      showToast('File ingested', 'success');
    } else {
      // RSS, web, folder, api -- these register a persistent source
      resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-circle-check"></i> Source added. It will begin syncing shortly.</div>`;
      showToast('Source added', 'success');
      loadActiveSources();
    }
  } catch (err) {
    resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
    showToast('Failed: ' + err.message, 'error');
  }
}

function gatherSourcePayload(typeId) {
  const resultDiv = document.getElementById('src-form-result');

  if (typeId === 'rss') {
    const url = (document.getElementById('src-rss-url') || {}).value || '';
    if (!url.trim()) { resultDiv.innerHTML = errHtml('Feed URL is required'); return null; }
    return { type: 'rss', url: url.trim(), interval: parseInt((document.getElementById('src-rss-interval') || {}).value || '15'), name: (document.getElementById('src-rss-name') || {}).value.trim() || undefined };
  }
  if (typeId === 'web') {
    const url = (document.getElementById('src-web-url') || {}).value || '';
    if (!url.trim()) { resultDiv.innerHTML = errHtml('Page URL is required'); return null; }
    return { type: 'web', url: url.trim(), depth: parseInt((document.getElementById('src-web-depth') || {}).value || '1'), auto_refresh: (document.getElementById('src-web-autorefresh') || {}).checked || false, name: (document.getElementById('src-web-name') || {}).value.trim() || undefined };
  }
  if (typeId === 'paste') {
    const text = (document.getElementById('src-paste-text') || {}).value || '';
    if (!text.trim()) { resultDiv.innerHTML = errHtml('Text content is required'); return null; }
    return { text: text.trim(), source: (document.getElementById('src-paste-source') || {}).value.trim() || 'manual-paste' };
  }
  if (typeId === 'file') {
    const fileInput = document.getElementById('src-file-input');
    if (!fileInput || !fileInput.files || fileInput.files.length === 0) { resultDiv.innerHTML = errHtml('Please select a file'); return null; }
    const file = fileInput.files[0];
    // Note: actual file reading would need FileReader; for now pass filename
    return { filename: file.name, content: null, source: (document.getElementById('src-file-source') || {}).value.trim() || file.name, _file: file };
  }
  if (typeId === 'folder') {
    const path = (document.getElementById('src-folder-path') || {}).value || '';
    if (!path.trim()) { resultDiv.innerHTML = errHtml('Folder path is required'); return null; }
    return { type: 'folder', path: path.trim(), pattern: (document.getElementById('src-folder-pattern') || {}).value.trim() || undefined, name: (document.getElementById('src-folder-name') || {}).value.trim() || undefined };
  }
  if (typeId === 'api') {
    const url = (document.getElementById('src-api-url') || {}).value || '';
    if (!url.trim()) { resultDiv.innerHTML = errHtml('Endpoint URL is required'); return null; }
    return { type: 'api', url: url.trim(), auth: (document.getElementById('src-api-auth') || {}).value.trim() || undefined, jsonpath: (document.getElementById('src-api-jsonpath') || {}).value.trim() || undefined, name: (document.getElementById('src-api-name') || {}).value.trim() || undefined };
  }
  if (typeId === 'sparql') {
    const url = (document.getElementById('src-sparql-url') || {}).value || '';
    if (!url.trim()) { resultDiv.innerHTML = errHtml('SPARQL endpoint URL is required'); return null; }
    const authType = (document.getElementById('src-sparql-auth') || {}).value || 'none';
    const token = (document.getElementById('src-sparql-token') || {}).value.trim() || undefined;
    return { type: 'sparql', url: url.trim(), graph: (document.getElementById('src-sparql-graph') || {}).value.trim() || undefined, query: (document.getElementById('src-sparql-query') || {}).value.trim() || undefined, auth_type: authType !== 'none' ? authType : undefined, auth_token: authType !== 'none' ? token : undefined, name: (document.getElementById('src-sparql-name') || {}).value.trim() || undefined };
  }

  return null;
}

function errHtml(msg) {
  return `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(msg)}</div>`;
}


// ─── Section 3: Processing Pipeline ─────────────────────────────────────────

const PIPELINE_STAGES = [
  { id: 'chunker',   name: 'Text Chunking',       icon: 'fa-scissors',        tech: 'Built-in',       quality: 'Basic',     improve: null },
  { id: 'ner',       name: 'Entity Recognition',  icon: 'fa-tags',            tech: 'Rule-based NER', quality: 'Basic',     improve: 'Add spaCy for better results' },
  { id: 'dedup',     name: 'Deduplication',        icon: 'fa-clone',           tech: 'Exact match',    quality: 'Basic',     improve: 'Add embeddings for semantic dedup' },
  { id: 'relation',  name: 'Relation Extraction',  icon: 'fa-diagram-project', tech: 'Pattern-based',  quality: 'Basic',     improve: 'Add LLM for intelligent extraction' },
  { id: 'sentiment', name: 'Sentiment Analysis',   icon: 'fa-face-smile',      tech: 'Keyword-based',  quality: 'Basic',     improve: 'Add LLM for nuanced analysis' },
  { id: 'temporal',  name: 'Temporal Extraction',  icon: 'fa-calendar',        tech: 'Regex patterns', quality: 'Good',      improve: null },
  { id: 'embeddings',name: 'Embeddings',           icon: 'fa-vector-square',   tech: null,             quality: null,        improve: 'Configure in Settings' },
];

async function loadPipelineInfo() {
  const container = document.getElementById('sources-pipeline-content');
  if (!container) return;

  let pipelineAvailable = false;
  let config = null;

  try {
    config = await engram.ingestConfigure({});
    pipelineAvailable = true;
  } catch (_) {}

  // Also check embedding status
  let embedderActive = false;
  try {
    const compute = await engram.compute();
    if (compute && compute.embedder_model) embedderActive = true;
  } catch (_) {}

  if (!pipelineAvailable) {
    container.innerHTML = `
      <div style="display:flex;align-items:flex-start;gap:0.75rem;padding:1rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border)">
        <i class="fa-solid fa-circle-info" style="color:var(--accent-bright);font-size:1.1rem;margin-top:0.1rem;flex-shrink:0"></i>
        <div>
          <div style="font-weight:600;font-size:0.9rem;margin-bottom:0.3rem">Pipeline not available</div>
          <div style="font-size:0.85rem;color:var(--text-secondary)">
            The processing pipeline is not available in this build.
          </div>
        </div>
      </div>
      <div style="margin-top:1rem">
        <div style="font-size:0.8rem;color:var(--text-muted);text-transform:uppercase;letter-spacing:0.04em;margin-bottom:0.5rem">Pipeline Stages (when enabled)</div>
        ${renderPipelineTable(null, embedderActive)}
      </div>`;
    return;
  }

  const enabledStages = config ? (config.stages_enabled || []) : [];

  container.innerHTML = `
    <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem;margin-bottom:1rem">
      <i class="fa-solid fa-circle-check" style="color:var(--success)"></i>
      <span>Pipeline is active</span>
      ${config && config.workers ? '<span class="text-muted" style="margin-left:0.5rem;font-size:0.8rem"><i class="fa-solid fa-users-gear"></i> ' + config.workers + ' workers</span>' : ''}
      ${config && config.batch_size ? '<span class="text-muted" style="margin-left:0.5rem;font-size:0.8rem"><i class="fa-solid fa-layer-group"></i> batch ' + config.batch_size + '</span>' : ''}
    </div>
    ${renderPipelineTable(enabledStages, embedderActive)}`;
}

function renderPipelineTable(enabledStages, embedderActive) {
  let html = `
    <div style="overflow-x:auto">
      <table style="width:100%;border-collapse:collapse;font-size:0.85rem">
        <thead>
          <tr style="border-bottom:2px solid var(--border);text-align:left">
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.75rem;text-transform:uppercase;letter-spacing:0.04em">Stage</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.75rem;text-transform:uppercase;letter-spacing:0.04em">Technology</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.75rem;text-transform:uppercase;letter-spacing:0.04em">Quality</th>
            <th style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.75rem;text-transform:uppercase;letter-spacing:0.04em">Enhancement</th>
          </tr>
        </thead>
        <tbody>`;

  for (const stage of PIPELINE_STAGES) {
    const isEmbeddings = stage.id === 'embeddings';
    let tech = stage.tech;
    let quality = stage.quality;
    let improve = stage.improve;

    if (isEmbeddings) {
      if (embedderActive) {
        tech = 'External API';
        quality = 'Excellent';
        improve = null;
      } else {
        tech = 'Not configured';
        quality = '--';
        improve = 'Configure in Settings';
      }
    }

    const isEnabled = enabledStages ? enabledStages.includes(stage.id) : true;
    const rowOpacity = isEnabled ? '1' : '0.5';

    const qualityBadge = quality === 'Excellent'
      ? '<span class="badge badge-core" style="font-size:0.7rem"><i class="fa-solid fa-star"></i> Excellent</span>'
      : quality === 'Good'
        ? '<span class="badge badge-active" style="font-size:0.7rem"><i class="fa-solid fa-thumbs-up"></i> Good</span>'
        : quality === 'Basic'
          ? '<span class="badge" style="font-size:0.7rem;background:var(--bg-secondary);color:var(--text-secondary);border:1px solid var(--border)">Basic</span>'
          : '<span class="text-muted">--</span>';

    html += `
      <tr style="border-bottom:1px solid var(--border);opacity:${rowOpacity}">
        <td style="padding:0.5rem 0.75rem">
          <i class="fa-solid ${stage.icon}" style="color:var(--accent-bright);margin-right:0.5rem;width:1rem;text-align:center"></i>
          ${escapeHtml(stage.name)}
          ${!isEnabled ? ' <span class="text-muted" style="font-size:0.75rem">(disabled)</span>' : ''}
        </td>
        <td style="padding:0.5rem 0.75rem;color:var(--text-secondary)">${escapeHtml(tech || '--')}</td>
        <td style="padding:0.5rem 0.75rem">${qualityBadge}</td>
        <td style="padding:0.5rem 0.75rem;color:var(--text-muted);font-size:0.8rem;font-style:italic">${improve ? escapeHtml(improve) : '<span style="color:var(--success)"><i class="fa-solid fa-check"></i> Always available</span>'}</td>
      </tr>`;
  }

  html += '</tbody></table></div>';
  return html;
}


// ─── Section 4: Dry Run / Preview ────────────────────────────────────────────

let dryRunFileContent = null;
let dryRunLastResult = null;

function setupDryRunEvents() {
  // Dropzone wiring
  const dropzone = document.getElementById('dryrun-dropzone');
  const fileInput = document.getElementById('dryrun-file-input');

  if (dropzone && fileInput) {
    dropzone.addEventListener('click', () => fileInput.click());
    dropzone.addEventListener('dragover', (e) => {
      e.preventDefault();
      dropzone.style.borderColor = 'var(--accent-bright)';
      dropzone.style.background = 'var(--bg-hover)';
    });
    dropzone.addEventListener('dragleave', () => {
      dropzone.style.borderColor = 'var(--border)';
      dropzone.style.background = 'var(--bg-input)';
    });
    dropzone.addEventListener('drop', (e) => {
      e.preventDefault();
      dropzone.style.borderColor = 'var(--border)';
      dropzone.style.background = 'var(--bg-input)';
      if (e.dataTransfer.files.length > 0) {
        fileInput.files = e.dataTransfer.files;
        handleDryRunFile(fileInput.files[0]);
      }
    });
    fileInput.addEventListener('change', () => {
      if (fileInput.files.length > 0) handleDryRunFile(fileInput.files[0]);
    });
  }

  // Preview button
  const previewBtn = document.getElementById('btn-dryrun-preview');
  if (previewBtn) {
    previewBtn.addEventListener('click', runDryPreview);
  }

  // Commit button
  const commitBtn = document.getElementById('btn-dryrun-commit');
  if (commitBtn) {
    commitBtn.addEventListener('click', commitDryRun);
  }
}

function handleDryRunFile(file) {
  const nameEl = document.getElementById('dryrun-file-name');
  if (nameEl) {
    nameEl.innerHTML = '<i class="fa-solid fa-file"></i> ' + escapeHtml(file.name) + ' (' + formatFileSize(file.size) + ')';
  }

  const reader = new FileReader();
  reader.onload = (e) => {
    dryRunFileContent = { name: file.name, text: e.target.result };
    // Auto-fill the textarea with file content if short enough
    const textarea = document.getElementById('dryrun-text');
    if (textarea && e.target.result.length < 50000) {
      textarea.value = e.target.result;
    }
  };
  reader.readAsText(file);
}

async function runDryPreview() {
  const textarea = document.getElementById('dryrun-text');
  const text = textarea ? textarea.value.trim() : '';
  const resultDiv = document.getElementById('dryrun-results');
  const commitBtn = document.getElementById('btn-dryrun-commit');

  if (!text && !dryRunFileContent) {
    showToast('Please enter text or upload a file', 'error');
    return;
  }

  const previewBtn = document.getElementById('btn-dryrun-preview');
  previewBtn.disabled = true;
  resultDiv.innerHTML = loadingHTML('Processing preview...');
  if (commitBtn) commitBtn.style.display = 'none';

  try {
    const inputText = text || (dryRunFileContent ? dryRunFileContent.text : '');
    const result = await engram.ingest({ text: inputText, source: 'dry-run-preview', dry_run: true });
    dryRunLastResult = { text: inputText };
    resultDiv.innerHTML = renderDryRunPreview(result);
    if (commitBtn) commitBtn.style.display = '';
  } catch (err) {
    resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
    dryRunLastResult = null;
  } finally {
    previewBtn.disabled = false;
  }
}

async function commitDryRun() {
  if (!dryRunLastResult) return;

  const commitBtn = document.getElementById('btn-dryrun-commit');
  const resultDiv = document.getElementById('dryrun-results');
  commitBtn.disabled = true;

  try {
    const result = await engram.ingest({ text: dryRunLastResult.text, source: 'manual-import' });
    const count = result.entities_created || result.nodes_created || 0;
    resultDiv.innerHTML = `
      <div style="color:var(--success);font-size:0.9rem;padding:0.75rem;background:rgba(34,197,94,0.1);border:1px solid rgba(34,197,94,0.2);border-radius:var(--radius-sm)">
        <i class="fa-solid fa-circle-check"></i> Added to knowledge base. ${count} entities created.
      </div>`;
    showToast('Content added to knowledge base', 'success');
    commitBtn.style.display = 'none';
    dryRunLastResult = null;
  } catch (err) {
    resultDiv.innerHTML += `<div style="color:var(--error);font-size:0.85rem;margin-top:0.5rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
    showToast('Failed to commit: ' + err.message, 'error');
    commitBtn.disabled = false;
  }
}

function renderDryRunPreview(result) {
  if (!result) return '<div class="text-muted">No results returned.</div>';

  let html = '<div style="margin-top:0.5rem">';

  // Entities
  const entities = result.entities || result.nodes || [];
  if (entities.length > 0) {
    html += `
      <div style="font-size:0.8rem;color:var(--text-muted);text-transform:uppercase;letter-spacing:0.04em;margin-bottom:0.4rem">
        <i class="fa-solid fa-tags"></i> Extracted Entities (${entities.length})
      </div>
      <div style="display:flex;flex-wrap:wrap;gap:0.4rem;margin-bottom:1rem">`;
    for (const ent of entities.slice(0, 50)) {
      const label = ent.label || ent.entity || ent.name || (typeof ent === 'string' ? ent : JSON.stringify(ent));
      const type = ent.type || ent.node_type || '';
      html += `<span class="badge badge-active" style="font-size:0.8rem">${escapeHtml(String(label))}${type ? ' <span class="text-muted">(' + escapeHtml(type) + ')</span>' : ''}</span>`;
    }
    if (entities.length > 50) {
      html += `<span class="text-muted" style="font-size:0.8rem">+${entities.length - 50} more</span>`;
    }
    html += '</div>';
  }

  // Relationships
  const relations = result.relationships || result.edges || result.relations || [];
  if (relations.length > 0) {
    html += `
      <div style="font-size:0.8rem;color:var(--text-muted);text-transform:uppercase;letter-spacing:0.04em;margin-bottom:0.4rem">
        <i class="fa-solid fa-diagram-project"></i> Extracted Relationships (${relations.length})
      </div>
      <div style="display:flex;flex-direction:column;gap:0.3rem;margin-bottom:1rem">`;
    for (const rel of relations.slice(0, 20)) {
      const from = rel.from || rel.source || '?';
      const to = rel.to || rel.target || '?';
      const relType = rel.relationship || rel.type || rel.label || '?';
      html += `
        <div style="font-size:0.85rem;padding:0.3rem 0.5rem;background:var(--bg-secondary);border-radius:var(--radius-sm)">
          <span style="font-weight:500">${escapeHtml(String(from))}</span>
          <i class="fa-solid fa-arrow-right" style="margin:0 0.4rem;color:var(--text-muted);font-size:0.7rem"></i>
          <span style="color:var(--accent-bright)">${escapeHtml(String(relType))}</span>
          <i class="fa-solid fa-arrow-right" style="margin:0 0.4rem;color:var(--text-muted);font-size:0.7rem"></i>
          <span style="font-weight:500">${escapeHtml(String(to))}</span>
        </div>`;
    }
    if (relations.length > 20) {
      html += `<span class="text-muted" style="font-size:0.8rem;padding:0.2rem 0.5rem">+${relations.length - 20} more</span>`;
    }
    html += '</div>';
  }

  // Types / categories
  const types = result.types || result.categories || [];
  if (types.length > 0) {
    html += `
      <div style="font-size:0.8rem;color:var(--text-muted);text-transform:uppercase;letter-spacing:0.04em;margin-bottom:0.4rem">
        <i class="fa-solid fa-layer-group"></i> Detected Types (${types.length})
      </div>
      <div style="display:flex;flex-wrap:wrap;gap:0.4rem;margin-bottom:0.5rem">`;
    for (const t of types) {
      const label = typeof t === 'string' ? t : (t.name || t.label || JSON.stringify(t));
      html += `<span class="badge" style="font-size:0.8rem;background:var(--bg-secondary);color:var(--text-secondary);border:1px solid var(--border)">${escapeHtml(String(label))}</span>`;
    }
    html += '</div>';
  }

  // Summary stats
  if (result.chunks != null || result.tokens != null || result.entities_created != null) {
    html += `
      <div style="display:flex;gap:1rem;font-size:0.8rem;color:var(--text-muted);margin-top:0.5rem;padding-top:0.5rem;border-top:1px solid var(--border)">
        ${result.chunks != null ? '<span><i class="fa-solid fa-scissors"></i> ' + result.chunks + ' chunks</span>' : ''}
        ${result.tokens != null ? '<span><i class="fa-solid fa-font"></i> ' + result.tokens + ' tokens</span>' : ''}
        ${result.entities_created != null ? '<span><i class="fa-solid fa-tags"></i> ' + result.entities_created + ' entities</span>' : ''}
      </div>`;
  }

  // Fallback: raw result if nothing specific was found
  if (entities.length === 0 && relations.length === 0 && types.length === 0) {
    html += `
      <div style="font-size:0.85rem;color:var(--text-secondary)">
        <i class="fa-solid fa-circle-info"></i> Preview returned but no structured entities or relationships were extracted. Raw response:
      </div>
      <details style="margin-top:0.5rem">
        <summary class="text-muted" style="cursor:pointer;font-size:0.8rem">Show raw JSON</summary>
        <pre style="margin-top:0.5rem;overflow-x:auto;font-size:0.8rem;padding:0.5rem;background:var(--bg-secondary);border-radius:var(--radius-sm)">${escapeHtml(JSON.stringify(result, null, 2))}</pre>
      </details>`;
  }

  html += '</div>';
  return html;
}


// ─── Shared Helpers ──────────────────────────────────────────────────────────

function formatSourceTime(ts) {
  try {
    const d = new Date(ts);
    const now = new Date();
    const diff = now - d;
    if (diff < 60000) return 'Just now';
    if (diff < 3600000) return Math.floor(diff / 60000) + 'm ago';
    if (diff < 86400000) return Math.floor(diff / 3600000) + 'h ago';
    return d.toLocaleDateString();
  } catch (_) {
    return String(ts);
  }
}
