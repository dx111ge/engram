/* ============================================
   engram - System View
   Unified control panel: connection, embedding,
   LLM, NER, quantization, mesh, secrets, import/export
   ============================================ */

// Provider presets for quick configuration
const SYSTEM_EMBED_PROVIDERS = [
  { id: 'onnx',    label: 'ONNX Local',   endpoint: '',  models: [], local: true },
  { id: 'ollama',  label: 'Ollama',       endpoint: 'http://localhost:11434', models: ['nomic-embed-text', 'nomic-embed-text-v2-moe', 'mxbai-embed-large', 'all-minilm'] },
  { id: 'openai',  label: 'OpenAI',       endpoint: 'https://api.openai.com/v1', models: ['text-embedding-3-small', 'text-embedding-3-large', 'text-embedding-ada-002'] },
  { id: 'vllm',    label: 'vLLM',         endpoint: 'http://localhost:8000/v1', models: [] },
  { id: 'lmstudio', label: 'LM Studio',   endpoint: 'http://localhost:1234/v1', models: [] },
  { id: 'custom',  label: 'Custom',       endpoint: '', models: [] },
];

const SYSTEM_LLM_PROVIDERS = [
  { id: 'ollama',    label: 'Ollama',       endpoint: 'http://localhost:11434/v1', models: ['llama3.2', 'qwen2.5:7b', 'mistral', 'gemma2', 'phi3', 'deepseek-r1:8b'], canFetchModels: true, description: 'Local models via Ollama' },
  { id: 'lmstudio', label: 'LM Studio',    endpoint: 'http://localhost:1234/v1',  models: [], canFetchModels: true, description: 'Local models via LM Studio' },
  { id: 'vllm',     label: 'vLLM',         endpoint: 'http://localhost:8000/v1',  models: [], canFetchModels: true, description: 'High-performance local inference' },
  { id: 'openai',   label: 'OpenAI',       endpoint: 'https://api.openai.com/v1', models: ['gpt-4o', 'gpt-4o-mini', 'o3-mini'], needsKey: true, description: 'OpenAI API (requires API key)' },
  { id: 'google',   label: 'Google Gemini', endpoint: 'https://generativelanguage.googleapis.com/v1beta/openai', models: ['gemini-2.0-flash', 'gemini-2.5-flash', 'gemini-2.5-pro'], needsKey: true, description: 'Gemini via OpenAI-compatible endpoint' },
  { id: 'deepseek', label: 'DeepSeek',     endpoint: 'https://api.deepseek.com/v1', models: ['deepseek-chat', 'deepseek-reasoner'], needsKey: true, description: 'DeepSeek API (affordable reasoning)' },
  { id: 'openrouter', label: 'OpenRouter',  endpoint: 'https://openrouter.ai/api/v1', models: ['anthropic/claude-sonnet-4', 'google/gemini-2.5-flash', 'deepseek/deepseek-r1'], needsKey: true, description: 'Multi-provider gateway (Anthropic, Google, etc.)' },
  { id: 'custom',   label: 'Custom',       endpoint: '', models: [], description: 'Any OpenAI-compatible endpoint' },
];

const SYSTEM_NER_PROVIDERS = [
  { id: 'builtin', label: 'Built-in (Rule-based)', quality: 'Basic', description: 'Pattern matching, always available, no extra setup.' },
  { id: 'spacy',   label: 'spaCy',                 quality: 'Good',  description: 'Statistical NER, good accuracy. Requires spaCy service.' },
  { id: 'anno',    label: 'spaCy + Anno',           quality: 'Excellent', description: 'Best NER quality with learning. Requires Anno service.' },
];

// ── Section toggle logic ──

function toggleSection(name) {
  const section = document.getElementById('section-' + name);
  if (!section) return;
  section.classList.toggle('collapsed');
  const arrow = section.querySelector('.section-arrow');
  if (arrow) arrow.style.transform = section.classList.contains('collapsed') ? 'rotate(-90deg)' : '';
}

// ── Status dot helper ──

function systemStatusDot(type, label) {
  const colors = {
    active: { dot: 'var(--success)', text: 'var(--success)' },
    setup:  { dot: 'var(--accent-bright)', text: 'var(--accent-bright)' },
    error:  { dot: 'var(--error)', text: 'var(--error)' },
  };
  const c = colors[type] || colors.setup;
  return `<span class="feature-status ${type}" style="font-size:0.75rem;display:inline-flex;align-items:center;gap:0.35rem;color:${c.text}"><i class="fa-solid fa-circle" style="font-size:0.4rem;color:${c.dot}"></i> ${escapeHtml(label)}</span>`;
}

// ── Build section wrapper ──

function systemSection(id, icon, title, statusHtml, collapsed, bodyHtml) {
  return `
    <div class="system-section${collapsed ? ' collapsed' : ''}" id="section-${id}">
      <div class="section-toggle" onclick="toggleSection('${id}')">
        <div style="display:flex;align-items:center;gap:0.5rem">
          <i class="fa-solid ${icon}"></i>
          <span>${escapeHtml(title)}</span>
        </div>
        <div style="display:flex;align-items:center;gap:0.75rem">
          ${statusHtml}
          <i class="fa-solid fa-chevron-down section-arrow"${collapsed ? ' style="transform:rotate(-90deg)"' : ''}></i>
        </div>
      </div>
      <div class="section-body">
        ${bodyHtml}
      </div>
    </div>`;
}

// ══════════════════════════════════════════
//  Route
// ══════════════════════════════════════════

router.register('/system', async () => {
  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-sliders"></i> System</h1>
        <p class="text-secondary" style="margin-top:0.25rem">Control panel -- connection, models, mesh, secrets, data</p>
      </div>
    </div>
    <div id="system-content">${loadingHTML('Loading system configuration...')}</div>
  `);

  await loadSystemView();
});

// ══════════════════════════════════════════
//  Main loader
// ══════════════════════════════════════════

async function loadSystemView() {
  const container = document.getElementById('system-content');

  // Gather state in parallel
  let cfg = null, compute = null, secrets = [], meshIdentity = null, meshPeers = [];
  let meshEnabled = true, meshError = false;
  let onnxStatus = null;

  const gathers = [
    engram.getConfig().then(r => { cfg = r; }).catch(() => {}),
    engram.compute().then(r => { compute = r; }).catch(() => {}),
    engram._fetch('/config/onnx-model').then(r => { onnxStatus = r; }).catch(() => {}),
    engram.listSecrets().then(r => {
      secrets = Array.isArray(r) ? r : (r && r.keys ? r.keys : []);
    }).catch(() => {}),
    engram.meshIdentity().then(r => { meshIdentity = r; }).catch(err => {
      if (err.message && (err.message.includes('501') || err.message.includes('not enabled'))) {
        meshEnabled = false;
      } else {
        meshError = true;
      }
    }),
    engram.meshPeers().then(r => {
      meshPeers = Array.isArray(r) ? r : (r && r.peers ? r.peers : []);
    }).catch(() => {}),
  ];
  await Promise.allSettled(gathers);

  if (!cfg) cfg = {};

  // Derive statuses
  const apiBase = localStorage.getItem('engram_api') || '';
  const isConnected = compute !== null || cfg !== null;
  const hasOnnx = !!(onnxStatus && onnxStatus.ready);
  const hasEmbed = !!(cfg.embed_endpoint && cfg.embed_model) || hasOnnx;
  const hasLlm = !!(cfg.llm_endpoint && cfg.llm_model);
  const nerProvider = cfg.ner_provider || 'builtin';
  const quantActive = !!(compute && compute.quantization_enabled);
  const peerCount = meshPeers.length;
  const secretCount = secrets.length;

  let html = '';

  // ── 1. Connection ──
  html += systemSection('connection', 'fa-plug', 'Connection',
    isConnected ? systemStatusDot('active', 'Connected') : systemStatusDot('error', 'Offline'),
    false,
    buildConnectionSection(apiBase, isConnected)
  );

  // ── 2. Embedding Model ──
  const embedLabel = hasOnnx ? 'ONNX Local' : (cfg.embed_model || 'Active');
  html += systemSection('embedding', 'fa-cube', 'Embedding Model',
    hasEmbed
      ? systemStatusDot('active', escapeHtml(embedLabel))
      : systemStatusDot('setup', 'Not configured'),
    false,
    buildEmbeddingSection(cfg, compute)
  );

  // ── 3. Language Model ──
  html += systemSection('llm', 'fa-robot', 'Language Model',
    hasLlm
      ? systemStatusDot('active', escapeHtml(cfg.llm_model || 'Configured'))
      : systemStatusDot('setup', 'Not configured'),
    true,
    buildLlmSection(cfg)
  );

  // ── 4. NER / Entity Recognition ──
  const nerLabel = SYSTEM_NER_PROVIDERS.find(p => p.id === nerProvider)?.label || nerProvider;
  html += systemSection('ner', 'fa-tag', 'NER / Entity Recognition',
    systemStatusDot('active', nerLabel),
    true,
    buildNerSection(cfg)
  );

  // ── 5. Quantization ──
  html += systemSection('quantization', 'fa-compress', 'Quantization',
    quantActive
      ? systemStatusDot('active', 'Active')
      : systemStatusDot('setup', 'Off'),
    true,
    buildQuantizationSection(compute, quantActive)
  );

  // ── 6. Mesh Network ──
  let meshStatusHtml;
  if (!meshEnabled) {
    meshStatusHtml = systemStatusDot('setup', 'Not enabled');
  } else if (meshError) {
    meshStatusHtml = systemStatusDot('error', 'Error');
  } else {
    meshStatusHtml = systemStatusDot('active', peerCount + ' peer' + (peerCount !== 1 ? 's' : '') + ' connected');
  }
  html += systemSection('mesh', 'fa-network-wired', 'Mesh Network',
    meshStatusHtml,
    true,
    buildMeshSection(meshEnabled, meshIdentity, meshPeers)
  );

  // ── 7. Secrets ──
  html += systemSection('secrets', 'fa-key', 'Secrets',
    secretCount > 0
      ? systemStatusDot('active', secretCount + ' key' + (secretCount !== 1 ? 's' : ''))
      : systemStatusDot('setup', 'No secrets'),
    true,
    buildSecretsSection(secrets)
  );

  // ── 8. Import / Export ──
  html += systemSection('importexport', 'fa-file-export', 'Import / Export',
    systemStatusDot('active', 'Available'),
    true,
    buildImportExportSection()
  );

  container.innerHTML = html;

  // Bind all events after render
  bindSystemEvents(cfg, meshEnabled, meshPeers);
}

// ══════════════════════════════════════════
//  Section builders
// ══════════════════════════════════════════

// ── Connection ──

function buildConnectionSection(apiBase, isConnected) {
  return `
    <div class="form-group">
      <label>API Base URL</label>
      <input type="text" id="sys-api-base" placeholder="http://localhost:3030" value="${escapeHtml(apiBase || '')}">
    </div>
    <div id="sys-conn-status" style="margin-bottom:0.75rem">
      ${isConnected
        ? '<div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem"><i class="fa-solid fa-circle-check" style="color:var(--success)"></i> Connected</div>'
        : '<div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem"><i class="fa-solid fa-circle-xmark" style="color:var(--error)"></i> Offline</div>'
      }
    </div>
    <div style="display:flex;gap:0.5rem">
      <button class="btn btn-secondary" id="sys-conn-test">
        <i class="fa-solid fa-plug"></i> Test Connection
      </button>
      <button class="btn btn-primary" id="sys-conn-save">
        <i class="fa-solid fa-save"></i> Save
      </button>
    </div>
    <div id="sys-conn-result" class="mt-1"></div>`;
}

// ── Embedding Model ──

function buildEmbeddingSection(cfg, compute) {
  const dimValue = (compute && compute.embedder_dim) ? compute.embedder_dim : '';

  let lockWarningHtml = '';
  // Check if embedder is locked (data exists + model configured)
  if (cfg.embed_endpoint && cfg.embed_model) {
    lockWarningHtml = `
      <div style="display:flex;align-items:center;gap:0.5rem;padding:0.6rem 0.75rem;background:rgba(227,160,8,0.1);border:1px solid rgba(227,160,8,0.3);border-radius:var(--radius-sm);font-size:0.85rem;color:var(--confidence-mid);margin-bottom:0.75rem">
        <i class="fa-solid fa-lock" style="flex-shrink:0"></i>
        <span><strong>Embedder committed.</strong> Changing the embedding model will invalidate all vectors and require a full reindex.</span>
      </div>`;
  }

  return `
    ${lockWarningHtml}
    <div class="form-group">
      <label>Provider</label>
      <select id="sys-embed-provider" style="width:100%">
        ${SYSTEM_EMBED_PROVIDERS.map(p => `<option value="${p.id}">${escapeHtml(p.label)}</option>`).join('')}
      </select>
    </div>
    <div id="sys-embed-api-fields">
      <div class="form-group">
        <label>Endpoint URL</label>
        <input type="text" id="sys-embed-endpoint" placeholder="http://localhost:11434" value="${escapeHtml(cfg.embed_endpoint || '')}">
      </div>
      <div class="form-group">
        <label>Model Name</label>
        <div style="position:relative">
          <input type="text" id="sys-embed-model" placeholder="nomic-embed-text" list="sys-embed-model-list" value="${escapeHtml(cfg.embed_model || '')}">
          <datalist id="sys-embed-model-list"></datalist>
        </div>
      </div>
      <div id="sys-embed-dimensions" style="font-size:0.85rem;margin-bottom:0.75rem">
        ${dimValue ? `<div style="display:flex;align-items:center;gap:0.5rem;color:var(--text-secondary)"><i class="fa-solid fa-ruler-combined"></i> Dimensions: <strong>${dimValue}</strong></div>` : ''}
      </div>
      <div style="display:flex;gap:0.5rem;flex-wrap:wrap">
        <button class="btn btn-secondary" id="sys-embed-test">
          <i class="fa-solid fa-plug"></i> Test Connection
        </button>
        <button class="btn btn-primary" id="sys-embed-save">
          <i class="fa-solid fa-save"></i> Save
        </button>
        <button class="btn btn-secondary" id="sys-embed-reindex" title="Re-embed all nodes with the current model">
          <i class="fa-solid fa-arrows-rotate"></i> Reindex
        </button>
      </div>
    </div>
    <div id="sys-embed-onnx-fields" style="display:none">
      <div style="padding:0.75rem;background:var(--bg-elevated);border-radius:var(--radius-sm);margin-bottom:0.75rem;font-size:0.85rem">
        <p style="margin:0 0 0.5rem"><strong><i class="fa-solid fa-microchip"></i> Local ONNX Embedding</strong></p>
        <p style="margin:0 0 0.5rem;color:var(--text-secondary)">Run embeddings locally without any external service. Requires an ONNX model and tokenizer file.</p>
        <p style="margin:0 0 0.5rem;color:var(--text-secondary)">Download a model with ONNX weights and a <code>tokenizer.json</code> from HuggingFace.</p>
        <div style="display:flex;gap:0.5rem;flex-wrap:wrap;margin:0.5rem 0">
          <a href="https://huggingface.co/models?pipeline_tag=sentence-similarity&library=onnx&sort=trending" target="_blank" rel="noopener" class="btn btn-secondary" style="font-size:0.8rem;padding:0.25rem 0.6rem;text-decoration:none">
            <i class="fa-solid fa-magnifying-glass"></i> Browse Embedding Models
          </a>
        </div>
        <p style="margin:0 0 0.5rem;font-size:0.8rem;color:var(--text-muted)"><i class="fa-solid fa-star"></i> Suggestions: <a href="https://huggingface.co/intfloat/multilingual-e5-small/tree/main/onnx" target="_blank" rel="noopener" style="color:var(--accent-bright)">multilingual-e5-small</a> (384d, 120MB, 100+ langs), <a href="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/tree/main/onnx" target="_blank" rel="noopener" style="color:var(--accent-bright)">all-MiniLM-L6-v2</a> (384d, 90MB, English), <a href="https://huggingface.co/BAAI/bge-small-en-v1.5/tree/main/onnx" target="_blank" rel="noopener" style="color:var(--accent-bright)">bge-small-en-v1.5</a> (384d, 130MB, English)</p>
        <p style="margin:0;font-size:0.8rem;color:var(--text-muted)"><i class="fa-solid fa-scale-balanced"></i> Powered by <a href="https://github.com/microsoft/onnxruntime" target="_blank" rel="noopener" style="color:var(--accent-bright)">ONNX Runtime</a> (MIT License)</p>
      </div>
      <div id="sys-onnx-status" style="margin-bottom:0.75rem"></div>
      <div class="form-group">
        <label><i class="fa-solid fa-cube"></i> ONNX Model File (.onnx)</label>
        <input type="file" id="sys-onnx-model-file" accept=".onnx" style="font-size:0.85rem">
      </div>
      <div class="form-group">
        <label><i class="fa-solid fa-file-code"></i> Tokenizer File (.json)</label>
        <input type="file" id="sys-onnx-tokenizer-file" accept=".json" style="font-size:0.85rem">
      </div>
      <div style="display:flex;gap:0.5rem">
        <button class="btn btn-secondary" id="sys-onnx-check">
          <i class="fa-solid fa-magnifying-glass"></i> Check Files
        </button>
        <button class="btn btn-primary" id="sys-onnx-upload">
          <i class="fa-solid fa-upload"></i> Upload &amp; Install
        </button>
      </div>
    </div>
    <div id="sys-embed-result" class="mt-1"></div>`;
}

// ── Language Model ──

function buildLlmSection(cfg) {
  const tempVal = cfg.llm_temperature != null ? cfg.llm_temperature.toFixed(1) : '0.7';

  return `
    <div class="form-group">
      <label>Provider</label>
      <select id="sys-llm-provider" style="width:100%">
        ${SYSTEM_LLM_PROVIDERS.map(p => `<option value="${p.id}">${escapeHtml(p.label)}</option>`).join('')}
      </select>
      <div id="sys-llm-provider-desc" style="font-size:0.8rem;color:var(--text-muted);margin-top:0.25rem"></div>
    </div>
    <div class="form-group">
      <label>Endpoint URL</label>
      <input type="text" id="sys-llm-endpoint" placeholder="http://localhost:11434/v1" value="${escapeHtml(cfg.llm_endpoint || '')}">
      <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.2rem">
        <i class="fa-solid fa-info-circle"></i> Must be OpenAI-compatible (<code>/v1/chat/completions</code>).
        For Anthropic, use <a href="https://openrouter.ai" target="_blank" rel="noopener" style="color:var(--accent-bright)">OpenRouter</a> or a local proxy.
      </div>
    </div>
    <div class="form-group">
      <label>Model Name</label>
      <div style="display:flex;gap:0.5rem;align-items:center">
        <input type="text" id="sys-llm-model" placeholder="llama3.2" list="sys-llm-model-list" value="${escapeHtml(cfg.llm_model || '')}" style="flex:1">
        <button class="btn btn-secondary" id="sys-llm-fetch-models" style="white-space:nowrap;font-size:0.8rem;padding:0.3rem 0.6rem;display:none" title="Fetch available models from endpoint">
          <i class="fa-solid fa-download"></i> Fetch
        </button>
        <datalist id="sys-llm-model-list"></datalist>
      </div>
      <select id="sys-llm-model-select" style="display:none;margin-top:0.35rem;width:100%"></select>
      <div id="sys-llm-models-status" style="font-size:0.8rem;margin-top:0.25rem"></div>
    </div>
    <div class="form-group" id="sys-llm-key-group">
      <label><i class="fa-solid fa-key"></i> API Key</label>
      <div style="position:relative">
        <input type="password" id="sys-llm-api-key" placeholder="Stored encrypted -- leave blank to keep">
        <span id="sys-llm-key-indicator" style="position:absolute;right:0.75rem;top:50%;transform:translateY(-50%);font-size:0.8rem"></span>
      </div>
    </div>
    <div class="form-group">
      <label style="display:flex;justify-content:space-between;align-items:center">
        <span>Temperature</span>
        <span id="sys-llm-temp-value" style="font-family:var(--font-mono);font-size:0.85rem;color:var(--text-secondary)">${tempVal}</span>
      </label>
      <input type="range" id="sys-llm-temperature" min="0" max="2" step="0.1" value="${tempVal}" style="width:100%;cursor:pointer">
      <div id="sys-llm-temp-hint" style="font-size:0.75rem;color:var(--text-muted);margin-top:0.2rem">
        Lower = more focused, higher = more creative. Ignored for thinking models.
      </div>
    </div>
    <div class="form-group">
      <label style="display:flex;align-items:center;gap:0.5rem;cursor:pointer">
        <input type="checkbox" id="sys-llm-thinking" style="width:auto;margin:0">
        <span><i class="fa-solid fa-brain"></i> Thinking / Reasoning Model</span>
      </label>
      <div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.2rem">
        Enable for models like DeepSeek-R1, QwQ, o3-mini that use chain-of-thought reasoning. Temperature is ignored; responses may be longer.
      </div>
    </div>
    <div style="display:flex;gap:0.5rem">
      <button class="btn btn-secondary" id="sys-llm-test">
        <i class="fa-solid fa-plug"></i> Test Connection
      </button>
      <button class="btn btn-primary" id="sys-llm-save">
        <i class="fa-solid fa-save"></i> Save
      </button>
    </div>
    <div id="sys-llm-result" class="mt-1"></div>`;
}

// ── NER / Entity Recognition ──

function buildNerSection(cfg) {
  const nerProvider = cfg.ner_provider || 'builtin';

  return `
    <div class="form-group">
      <label>NER Provider</label>
      <div id="sys-ner-provider-cards" style="display:flex;flex-direction:column;gap:0.5rem">
        ${SYSTEM_NER_PROVIDERS.map(p => `
          <label style="display:flex;align-items:flex-start;gap:0.6rem;padding:0.6rem 0.75rem;background:var(--bg-secondary);border:2px solid ${p.id === nerProvider ? 'var(--accent-bright)' : 'var(--border)'};border-radius:var(--radius-sm);cursor:pointer;transition:border-color 0.15s" data-provider="${p.id}">
            <input type="radio" name="sys-ner-provider" value="${p.id}" style="margin-top:0.2rem;flex-shrink:0"${p.id === nerProvider ? ' checked' : ''}>
            <div style="flex:1">
              <div style="font-weight:600;font-size:0.9rem">${escapeHtml(p.label)}
                <span style="font-size:0.75rem;padding:0.1rem 0.35rem;border-radius:999px;margin-left:0.4rem;background:${p.quality === 'Excellent' ? 'rgba(46,160,67,0.15)' : p.quality === 'Good' ? 'rgba(227,160,8,0.15)' : 'var(--bg-input)'};color:${p.quality === 'Excellent' ? 'var(--success)' : p.quality === 'Good' ? 'var(--confidence-mid)' : 'var(--text-muted)'}">${p.quality}</span>
              </div>
              <div style="font-size:0.8rem;color:var(--text-muted)">${escapeHtml(p.description)}</div>
            </div>
          </label>
        `).join('')}
      </div>
    </div>
    <div id="sys-ner-endpoint-group" class="form-group" style="display:${nerProvider === 'spacy' || nerProvider === 'anno' ? '' : 'none'}">
      <label>NER Service Endpoint</label>
      <input type="text" id="sys-ner-endpoint" placeholder="http://localhost:5000" value="${escapeHtml(cfg.ner_endpoint || '')}">
    </div>
    <div id="sys-ner-model-group" class="form-group" style="display:${nerProvider === 'spacy' || nerProvider === 'anno' ? '' : 'none'}">
      <label>Model Name</label>
      <input type="text" id="sys-ner-model" placeholder="en_core_web_sm" value="${escapeHtml(cfg.ner_model || '')}">
    </div>
    <div style="display:flex;gap:0.5rem">
      <button class="btn btn-primary" id="sys-ner-save">
        <i class="fa-solid fa-save"></i> Save NER Config
      </button>
    </div>
    <div id="sys-ner-result" class="mt-1"></div>`;
}

// ── Quantization ──

function buildQuantizationSection(compute, quantActive) {
  const vectorCount = (compute && compute.vector_count != null) ? compute.vector_count : '--';
  const memUsage = (compute && compute.vector_memory) ? compute.vector_memory : '--';

  return `
    <div style="display:flex;flex-direction:column;gap:0.75rem;margin-bottom:1rem">
      <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
        <i class="fa-solid fa-microchip" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
        <span style="font-size:0.85rem;color:var(--text-secondary);min-width:120px">Int8 Quantization</span>
        <span style="font-size:0.9rem;font-weight:600;color:${quantActive ? 'var(--success)' : 'var(--text-muted)'}">
          ${quantActive ? 'Active (4x memory reduction)' : 'Off'}
        </span>
      </div>
      <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
        <i class="fa-solid fa-database" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
        <span style="font-size:0.85rem;color:var(--text-secondary);min-width:120px">Vectors</span>
        <span style="font-size:0.9rem">${vectorCount}</span>
      </div>
      <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
        <i class="fa-solid fa-memory" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
        <span style="font-size:0.85rem;color:var(--text-secondary);min-width:120px">Memory Usage</span>
        <span style="font-size:0.9rem">${memUsage}</span>
      </div>
    </div>
    <div style="display:flex;gap:0.5rem">
      <button class="btn ${quantActive ? 'btn-secondary' : 'btn-primary'}" id="sys-quant-toggle">
        <i class="fa-solid ${quantActive ? 'fa-toggle-off' : 'fa-toggle-on'}"></i>
        ${quantActive ? 'Disable Quantization' : 'Enable Quantization'}
      </button>
    </div>
    <div id="sys-quant-result" class="mt-1"></div>`;
}

// ── Mesh Network ──

function buildMeshSection(meshEnabled, identity, peers) {
  if (!meshEnabled) {
    return buildMeshDisabledContent();
  }
  return buildMeshEnabledContent(identity, peers);
}

function buildMeshDisabledContent() {
  return `
    <div style="text-align:center;padding:1.5rem 0">
      <i class="fa-solid fa-network-wired" style="font-size:2.5rem;color:var(--text-muted);margin-bottom:0.75rem"></i>
      <h3 style="margin-bottom:0.5rem">Mesh Networking Not Enabled</h3>
      <p style="color:var(--text-secondary);max-width:500px;margin:0 auto 1.5rem;font-size:0.9rem">
        Mesh networking allows engram instances to sync knowledge, creating a distributed knowledge graph.
      </p>
    </div>
    <div style="display:flex;flex-direction:column;gap:0.75rem;margin-bottom:1rem">
      <div style="display:flex;gap:0.75rem;align-items:flex-start;font-size:0.9rem;color:var(--text-secondary)">
        <i class="fa-solid fa-arrows-rotate" style="color:var(--accent-bright);margin-top:0.15rem;flex-shrink:0"></i>
        <span>Sync facts, relationships, and confidence scores between instances</span>
      </div>
      <div style="display:flex;gap:0.75rem;align-items:flex-start;font-size:0.9rem;color:var(--text-secondary)">
        <i class="fa-solid fa-shield-halved" style="color:var(--accent-bright);margin-top:0.15rem;flex-shrink:0"></i>
        <span>Zero-trust security with ed25519 identity and topic-level ACLs</span>
      </div>
      <div style="display:flex;gap:0.75rem;align-items:flex-start;font-size:0.9rem;color:var(--text-secondary)">
        <i class="fa-solid fa-magnifying-glass" style="color:var(--accent-bright);margin-top:0.15rem;flex-shrink:0"></i>
        <span>Federated queries across the mesh for distributed knowledge search</span>
      </div>
    </div>
    <div style="font-size:0.8rem;color:var(--text-muted);text-transform:uppercase;letter-spacing:0.05em;margin-bottom:0.5rem">Topology</div>
    <div style="display:flex;flex-direction:column;gap:0.5rem;margin-bottom:1rem">
      <label style="display:flex;gap:0.5rem;align-items:center;cursor:pointer">
        <input type="radio" name="sys-mesh-topology" value="star" checked style="accent-color:var(--accent-bright)">
        <span><strong>Star</strong> -- one hub, many spokes. Simple, centralized sync.</span>
      </label>
      <label style="display:flex;gap:0.5rem;align-items:center;cursor:pointer">
        <input type="radio" name="sys-mesh-topology" value="full" style="accent-color:var(--accent-bright)">
        <span><strong>Full mesh</strong> -- every node connects to every other.</span>
      </label>
      <label style="display:flex;gap:0.5rem;align-items:center;cursor:pointer">
        <input type="radio" name="sys-mesh-topology" value="ring" style="accent-color:var(--accent-bright)">
        <span><strong>Ring</strong> -- each node syncs with two neighbors.</span>
      </label>
    </div>
    <button class="btn btn-primary" id="sys-mesh-enable-btn">
      <i class="fa-solid fa-power-off"></i> Enable Mesh
    </button>
    <p style="margin-top:0.75rem;font-size:0.8rem;color:var(--text-muted)">
      <i class="fa-solid fa-circle-info" style="margin-right:0.25rem"></i>
      After enabling, restart the engram server to activate mesh endpoints.
    </p>`;
}

function buildMeshEnabledContent(identity, peers) {
  const pubKey = identity
    ? (identity.public_key || identity.id || (typeof identity === 'string' ? identity : JSON.stringify(identity)))
    : 'Unknown';
  const shortKey = pubKey.length > 24 ? pubKey.substring(0, 24) + '...' : pubKey;

  let html = '';

  // Identity
  html += `
    <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:1rem;margin-bottom:1rem">
      <div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem">
        <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.75rem"><i class="fa-solid fa-fingerprint" style="margin-right:0.4rem"></i> Identity</div>
        <div style="display:flex;flex-direction:column;gap:0.5rem">
          <div style="display:flex;align-items:center;gap:0.5rem">
            <i class="fa-solid fa-key" style="color:var(--accent-bright);width:16px;text-align:center"></i>
            <span style="font-size:0.8rem;color:var(--text-secondary);min-width:70px">Public Key</span>
            <span style="font-size:0.85rem;font-family:var(--font-mono);word-break:break-all" title="${escapeHtml(pubKey)}">${escapeHtml(shortKey)}</span>
          </div>
          <div style="display:flex;align-items:center;gap:0.5rem">
            <i class="fa-solid fa-signal" style="color:var(--success);width:16px;text-align:center"></i>
            <span style="font-size:0.8rem;color:var(--text-secondary);min-width:70px">Status</span>
            <span style="font-size:0.85rem;color:var(--success)"><i class="fa-solid fa-circle" style="font-size:0.4rem;vertical-align:middle"></i> Online</span>
          </div>
        </div>
      </div>

      <div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem">
        <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.75rem"><i class="fa-solid fa-user-plus" style="margin-right:0.4rem"></i> Add Peer</div>
        <div style="display:flex;flex-direction:column;gap:0.5rem">
          <input type="text" id="sys-mesh-peer-endpoint" placeholder="http://host:3030" style="width:100%;padding:0.4rem 0.6rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font-size:0.85rem">
          <input type="text" id="sys-mesh-peer-name" placeholder="peer-name" style="width:100%;padding:0.4rem 0.6rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font-size:0.85rem">
          <div>
            <label style="font-size:0.8rem;color:var(--text-secondary)">Trust: <span id="sys-mesh-trust-value">0.7</span></label>
            <input type="range" id="sys-mesh-peer-trust" min="0" max="1" step="0.05" value="0.7" style="width:100%;accent-color:var(--accent-bright)">
          </div>
          <button class="btn btn-primary" id="sys-mesh-add-peer-btn" style="width:100%">
            <i class="fa-solid fa-plus"></i> Add Peer
          </button>
        </div>
      </div>
    </div>`;

  // Peers table
  html += `
    <div style="margin-bottom:1rem">
      <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:0.5rem">
        <div style="font-weight:600;font-size:0.85rem"><i class="fa-solid fa-users" style="margin-right:0.4rem"></i> Connected Peers</div>
        <button id="sys-mesh-refresh-btn" style="padding:0.3rem 0.6rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-secondary);cursor:pointer;font-size:0.8rem;display:flex;align-items:center;gap:0.3rem">
          <i class="fa-solid fa-arrows-rotate"></i> Refresh
        </button>
      </div>
      <div id="sys-mesh-peers-table">
        ${systemRenderPeersTable(peers)}
      </div>
    </div>`;

  // Discovery + Sync
  html += `
    <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:1rem">
      <div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem">
        <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.75rem"><i class="fa-solid fa-satellite-dish" style="margin-right:0.4rem"></i> Discovery</div>
        <div style="display:flex;gap:0.5rem;margin-bottom:0.5rem">
          <input type="text" id="sys-mesh-discover-topic" placeholder="topic..." style="flex:1;padding:0.4rem 0.6rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font-size:0.85rem">
          <button class="btn btn-primary" id="sys-mesh-discover-btn" style="white-space:nowrap">
            <i class="fa-solid fa-search"></i> Discover
          </button>
        </div>
        <div id="sys-mesh-discover-results" style="font-size:0.9rem;color:var(--text-secondary)"></div>
      </div>

      <div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem">
        <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.75rem"><i class="fa-solid fa-rotate" style="margin-right:0.4rem"></i> Sync Status</div>
        <div id="sys-mesh-sync-status" style="display:flex;flex-direction:column;gap:0.5rem">
          <div style="display:flex;align-items:center;gap:0.5rem">
            <i class="fa-solid fa-clock" style="color:var(--accent-bright);width:16px;text-align:center"></i>
            <span style="font-size:0.8rem;color:var(--text-secondary);min-width:70px">Last sync</span>
            <span style="font-size:0.85rem" id="sys-mesh-last-sync">--</span>
          </div>
          <div style="display:flex;align-items:center;gap:0.5rem">
            <i class="fa-solid fa-list-check" style="color:var(--accent-bright);width:16px;text-align:center"></i>
            <span style="font-size:0.8rem;color:var(--text-secondary);min-width:70px">Pending</span>
            <span style="font-size:0.85rem" id="sys-mesh-pending">0 deltas</span>
          </div>
          <div style="display:flex;align-items:center;gap:0.5rem">
            <i class="fa-solid fa-triangle-exclamation" style="color:var(--accent-bright);width:16px;text-align:center"></i>
            <span style="font-size:0.8rem;color:var(--text-secondary);min-width:70px">Conflicts</span>
            <span style="font-size:0.85rem" id="sys-mesh-conflicts">0</span>
          </div>
          <button class="btn btn-secondary" id="sys-mesh-audit-btn" style="margin-top:0.25rem">
            <i class="fa-solid fa-scroll"></i> View Audit Log
          </button>
        </div>
      </div>
    </div>
    <div id="sys-mesh-audit-panel" style="display:none;margin-top:1rem"></div>`;

  return html;
}

function systemRenderPeersTable(peers) {
  if (!peers || peers.length === 0) {
    return '<div style="padding:1rem;text-align:center;color:var(--text-muted);font-size:0.85rem"><i class="fa-solid fa-users-slash" style="margin-right:0.4rem"></i> No peers connected yet. Add a peer above to start syncing.</div>';
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
              <div style="width:${trustPct}%;height:100%;background:${typeof confidenceColor === 'function' ? confidenceColor(trust) : 'var(--accent-bright)'};border-radius:3px"></div>
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
          <button class="sys-mesh-remove-peer" data-key="${peerKey}" style="padding:0.25rem 0.5rem;background:none;border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-muted);cursor:pointer;font-size:0.8rem;display:flex;align-items:center;gap:0.3rem" title="Remove peer">
            <i class="fa-solid fa-trash-can"></i> Remove
          </button>
        </td>
      </tr>`;
  }

  return `
    <div class="table-wrap">
      <table>
        <thead>
          <tr><th>Name</th><th>Endpoint</th><th>Trust</th><th>Status</th><th>Actions</th></tr>
        </thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

// ── Secrets ──

function buildSecretsSection(secrets) {
  let listHtml = '';
  if (secrets.length === 0) {
    listHtml = '<div style="padding:0.75rem;color:var(--text-muted);font-size:0.85rem"><i class="fa-solid fa-circle-info" style="margin-right:0.3rem"></i> No secrets stored.</div>';
  } else {
    listHtml = '<div id="sys-secrets-list" style="display:flex;flex-direction:column;gap:0.3rem;margin-bottom:1rem">';
    for (const key of secrets) {
      const keyName = typeof key === 'string' ? key : (key.key || key.name || key.id || JSON.stringify(key));
      listHtml += `
        <div style="display:flex;justify-content:space-between;align-items:center;padding:0.5rem 0.75rem;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm)">
          <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.85rem">
            <i class="fa-solid fa-lock" style="color:var(--accent-bright)"></i>
            <span style="font-family:var(--font-mono)">${escapeHtml(keyName)}</span>
          </div>
          <button class="sys-secret-delete btn btn-danger" data-key="${escapeHtml(keyName)}" style="padding:0.2rem 0.5rem;font-size:0.8rem">
            <i class="fa-solid fa-trash-can"></i>
          </button>
        </div>`;
    }
    listHtml += '</div>';
  }

  return `
    ${listHtml}
    <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.5rem"><i class="fa-solid fa-plus" style="margin-right:0.3rem"></i> Add Secret</div>
    <div class="form-group">
      <label>Key</label>
      <input type="text" id="sys-secret-key" placeholder="e.g. api.openai_key">
    </div>
    <div class="form-group">
      <label>Value</label>
      <input type="password" id="sys-secret-value" placeholder="Secret value (stored encrypted)">
    </div>
    <button class="btn btn-primary" id="sys-secret-save">
      <i class="fa-solid fa-save"></i> Save Secret
    </button>
    <div id="sys-secret-result" class="mt-1"></div>`;
}

// ── Import / Export ──

function buildImportExportSection() {
  return `
    <div style="margin-bottom:1.5rem">
      <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.5rem"><i class="fa-solid fa-download" style="margin-right:0.3rem"></i> Export</div>
      <p style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.75rem">
        Export your knowledge base as JSON-LD for backup or sharing.
      </p>
      <button class="btn btn-primary" id="sys-export-jsonld">
        <i class="fa-solid fa-download"></i> Export as JSON-LD
      </button>
      <div id="sys-export-result" class="mt-1"></div>
    </div>
    <div>
      <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.5rem"><i class="fa-solid fa-upload" style="margin-right:0.3rem"></i> Import</div>
      <div class="form-group">
        <label>Paste JSON-LD data</label>
        <textarea id="sys-import-jsonld" rows="4" placeholder="Paste JSON-LD data here..."></textarea>
      </div>
      <button class="btn btn-primary" id="sys-import-jsonld-btn">
        <i class="fa-solid fa-upload"></i> Import
      </button>
      <div id="sys-import-result" class="mt-1"></div>
    </div>`;
}

// ══════════════════════════════════════════
//  Event bindings
// ══════════════════════════════════════════

function bindSystemEvents(cfg, meshEnabled, meshPeers) {
  bindConnectionEvents();
  bindEmbeddingEvents(cfg);
  bindLlmEvents(cfg);
  bindNerEvents();
  bindQuantizationEvents();
  bindMeshEvents_system(meshEnabled, meshPeers);
  bindSecretsEvents();
  bindImportExportEvents();
}

// ── Connection events ──

function bindConnectionEvents() {
  const testBtn = document.getElementById('sys-conn-test');
  const saveBtn = document.getElementById('sys-conn-save');

  if (testBtn) {
    testBtn.addEventListener('click', async () => {
      const base = document.getElementById('sys-api-base').value.trim();
      const resultDiv = document.getElementById('sys-conn-result');
      const statusDiv = document.getElementById('sys-conn-status');
      testBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Testing connection...');

      try {
        // Temporarily set and test
        const oldBase = localStorage.getItem('engram_api');
        if (base) localStorage.setItem('engram_api', base);
        await engram.health();
        resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Connection successful.</div>';
        statusDiv.innerHTML = '<div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem"><i class="fa-solid fa-circle-check" style="color:var(--success)"></i> Connected</div>';
        showToast('Connection verified', 'success');
        // Restore if user didn't save
        if (oldBase !== null && !base) localStorage.setItem('engram_api', oldBase);
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        statusDiv.innerHTML = '<div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem"><i class="fa-solid fa-circle-xmark" style="color:var(--error)"></i> Offline</div>';
        showToast('Connection failed', 'error');
      } finally {
        testBtn.disabled = false;
      }
    });
  }

  if (saveBtn) {
    saveBtn.addEventListener('click', () => {
      const base = document.getElementById('sys-api-base').value.trim();
      localStorage.setItem('engram_api', base);
      showToast('API base URL saved', 'success');
      document.getElementById('sys-conn-result').innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Saved. Reload the page to apply.</div>';
    });
  }
}

// ── Embedding events ──

function bindEmbeddingEvents(cfg) {
  const providerSelect = document.getElementById('sys-embed-provider');
  const apiFields = document.getElementById('sys-embed-api-fields');
  const onnxFields = document.getElementById('sys-embed-onnx-fields');

  function toggleEmbedFields(providerId) {
    const isOnnx = providerId === 'onnx';
    if (apiFields) apiFields.style.display = isOnnx ? 'none' : '';
    if (onnxFields) onnxFields.style.display = isOnnx ? '' : 'none';
    if (isOnnx) checkOnnxStatus();
  }

  // Auto-detect current provider from endpoint
  if (providerSelect) {
    if (cfg.embed_endpoint) {
      const matched = SYSTEM_EMBED_PROVIDERS.find(p => p.id !== 'custom' && p.id !== 'onnx' && p.endpoint && cfg.embed_endpoint.includes(p.endpoint.replace(/\/v1$/, '')));
      providerSelect.value = matched ? matched.id : 'custom';
    } else {
      // No endpoint -- could be ONNX or unconfigured. Check ONNX status.
      // Default to first API provider for now; ONNX will be selected by user.
    }
    toggleEmbedFields(providerSelect.value);
    systemUpdateEmbedSuggestions();
  }

  if (providerSelect) {
    providerSelect.addEventListener('change', (e) => {
      const preset = SYSTEM_EMBED_PROVIDERS.find(p => p.id === e.target.value);
      if (preset && preset.endpoint) {
        document.getElementById('sys-embed-endpoint').value = preset.endpoint;
      }
      toggleEmbedFields(e.target.value);
      systemUpdateEmbedSuggestions();
    });
  }

  // ── API provider: Test ──
  const testBtn = document.getElementById('sys-embed-test');
  if (testBtn) {
    testBtn.addEventListener('click', async () => {
      const endpoint = document.getElementById('sys-embed-endpoint').value.trim();
      const model = document.getElementById('sys-embed-model').value.trim();
      const resultDiv = document.getElementById('sys-embed-result');

      if (!endpoint || !model) { showToast('Enter both endpoint URL and model name', 'error'); return; }

      testBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Testing connection...');

      try {
        await engram.setConfig({ embed_endpoint: endpoint, embed_model: model });
        const compute = await engram.compute();
        let dimText = '';
        if (compute && compute.embedder_dim) {
          dimText = ' Dimensions: ' + compute.embedder_dim;
          const dimDiv = document.getElementById('sys-embed-dimensions');
          if (dimDiv) dimDiv.innerHTML = `<div style="display:flex;align-items:center;gap:0.5rem;color:var(--text-secondary)"><i class="fa-solid fa-ruler-combined"></i> Dimensions: <strong>${compute.embedder_dim}</strong></div>`;
        }
        resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Connection successful.${dimText}</div>`;
        showToast('Embedding connection verified', 'success');
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Embedding test failed', 'error');
      } finally {
        testBtn.disabled = false;
      }
    });
  }

  // ── API provider: Save ──
  const saveBtn = document.getElementById('sys-embed-save');
  if (saveBtn) {
    saveBtn.addEventListener('click', async () => {
      const endpoint = document.getElementById('sys-embed-endpoint').value.trim();
      const model = document.getElementById('sys-embed-model').value.trim();
      const resultDiv = document.getElementById('sys-embed-result');

      if (!endpoint || !model) { showToast('Enter both endpoint URL and model name', 'error'); return; }

      saveBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Saving embedding configuration...');

      try {
        await engram.setConfig({ embed_endpoint: endpoint, embed_model: model });
        let dimText = '';
        try {
          const compute = await engram.compute();
          if (compute && compute.embedder_dim) dimText = ' Detected dimensions: ' + compute.embedder_dim;
        } catch (_) {}
        resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Embedding configuration saved.${dimText}</div>`;
        showToast('Embedding configuration saved', 'success');
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Save failed', 'error');
      } finally {
        saveBtn.disabled = false;
      }
    });
  }

  // ── Reindex ──
  const reindexBtn = document.getElementById('sys-embed-reindex');
  if (reindexBtn) {
    reindexBtn.addEventListener('click', () => {
      const resultDiv = document.getElementById('sys-embed-result');
      // Inline confirmation instead of ugly native confirm()
      resultDiv.innerHTML = `
        <div style="display:flex;align-items:center;gap:0.5rem;padding:0.6rem 0.75rem;background:rgba(227,160,8,0.1);border:1px solid rgba(227,160,8,0.3);border-radius:var(--radius-sm);font-size:0.85rem;color:var(--confidence-mid)">
          <i class="fa-solid fa-triangle-exclamation" style="flex-shrink:0"></i>
          <span>Re-embed all nodes? This may take a while for large graphs.</span>
          <button class="btn btn-primary" id="sys-reindex-confirm" style="margin-left:auto;padding:0.25rem 0.75rem;font-size:0.8rem">
            <i class="fa-solid fa-check"></i> Confirm
          </button>
          <button class="btn btn-secondary" id="sys-reindex-cancel" style="padding:0.25rem 0.75rem;font-size:0.8rem">
            Cancel
          </button>
        </div>`;
      document.getElementById('sys-reindex-cancel').addEventListener('click', () => { resultDiv.innerHTML = ''; });
      document.getElementById('sys-reindex-confirm').addEventListener('click', async () => {
        reindexBtn.disabled = true;
        resultDiv.innerHTML = loadingHTML('Reindexing all nodes...');
        try {
          const resp = await engram._post('/reindex', {});
          resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Reindex complete. ${resp.reindexed} nodes re-embedded.</div>`;
          showToast(`Reindexed ${resp.reindexed} nodes`, 'success');
        } catch (err) {
          resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> Reindex failed: ${escapeHtml(err.message)}</div>`;
          showToast('Reindex failed', 'error');
        } finally {
          reindexBtn.disabled = false;
        }
      });
    });
  }

  // ── ONNX: Check files ──
  const onnxCheckBtn = document.getElementById('sys-onnx-check');
  if (onnxCheckBtn) {
    onnxCheckBtn.addEventListener('click', checkOnnxStatus);
  }

  // ── ONNX: Upload ──
  const onnxUploadBtn = document.getElementById('sys-onnx-upload');
  if (onnxUploadBtn) {
    onnxUploadBtn.addEventListener('click', async () => {
      const modelFile = document.getElementById('sys-onnx-model-file').files[0];
      const tokenizerFile = document.getElementById('sys-onnx-tokenizer-file').files[0];
      const resultDiv = document.getElementById('sys-embed-result');

      if (!modelFile && !tokenizerFile) {
        showToast('Select at least one file to upload', 'error');
        return;
      }

      onnxUploadBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Uploading ONNX files... (this may take a moment for large models)');

      try {
        const form = new FormData();
        if (modelFile) form.append('model', modelFile);
        if (tokenizerFile) form.append('tokenizer', tokenizerFile);
        const base = localStorage.getItem('engram_api_base') || 'http://localhost:3030';
        const raw = await fetch(base + '/config/onnx-model', { method: 'POST', body: form });
        if (!raw.ok) { const e = await raw.json().catch(() => ({})); throw new Error(e.error || raw.statusText); }
        const resp = await raw.json();
        const sizeText = resp.model_size_mb ? ` (${resp.model_size_mb.toFixed(1)} MB)` : '';
        if (resp.activated) {
          resultDiv.innerHTML = `
            <div style="color:var(--success);font-size:0.85rem;margin-bottom:0.5rem"><i class="fa-solid fa-check"></i> ONNX embedder activated${sizeText}.</div>
            <button class="btn btn-primary" id="sys-onnx-reindex-now">
              <i class="fa-solid fa-arrows-rotate"></i> Reindex Now
            </button>`;
          document.getElementById('sys-onnx-reindex-now').addEventListener('click', async () => {
            const btn = document.getElementById('sys-onnx-reindex-now');
            btn.disabled = true;
            resultDiv.innerHTML = loadingHTML('Reindexing all nodes with ONNX embedder...');
            try {
              const r = await engram._post('/reindex', {});
              resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Reindex complete. ${r.reindexed} nodes re-embedded with ONNX.</div>`;
              showToast(`Reindexed ${r.reindexed} nodes`, 'success');
            } catch (e) {
              resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> Reindex failed: ${escapeHtml(e.message)}</div>`;
            }
          });
          showToast('ONNX embedder activated', 'success');
        } else {
          resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> ${escapeHtml(resp.message)}${sizeText}</div>`;
          showToast('ONNX files uploaded', 'success');
        }
        checkOnnxStatus();
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Upload failed', 'error');
      } finally {
        onnxUploadBtn.disabled = false;
      }
    });
  }
}

async function checkOnnxStatus() {
  const statusDiv = document.getElementById('sys-onnx-status');
  if (!statusDiv) return;
  try {
    const resp = await engram._fetch('/config/onnx-model');
    if (resp.ready) {
      const sizeText = resp.model_size_mb ? ` (${resp.model_size_mb.toFixed(1)} MB)` : '';
      statusDiv.innerHTML = `
        <div style="display:flex;align-items:center;gap:0.5rem;padding:0.5rem 0.75rem;background:rgba(72,199,142,0.1);border:1px solid rgba(72,199,142,0.3);border-radius:var(--radius-sm);font-size:0.85rem;color:var(--success)">
          <i class="fa-solid fa-circle-check"></i>
          <span><strong>ONNX model installed</strong>${sizeText}. Ready to use.</span>
        </div>`;
    } else {
      const modelIcon = resp.model_exists ? '<i class="fa-solid fa-check" style="color:var(--success)"></i>' : '<i class="fa-solid fa-xmark" style="color:var(--error)"></i>';
      const tokIcon = resp.tokenizer_exists ? '<i class="fa-solid fa-check" style="color:var(--success)"></i>' : '<i class="fa-solid fa-xmark" style="color:var(--error)"></i>';
      statusDiv.innerHTML = `
        <div style="padding:0.5rem 0.75rem;background:var(--bg-elevated);border-radius:var(--radius-sm);font-size:0.85rem">
          <div>${modelIcon} model.onnx ${resp.model_exists ? '<span style="color:var(--text-muted)">(found)</span>' : '<span style="color:var(--text-muted)">(missing)</span>'}</div>
          <div>${tokIcon} tokenizer.json ${resp.tokenizer_exists ? '<span style="color:var(--text-muted)">(found)</span>' : '<span style="color:var(--text-muted)">(missing)</span>'}</div>
        </div>`;
    }
  } catch (err) {
    statusDiv.innerHTML = `<div style="color:var(--text-muted);font-size:0.85rem"><i class="fa-solid fa-info-circle"></i> Upload model and tokenizer files below.</div>`;
  }
}

function systemUpdateEmbedSuggestions() {
  const provider = document.getElementById('sys-embed-provider').value;
  const preset = SYSTEM_EMBED_PROVIDERS.find(p => p.id === provider);
  const datalist = document.getElementById('sys-embed-model-list');
  if (datalist) datalist.innerHTML = (preset?.models || []).map(m => `<option value="${m}">`).join('');
}

// ── LLM events ──

function bindLlmEvents(cfg) {
  const providerSelect = document.getElementById('sys-llm-provider');

  // Auto-detect provider from saved endpoint
  if (providerSelect && cfg.llm_endpoint) {
    const matched = SYSTEM_LLM_PROVIDERS.find(p => p.id !== 'custom' && p.endpoint && cfg.llm_endpoint.includes(p.endpoint.replace(/\/v1$/, '').replace(/\/v1beta\/openai$/, '')));
    providerSelect.value = matched ? matched.id : 'custom';
  }
  systemUpdateLlmUI();

  // Restore thinking model toggle from config
  const thinkingCheckbox = document.getElementById('sys-llm-thinking');
  if (thinkingCheckbox && cfg.llm_thinking) {
    thinkingCheckbox.checked = true;
  }

  // LLM API key indicator
  systemLoadLlmKeyIndicator(cfg);

  if (providerSelect) {
    providerSelect.addEventListener('change', (e) => {
      const preset = SYSTEM_LLM_PROVIDERS.find(p => p.id === e.target.value);
      if (preset && preset.endpoint) {
        document.getElementById('sys-llm-endpoint').value = preset.endpoint;
      }
      systemUpdateLlmUI();
    });
  }

  // Temperature slider
  const tempSlider = document.getElementById('sys-llm-temperature');
  const tempLabel = document.getElementById('sys-llm-temp-value');
  if (tempSlider && tempLabel) {
    tempSlider.addEventListener('input', () => {
      tempLabel.textContent = parseFloat(tempSlider.value).toFixed(1);
    });
  }

  // Thinking toggle dims the temperature slider
  if (thinkingCheckbox) {
    thinkingCheckbox.addEventListener('change', () => {
      const tempGroup = document.getElementById('sys-llm-temperature');
      if (tempGroup) tempGroup.style.opacity = thinkingCheckbox.checked ? '0.4' : '1';
    });
  }

  // Fetch models from endpoint
  const fetchBtn = document.getElementById('sys-llm-fetch-models');
  if (fetchBtn) {
    fetchBtn.addEventListener('click', async () => {
      const endpoint = document.getElementById('sys-llm-endpoint').value.trim();
      const statusDiv = document.getElementById('sys-llm-models-status');
      if (!endpoint) { showToast('Enter an endpoint URL first', 'error'); return; }

      fetchBtn.disabled = true;
      if (statusDiv) statusDiv.innerHTML = '<span style="color:var(--text-muted)"><i class="fa-solid fa-spinner fa-spin"></i> Fetching models...</span>';

      try {
        // Try /v1/models endpoint (OpenAI-compatible standard)
        let modelsUrl = endpoint.replace(/\/+$/, '');
        if (modelsUrl.endsWith('/v1')) modelsUrl += '/models';
        else if (!modelsUrl.includes('/models')) modelsUrl += '/v1/models';

        const apiKey = document.getElementById('sys-llm-api-key').value.trim();
        const headers = { 'Content-Type': 'application/json' };
        if (apiKey) headers['Authorization'] = 'Bearer ' + apiKey;

        // Use the proxy to fetch models (avoids CORS)
        const base = localStorage.getItem('engram_api_base') || 'http://localhost:3030';
        const proxyUrl = base + '/proxy/llm';
        // Actually, we can't use the LLM proxy for /models. Let's try direct fetch first.
        const resp = await fetch(modelsUrl, { headers, signal: AbortSignal.timeout(5000) });
        const data = await resp.json();
        const models = (data.data || []).map(m => m.id).filter(Boolean).sort();

        if (models.length === 0) {
          if (statusDiv) statusDiv.innerHTML = '<span style="color:var(--text-muted)"><i class="fa-solid fa-info-circle"></i> No models found. Is the service running?</span>';
        } else {
          const datalist = document.getElementById('sys-llm-model-list');
          if (datalist) datalist.innerHTML = models.map(m => `<option value="${escapeHtml(m)}">`).join('');
          // Show visible dropdown for easy selection
          const modelSelect = document.getElementById('sys-llm-model-select');
          if (modelSelect) {
            const currentModel = document.getElementById('sys-llm-model').value;
            modelSelect.innerHTML = '<option value="">-- Select a model --</option>' +
              models.map(m => `<option value="${escapeHtml(m)}"${m === currentModel ? ' selected' : ''}>${escapeHtml(m)}</option>`).join('');
            modelSelect.style.display = '';
            modelSelect.onchange = () => {
              if (modelSelect.value) {
                document.getElementById('sys-llm-model').value = modelSelect.value;
              }
            };
          }
          if (statusDiv) statusDiv.innerHTML = `<span style="color:var(--success)"><i class="fa-solid fa-check"></i> Found ${models.length} model${models.length !== 1 ? 's' : ''}</span>`;
        }
      } catch (err) {
        if (statusDiv) statusDiv.innerHTML = `<span style="color:var(--warning)"><i class="fa-solid fa-triangle-exclamation"></i> Could not fetch models: ${escapeHtml(err.message || 'connection failed')}</span>`;
      } finally {
        fetchBtn.disabled = false;
      }
    });
  }

  // Test — saves first, then tests via proxy
  const testBtn = document.getElementById('sys-llm-test');
  if (testBtn) {
    testBtn.addEventListener('click', async () => {
      const endpoint = document.getElementById('sys-llm-endpoint').value.trim();
      const model = document.getElementById('sys-llm-model').value.trim();
      const resultDiv = document.getElementById('sys-llm-result');

      if (!endpoint) { showToast('Enter an endpoint URL', 'error'); return; }

      testBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Saving and testing LLM connection...');

      try {
        // Save config first so the proxy knows the endpoint
        const temperature = parseFloat(document.getElementById('sys-llm-temperature').value);
        const patch = { llm_endpoint: endpoint, llm_temperature: isNaN(temperature) ? 0.7 : temperature };
        if (model) patch.llm_model = model;
        const thinking = document.getElementById('sys-llm-thinking')?.checked;
        if (thinking !== undefined) patch.llm_thinking = thinking;
        await engram.setConfig(patch);

        // Save API key if provided
        const apiKey = document.getElementById('sys-llm-api-key').value;
        if (apiKey) {
          await engram.setSecret('llm.api_key', apiKey);
          document.getElementById('sys-llm-api-key').value = '';
        }

        // Now test via proxy
        await engram._post('/proxy/llm', { messages: [{ role: 'user', content: 'Respond with OK.' }], max_tokens: 5 });
        resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> LLM connection successful. Configuration saved.</div>';
        showToast('LLM connection verified', 'success');
        systemUpdateLlmBadge(model || document.getElementById('sys-llm-model').value);
        systemLoadLlmKeyIndicator(cfg);
      } catch (err) {
        const msg = err.message || '';
        if (msg.includes('502') || msg.includes('Connection refused')) {
          resultDiv.innerHTML = '<div style="color:var(--warning);font-size:0.85rem"><i class="fa-solid fa-triangle-exclamation"></i> Config saved, but LLM is not responding. Is the service running?</div>';
        } else if (msg.includes('401') || msg.includes('403')) {
          resultDiv.innerHTML = '<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-lock"></i> Authentication failed. Check your API key.</div>';
        } else {
          resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(msg)}</div>`;
        }
        showToast('LLM test failed', 'error');
      } finally {
        testBtn.disabled = false;
      }
    });
  }

  // Save
  const saveBtn = document.getElementById('sys-llm-save');
  if (saveBtn) {
    saveBtn.addEventListener('click', async () => {
      const endpoint = document.getElementById('sys-llm-endpoint').value.trim();
      const model = document.getElementById('sys-llm-model').value.trim();
      const apiKey = document.getElementById('sys-llm-api-key').value;
      const temperature = parseFloat(document.getElementById('sys-llm-temperature').value);
      const thinking = document.getElementById('sys-llm-thinking')?.checked;
      const resultDiv = document.getElementById('sys-llm-result');

      if (!endpoint) { showToast('Enter an endpoint URL', 'error'); return; }

      saveBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Saving LLM configuration...');

      try {
        const patch = {
          llm_endpoint: endpoint,
          llm_temperature: isNaN(temperature) ? 0.7 : temperature,
        };
        if (model) patch.llm_model = model;
        if (thinking !== undefined) patch.llm_thinking = thinking;

        await engram.setConfig(patch);

        // Store API key in encrypted secrets store
        if (apiKey) {
          await engram.setSecret('llm.api_key', apiKey);
        }

        resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> LLM configuration saved.</div>';
        showToast('LLM configuration saved', 'success');
        systemUpdateLlmBadge(model);
        document.getElementById('sys-llm-api-key').value = '';
        systemLoadLlmKeyIndicator(cfg);
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Save failed', 'error');
      } finally {
        saveBtn.disabled = false;
      }
    });
  }
}

function systemUpdateLlmUI() {
  const provider = document.getElementById('sys-llm-provider')?.value;
  const preset = SYSTEM_LLM_PROVIDERS.find(p => p.id === provider);

  // Update model suggestions
  const datalist = document.getElementById('sys-llm-model-list');
  if (datalist) datalist.innerHTML = (preset?.models || []).map(m => `<option value="${m}">`).join('');

  // Show/hide API key field
  const keyGroup = document.getElementById('sys-llm-key-group');
  if (keyGroup) keyGroup.style.display = (preset?.needsKey || provider === 'custom') ? '' : 'none';

  // Show/hide Fetch Models button (only for providers with /v1/models support)
  const fetchBtn = document.getElementById('sys-llm-fetch-models');
  if (fetchBtn) fetchBtn.style.display = (preset?.canFetchModels || provider === 'custom') ? '' : 'none';

  // Provider description
  const descDiv = document.getElementById('sys-llm-provider-desc');
  if (descDiv) descDiv.textContent = preset?.description || '';

  // Hide model dropdown when switching providers
  const modelSelect = document.getElementById('sys-llm-model-select');
  if (modelSelect) modelSelect.style.display = 'none';
}

/** Update the LLM section header badge after config changes. */
function systemUpdateLlmBadge(model) {
  const section = document.getElementById('section-llm');
  if (!section) return;
  const badge = section.querySelector('.feature-status');
  if (!badge) return;
  if (model) {
    badge.className = 'feature-status active';
    badge.style.color = 'var(--success)';
    badge.innerHTML = `<i class="fa-solid fa-circle" style="font-size:0.4rem;color:var(--success)"></i> ${escapeHtml(model)}`;
  }
}

async function systemLoadLlmKeyIndicator(cfg) {
  const indicator = document.getElementById('sys-llm-key-indicator');
  if (!indicator) return;
  try {
    const keyCheck = await engram.checkSecret('llm.api_key');
    if (keyCheck.exists) {
      indicator.innerHTML = '<span style="color:var(--success)"><i class="fa-solid fa-lock"></i> Encrypted</span>';
    } else if (cfg && cfg.has_llm_api_key) {
      indicator.innerHTML = '<span style="color:var(--confidence-mid)"><i class="fa-solid fa-check"></i> In config (unencrypted)</span>';
    }
  } catch (_) {
    if (cfg && cfg.has_llm_api_key) {
      indicator.innerHTML = '<span style="color:var(--success)"><i class="fa-solid fa-check"></i> Configured</span>';
    }
  }
}

// ── NER events ──

function bindNerEvents() {
  // Provider radio buttons
  document.querySelectorAll('input[name="sys-ner-provider"]').forEach(radio => {
    radio.addEventListener('change', () => {
      systemUpdateNerUI(radio.value);
    });
  });

  // Save
  const saveBtn = document.getElementById('sys-ner-save');
  if (saveBtn) {
    saveBtn.addEventListener('click', async () => {
      const selected = document.querySelector('input[name="sys-ner-provider"]:checked');
      if (!selected) { showToast('Select a NER provider', 'error'); return; }

      const provider = selected.value;
      const resultDiv = document.getElementById('sys-ner-result');
      saveBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Saving NER configuration...');

      try {
        const patch = { ner_provider: provider };
        if (provider === 'spacy' || provider === 'anno') {
          const endpoint = document.getElementById('sys-ner-endpoint').value.trim();
          const model = document.getElementById('sys-ner-model').value.trim();
          if (!endpoint) { showToast('Enter the NER service endpoint', 'error'); saveBtn.disabled = false; resultDiv.innerHTML = ''; return; }
          patch.ner_endpoint = endpoint;
          if (model) patch.ner_model = model;
        }

        await engram.setConfig(patch);
        resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> NER configuration saved.</div>';
        showToast('NER configuration saved', 'success');
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Save failed', 'error');
      } finally {
        saveBtn.disabled = false;
      }
    });
  }
}

function systemUpdateNerUI(provider) {
  const endpointGroup = document.getElementById('sys-ner-endpoint-group');
  const modelGroup = document.getElementById('sys-ner-model-group');
  const needsEndpoint = provider === 'spacy' || provider === 'anno';
  if (endpointGroup) endpointGroup.style.display = needsEndpoint ? '' : 'none';
  if (modelGroup) modelGroup.style.display = needsEndpoint ? '' : 'none';

  // Highlight selected card
  document.querySelectorAll('#sys-ner-provider-cards label').forEach(label => {
    const isSelected = label.dataset.provider === provider;
    label.style.borderColor = isSelected ? 'var(--accent-bright)' : 'var(--border)';
  });
}

// ── Quantization events ──

function bindQuantizationEvents() {
  const toggleBtn = document.getElementById('sys-quant-toggle');
  if (!toggleBtn) return;

  toggleBtn.addEventListener('click', async () => {
    const isCurrentlyOn = toggleBtn.textContent.trim().startsWith('Disable');
    const newState = !isCurrentlyOn;
    const resultDiv = document.getElementById('sys-quant-result');

    // Confirmation prompt
    if (newState) {
      if (!confirm('This will re-process all vectors for quantization. This may take a moment. Continue?')) return;
    } else {
      if (!confirm('Disable quantization? Vectors will be expanded back to full precision.')) return;
    }

    toggleBtn.disabled = true;
    resultDiv.innerHTML = loadingHTML(newState ? 'Enabling quantization...' : 'Disabling quantization...');

    try {
      await engram._post('/quantize', { enabled: newState });
      resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Quantization ${newState ? 'enabled' : 'disabled'}.</div>`;
      showToast('Quantization ' + (newState ? 'enabled' : 'disabled'), 'success');
      // Update button state
      toggleBtn.innerHTML = newState
        ? '<i class="fa-solid fa-toggle-off"></i> Disable Quantization'
        : '<i class="fa-solid fa-toggle-on"></i> Enable Quantization';
      toggleBtn.className = newState ? 'btn btn-secondary' : 'btn btn-primary';
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Failed: ' + err.message, 'error');
    } finally {
      toggleBtn.disabled = false;
    }
  });
}

// ── Mesh events ──

function bindMeshEvents_system(meshEnabled, meshPeers) {
  if (!meshEnabled) {
    // Enable mesh button
    const enableBtn = document.getElementById('sys-mesh-enable-btn');
    if (enableBtn) {
      enableBtn.addEventListener('click', async () => {
        enableBtn.disabled = true;
        enableBtn.innerHTML = '<span class="spinner"></span> Enabling...';
        const topology = document.querySelector('input[name="sys-mesh-topology"]:checked')?.value || 'star';
        try {
          await engram._post('/config', { mesh_enabled: true, mesh_topology: topology });
          showToast('Mesh networking enabled. Restart the server to activate.', 'success');
          enableBtn.innerHTML = '<i class="fa-solid fa-check"></i> Enabled';
        } catch (err) {
          showToast('Failed to enable mesh: ' + err.message, 'error');
          enableBtn.disabled = false;
          enableBtn.innerHTML = '<i class="fa-solid fa-power-off"></i> Enable Mesh';
        }
      });
    }
    return;
  }

  // Trust slider
  const trustSlider = document.getElementById('sys-mesh-peer-trust');
  const trustValue = document.getElementById('sys-mesh-trust-value');
  if (trustSlider && trustValue) {
    trustSlider.addEventListener('input', () => { trustValue.textContent = trustSlider.value; });
  }

  // Add peer
  const addBtn = document.getElementById('sys-mesh-add-peer-btn');
  if (addBtn) {
    addBtn.addEventListener('click', async () => {
      const endpoint = document.getElementById('sys-mesh-peer-endpoint').value.trim();
      const name = document.getElementById('sys-mesh-peer-name').value.trim();
      const trust = parseFloat(document.getElementById('sys-mesh-peer-trust').value);

      if (!endpoint) { showToast('Endpoint is required', 'error'); return; }

      addBtn.disabled = true;
      addBtn.innerHTML = '<span class="spinner"></span> Adding...';

      try {
        await engram._post('/mesh/peers', { endpoint, name, trust });
        showToast('Peer added successfully', 'success');
        await systemRefreshPeers();
        document.getElementById('sys-mesh-peer-endpoint').value = '';
        document.getElementById('sys-mesh-peer-name').value = '';
        document.getElementById('sys-mesh-peer-trust').value = '0.7';
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
  const refreshBtn = document.getElementById('sys-mesh-refresh-btn');
  if (refreshBtn) {
    refreshBtn.addEventListener('click', async () => {
      refreshBtn.disabled = true;
      refreshBtn.innerHTML = '<span class="spinner"></span>';
      await systemRefreshPeers();
      refreshBtn.disabled = false;
      refreshBtn.innerHTML = '<i class="fa-solid fa-arrows-rotate"></i> Refresh';
    });
  }

  // Remove peer buttons
  systemBindMeshRemoveButtons();

  // Discover
  const discoverBtn = document.getElementById('sys-mesh-discover-btn');
  if (discoverBtn) {
    discoverBtn.addEventListener('click', async () => {
      const topic = document.getElementById('sys-mesh-discover-topic').value.trim();
      if (!topic) { showToast('Enter a topic to discover peers', 'error'); return; }

      const resultsDiv = document.getElementById('sys-mesh-discover-results');
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
              <div style="display:flex;align-items:center;justify-content:space-between;padding:0.5rem;background:var(--bg-input);border:1px solid var(--border);border-radius:var(--radius-sm)">
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
  const auditBtn = document.getElementById('sys-mesh-audit-btn');
  if (auditBtn) {
    auditBtn.addEventListener('click', async () => {
      const panel = document.getElementById('sys-mesh-audit-panel');
      if (panel.style.display !== 'none') {
        panel.style.display = 'none';
        return;
      }

      panel.innerHTML = `<div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem">${loadingHTML('Loading audit log...')}</div>`;
      panel.style.display = 'block';

      try {
        const audit = await engram._fetch('/mesh/audit');
        const entries = Array.isArray(audit) ? audit : (audit && audit.entries ? audit.entries : []);
        if (entries.length === 0) {
          panel.innerHTML = '<div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem;text-align:center;color:var(--text-muted)"><i class="fa-solid fa-scroll" style="margin-right:0.3rem"></i> No audit entries yet.</div>';
        } else {
          let tableHtml = `
            <div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem">
              <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:0.75rem">
                <div style="font-weight:600;font-size:0.85rem"><i class="fa-solid fa-scroll" style="margin-right:0.4rem"></i> Audit Log</div>
                <button id="sys-mesh-audit-close" style="padding:0.25rem 0.5rem;background:none;border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-muted);cursor:pointer;font-size:0.8rem">
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

          document.getElementById('sys-mesh-audit-close').addEventListener('click', () => {
            panel.style.display = 'none';
          });
        }
      } catch (err) {
        panel.innerHTML = `<div style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:1rem;color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> Failed to load audit log: ${escapeHtml(err.message)}</div>`;
      }
    });
  }

  // Load sync status
  systemLoadSyncStatus();
}

async function systemRefreshPeers() {
  try {
    const result = await engram.meshPeers();
    const peers = Array.isArray(result) ? result : (result && result.peers ? result.peers : []);
    const tableDiv = document.getElementById('sys-mesh-peers-table');
    if (tableDiv) {
      tableDiv.innerHTML = systemRenderPeersTable(peers);
      systemBindMeshRemoveButtons();
    }
  } catch (err) {
    showToast('Failed to refresh peers: ' + err.message, 'error');
  }
}

function systemBindMeshRemoveButtons() {
  document.querySelectorAll('.sys-mesh-remove-peer').forEach(btn => {
    btn.addEventListener('click', async () => {
      const key = btn.getAttribute('data-key');
      if (!key) return;

      btn.disabled = true;
      btn.innerHTML = '<span class="spinner"></span>';

      try {
        await engram._fetch(`/mesh/peers/${encodeURIComponent(key)}`, { method: 'DELETE' });
        showToast('Peer removed', 'success');
        await systemRefreshPeers();
      } catch (err) {
        showToast('Failed to remove peer: ' + err.message, 'error');
        btn.disabled = false;
        btn.innerHTML = '<i class="fa-solid fa-trash-can"></i> Remove';
      }
    });
  });
}

async function systemLoadSyncStatus() {
  try {
    const audit = await engram._fetch('/mesh/audit');
    const entries = Array.isArray(audit) ? audit : (audit && audit.entries ? audit.entries : []);
    if (entries.length > 0) {
      const last = entries[entries.length - 1];
      const lastTime = last.timestamp || last.time;
      if (lastTime) {
        const el = document.getElementById('sys-mesh-last-sync');
        if (el) el.textContent = systemTimeAgo(lastTime);
      }

      let conflicts = 0;
      for (const e of entries) {
        if (e.event === 'conflict' || e.type === 'conflict') conflicts++;
      }
      const conflictEl = document.getElementById('sys-mesh-conflicts');
      if (conflictEl) conflictEl.textContent = String(conflicts);
    }
  } catch (_) {}
}

function systemTimeAgo(timestamp) {
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

// ── Secrets events ──

function bindSecretsEvents() {
  // Save secret
  const saveBtn = document.getElementById('sys-secret-save');
  if (saveBtn) {
    saveBtn.addEventListener('click', async () => {
      const key = document.getElementById('sys-secret-key').value.trim();
      const value = document.getElementById('sys-secret-value').value;
      const resultDiv = document.getElementById('sys-secret-result');

      if (!key || !value) { showToast('Enter both key and value', 'error'); return; }

      saveBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Saving secret...');

      try {
        await engram.setSecret(key, value);
        resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Secret saved securely.</div>';
        showToast('Secret saved', 'success');
        document.getElementById('sys-secret-key').value = '';
        document.getElementById('sys-secret-value').value = '';
        // Reload secrets list
        await systemReloadSecrets();
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Failed to save secret', 'error');
      } finally {
        saveBtn.disabled = false;
      }
    });
  }

  // Delete buttons
  systemBindSecretDeleteButtons();
}

function systemBindSecretDeleteButtons() {
  document.querySelectorAll('.sys-secret-delete').forEach(btn => {
    btn.addEventListener('click', async () => {
      const key = btn.getAttribute('data-key');
      if (!key) return;
      if (!confirm('Delete secret "' + key + '"? This cannot be undone.')) return;

      btn.disabled = true;
      try {
        await engram.deleteSecret(key);
        showToast('Secret deleted', 'success');
        await systemReloadSecrets();
      } catch (err) {
        showToast('Failed to delete: ' + err.message, 'error');
        btn.disabled = false;
      }
    });
  });
}

async function systemReloadSecrets() {
  try {
    const result = await engram.listSecrets();
    const secrets = Array.isArray(result) ? result : (result && result.keys ? result.keys : []);

    // Rebuild just the list portion
    const sectionBody = document.querySelector('#section-secrets .section-body');
    if (sectionBody) {
      sectionBody.innerHTML = buildSecretsSection(secrets);
      systemBindSecretDeleteButtons();
      // Re-bind save button
      const saveBtn = document.getElementById('sys-secret-save');
      if (saveBtn) {
        saveBtn.addEventListener('click', async () => {
          const key = document.getElementById('sys-secret-key').value.trim();
          const value = document.getElementById('sys-secret-value').value;
          const resultDiv = document.getElementById('sys-secret-result');

          if (!key || !value) { showToast('Enter both key and value', 'error'); return; }

          saveBtn.disabled = true;
          resultDiv.innerHTML = loadingHTML('Saving secret...');

          try {
            await engram.setSecret(key, value);
            resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Secret saved securely.</div>';
            showToast('Secret saved', 'success');
            document.getElementById('sys-secret-key').value = '';
            document.getElementById('sys-secret-value').value = '';
            await systemReloadSecrets();
          } catch (err) {
            resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
            showToast('Failed to save secret', 'error');
          } finally {
            saveBtn.disabled = false;
          }
        });
      }

      // Update section header status
      const statusSpan = document.querySelector('#section-secrets .feature-status');
      if (statusSpan) {
        statusSpan.outerHTML = secrets.length > 0
          ? systemStatusDot('active', secrets.length + ' key' + (secrets.length !== 1 ? 's' : ''))
          : systemStatusDot('setup', 'No secrets');
      }
    }
  } catch (_) {}
}

// ── Import / Export events ──

function bindImportExportEvents() {
  // Export
  const exportBtn = document.getElementById('sys-export-jsonld');
  if (exportBtn) {
    exportBtn.addEventListener('click', async () => {
      const resultDiv = document.getElementById('sys-export-result');
      exportBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Exporting...');

      try {
        const data = await engram.exportJsonLd();
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/ld+json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = 'engram-export-' + new Date().toISOString().slice(0, 10) + '.jsonld';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
        resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Export downloaded.</div>';
        showToast('Export complete', 'success');
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Export failed', 'error');
      } finally {
        exportBtn.disabled = false;
      }
    });
  }

  // Import
  const importBtn = document.getElementById('sys-import-jsonld-btn');
  if (importBtn) {
    importBtn.addEventListener('click', async () => {
      const text = document.getElementById('sys-import-jsonld').value.trim();
      if (!text) { showToast('Please paste JSON-LD data', 'error'); return; }

      let data;
      try {
        data = JSON.parse(text);
      } catch (err) {
        showToast('Invalid JSON: ' + err.message, 'error');
        return;
      }

      const resultDiv = document.getElementById('sys-import-result');
      importBtn.disabled = true;
      resultDiv.innerHTML = loadingHTML('Importing...');

      try {
        const result = await engram.importJsonLd(data);
        let msg = 'Import complete.';
        if (result.nodes_created != null) msg += ' ' + result.nodes_created + ' facts created.';
        if (result.edges_created != null) msg += ' ' + result.edges_created + ' connections created.';
        resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> ' + escapeHtml(msg) + '</div>';
        showToast('Import complete', 'success');
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Import failed', 'error');
      } finally {
        importBtn.disabled = false;
      }
    });
  }
}
