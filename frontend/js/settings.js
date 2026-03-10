/* ============================================
   engram - Settings View
   Configuration and maintenance
   ============================================ */

// Provider presets for quick configuration
const EMBED_PROVIDERS = [
  { id: 'ollama',  label: 'Ollama',       endpoint: 'http://localhost:11434', models: ['nomic-embed-text', 'mxbai-embed-large', 'all-minilm'] },
  { id: 'openai',  label: 'OpenAI',       endpoint: 'https://api.openai.com/v1', models: ['text-embedding-3-small', 'text-embedding-3-large', 'text-embedding-ada-002'] },
  { id: 'vllm',    label: 'vLLM',         endpoint: 'http://localhost:8000/v1', models: [] },
  { id: 'custom',  label: 'Custom',       endpoint: '', models: [] },
];

const LLM_PROVIDERS = [
  { id: 'ollama',    label: 'Ollama',     endpoint: 'http://localhost:11434/v1', models: ['llama3.2', 'mistral', 'gemma2', 'phi3'] },
  { id: 'openai',    label: 'OpenAI',     endpoint: 'https://api.openai.com/v1', models: ['gpt-4o', 'gpt-4o-mini', 'gpt-4-turbo'], needsKey: true },
  { id: 'anthropic', label: 'Anthropic',  endpoint: 'https://api.anthropic.com/v1', models: ['claude-sonnet-4-20250514', 'claude-haiku-4-20250414'], needsKey: true },
  { id: 'vllm',     label: 'vLLM',        endpoint: 'http://localhost:8000/v1', models: [] },
  { id: 'custom',   label: 'Custom',      endpoint: '', models: [] },
];

const NER_PROVIDERS = [
  { id: 'builtin', label: 'Built-in (Rule-based)', quality: 'Basic', description: 'Pattern matching, always available, no extra setup.' },
  { id: 'spacy',   label: 'spaCy',                 quality: 'Good',  description: 'Statistical NER, good accuracy. Requires spaCy service.' },
  { id: 'anno',    label: 'spaCy + Anno',           quality: 'Excellent', description: 'Best NER quality with learning. Requires Anno service.' },
];

router.register('/settings', async () => {
  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-sliders"></i> Settings</h1>
        <p class="text-secondary" style="margin-top:0.25rem">Configuration and maintenance</p>
      </div>
    </div>
    <div class="grid-2">

      <!-- 1. Embedding Model -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-cube"></i> Embedding Model</h3>
        </div>
        <div id="embed-lock-warning" style="display:none"></div>
        <div id="embed-status" style="margin-bottom:0.75rem"></div>
        <div class="form-group">
          <label>Provider</label>
          <select id="cfg-embed-provider" style="width:100%">
            ${EMBED_PROVIDERS.map(p => `<option value="${p.id}">${escapeHtml(p.label)}</option>`).join('')}
          </select>
        </div>
        <div class="form-group">
          <label>Endpoint URL</label>
          <input type="text" id="cfg-embed-endpoint" placeholder="http://localhost:11434">
        </div>
        <div class="form-group">
          <label>Model Name</label>
          <div style="position:relative">
            <input type="text" id="cfg-embed-model" placeholder="nomic-embed-text" list="embed-model-suggestions">
            <datalist id="embed-model-suggestions"></datalist>
          </div>
        </div>
        <div id="embed-dimensions" style="font-size:0.85rem;margin-bottom:0.75rem"></div>
        <div style="display:flex;gap:0.5rem">
          <button class="btn btn-secondary" id="btn-embed-test">
            <i class="fa-solid fa-plug"></i> Test Connection
          </button>
          <button class="btn btn-primary" id="btn-embed-save">
            <i class="fa-solid fa-save"></i> Save
          </button>
        </div>
        <div id="embed-result" class="mt-1"></div>
      </div>

      <!-- 2. Language Model (LLM) -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-robot"></i> Language Model (LLM)</h3>
        </div>
        <div id="llm-status" style="margin-bottom:0.75rem"></div>
        <div class="form-group">
          <label>Provider</label>
          <select id="cfg-llm-provider" style="width:100%">
            ${LLM_PROVIDERS.map(p => `<option value="${p.id}">${escapeHtml(p.label)}</option>`).join('')}
          </select>
        </div>
        <div class="form-group">
          <label>Endpoint URL</label>
          <input type="text" id="cfg-llm-endpoint" placeholder="http://localhost:11434/v1">
        </div>
        <div class="form-group">
          <label>Model Name</label>
          <div style="position:relative">
            <input type="text" id="cfg-llm-model" placeholder="llama3.2" list="llm-model-suggestions">
            <datalist id="llm-model-suggestions"></datalist>
          </div>
        </div>
        <div class="form-group" id="llm-key-group">
          <label>API Key</label>
          <div style="position:relative">
            <input type="password" id="cfg-llm-api-key" placeholder="Stored encrypted -- leave blank to keep">
            <span id="llm-key-indicator" style="position:absolute;right:0.75rem;top:50%;transform:translateY(-50%);font-size:0.8rem"></span>
          </div>
        </div>
        <div class="form-group">
          <label style="display:flex;justify-content:space-between;align-items:center">
            <span>Temperature</span>
            <span id="llm-temp-value" style="font-family:var(--font-mono);font-size:0.85rem;color:var(--text-secondary)">0.7</span>
          </label>
          <input type="range" id="cfg-llm-temperature" min="0" max="2" step="0.1" value="0.7"
            style="width:100%;cursor:pointer">
        </div>
        <div style="display:flex;gap:0.5rem">
          <button class="btn btn-secondary" id="btn-llm-test">
            <i class="fa-solid fa-plug"></i> Test Connection
          </button>
          <button class="btn btn-primary" id="btn-llm-save">
            <i class="fa-solid fa-save"></i> Save
          </button>
        </div>
        <div id="llm-result" class="mt-1"></div>
      </div>

      <!-- 3. Entity Recognition (NER) -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-tag"></i> Entity Recognition (NER)</h3>
        </div>
        <div id="ner-status" style="margin-bottom:0.75rem"></div>
        <div class="form-group">
          <label>NER Provider</label>
          <div id="ner-provider-cards" style="display:flex;flex-direction:column;gap:0.5rem">
            ${NER_PROVIDERS.map(p => `
              <label style="display:flex;align-items:flex-start;gap:0.6rem;padding:0.6rem 0.75rem;background:var(--bg-secondary);border:2px solid var(--border);border-radius:var(--radius-sm);cursor:pointer;transition:border-color 0.15s" data-provider="${p.id}">
                <input type="radio" name="ner-provider" value="${p.id}" style="margin-top:0.2rem;flex-shrink:0">
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
        <div id="ner-endpoint-group" class="form-group" style="display:none">
          <label>NER Service Endpoint</label>
          <input type="text" id="cfg-ner-endpoint" placeholder="http://localhost:5000">
        </div>
        <div id="ner-model-group" class="form-group" style="display:none">
          <label>Model Name</label>
          <input type="text" id="cfg-ner-model" placeholder="en_core_web_sm">
        </div>
        <div style="display:flex;gap:0.5rem">
          <button class="btn btn-primary" id="btn-ner-save">
            <i class="fa-solid fa-save"></i> Save NER Config
          </button>
        </div>
        <div id="ner-result" class="mt-1"></div>
      </div>

      <!-- 4. Learning -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-graduation-cap"></i> Learning</h3>
        </div>
        <p class="text-secondary mb-2" style="font-size:0.9rem">
          Reinforce or correct facts to improve knowledge quality.
        </p>
        <div class="form-group">
          <label>Reinforce a Fact</label>
          <div class="form-row mb-1">
            <input type="text" id="settings-reinforce-label" placeholder="Fact name to reinforce..." style="flex:1">
            <button class="btn btn-primary" id="btn-settings-reinforce">
              <i class="fa-solid fa-arrow-up"></i> Reinforce
            </button>
          </div>
        </div>
        <div class="form-group">
          <label>Correct a Fact</label>
          <div class="form-row mb-1">
            <input type="text" id="settings-correct-old" placeholder="Wrong fact..." style="flex:1">
            <input type="text" id="settings-correct-new" placeholder="Correct fact..." style="flex:1">
            <button class="btn btn-primary" id="btn-settings-correct">
              <i class="fa-solid fa-pen"></i> Correct
            </button>
          </div>
        </div>
        <div id="learning-result" class="mt-1"></div>
      </div>

      <!-- 5. Memory Decay -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-clock-rotate-left"></i> Memory Decay</h3>
        </div>
        <p class="text-secondary mb-2" style="font-size:0.9rem">
          Reduce strength of unused knowledge over time.
        </p>
        <button class="btn btn-danger" id="btn-settings-decay">
          <i class="fa-solid fa-hourglass-half"></i> Trigger Decay Cycle
        </button>
        <div id="settings-decay-result" class="mt-1"></div>
      </div>

      <!-- 6. Inference Rules -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-wand-magic-sparkles"></i> Inference Rules</h3>
        </div>
        <div class="form-group">
          <label>Rule Definitions</label>
          <textarea id="settings-rules" rows="6" placeholder="Enter inference rules, one per line.

Example:
IF ?x is_a ?y AND ?y is_a ?z THEN ?x is_a ?z
IF ?x uses ?y AND ?y is_a database THEN ?x has_database ?y"></textarea>
        </div>
        <button class="btn btn-primary" id="btn-settings-derive">
          <i class="fa-solid fa-play"></i> Run Rules
        </button>
        <div id="settings-derive-result" class="mt-1"></div>
      </div>

      <!-- 7. Action Rules -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-bolt"></i> Action Rules</h3>
        </div>
        <p class="text-secondary mb-2" style="font-size:0.85rem">
          Action rules trigger automated responses when knowledge changes.
        </p>
        <div id="action-rules-list">${loadingHTML('Loading action rules...')}</div>
        <div class="form-group mt-2">
          <label>Load New Rules (JSON)</label>
          <textarea id="settings-action-rules" rows="4" placeholder='[{"id":"example","condition":"...","action":"..."}]'></textarea>
        </div>
        <button class="btn btn-primary" id="btn-load-action-rules">
          <i class="fa-solid fa-upload"></i> Load Rules
        </button>
        <div id="action-rules-result" class="mt-1"></div>
      </div>

      <!-- 8. Quantization -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-compress"></i> Quantization</h3>
        </div>
        <p class="text-secondary mb-2" style="font-size:0.9rem">
          Reduce memory usage for vector search. Trades approximately 1% accuracy for 4x less memory.
        </p>
        <div style="display:flex;gap:0.75rem">
          <button class="btn btn-primary" id="btn-quantize-on">
            <i class="fa-solid fa-toggle-on"></i> Enable Quantization
          </button>
          <button class="btn btn-secondary" id="btn-quantize-off">
            <i class="fa-solid fa-toggle-off"></i> Disable
          </button>
        </div>
        <div id="quantize-result" class="mt-1"></div>
      </div>

      <!-- 9. Explain Entity -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-magnifying-glass-chart"></i> Explain Entity</h3>
        </div>
        <p class="text-secondary mb-2" style="font-size:0.85rem">
          View the full provenance and history of any fact.
        </p>
        <div class="form-row mb-1">
          <div class="form-group" style="margin-bottom:0">
            <input type="text" id="settings-explain-label" placeholder="Enter a fact name...">
          </div>
          <button class="btn btn-primary" id="btn-settings-explain">
            <i class="fa-solid fa-search"></i> Explain
          </button>
        </div>
        <div id="settings-explain-result"></div>
      </div>

      <!-- 10. Export / Import -->
      <div class="card">
        <div class="card-header">
          <h3><i class="fa-solid fa-file-export"></i> Export / Import</h3>
        </div>
        <p class="text-secondary mb-2" style="font-size:0.85rem">
          Export your knowledge base as JSON-LD for backup or sharing.
        </p>
        <button class="btn btn-primary mb-2" id="btn-export-jsonld">
          <i class="fa-solid fa-download"></i> Export as JSON-LD
        </button>
        <div id="export-result" class="mt-1"></div>

        <div class="form-group mt-2">
          <label>Import JSON-LD</label>
          <textarea id="settings-import-jsonld" rows="4" placeholder="Paste JSON-LD data here..."></textarea>
        </div>
        <button class="btn btn-primary" id="btn-import-jsonld">
          <i class="fa-solid fa-upload"></i> Import
        </button>
        <div id="import-result" class="mt-1"></div>
      </div>
    </div>
  `);

  setupSettingsEvents();
  setupConfigEvents();
  setupNerEvents();
  loadActionRules();
  loadCurrentConfig();
});

// --- Load current config from GET /config and populate fields ---
async function loadCurrentConfig() {
  try {
    const [cfg, stats] = await Promise.all([
      engram.getConfig(),
      engram.stats().catch(() => null),
    ]);

    const hasData = stats && (stats.nodes || stats.node_count || 0) > 0;

    // Embedding fields
    const embedEndpoint = document.getElementById('cfg-embed-endpoint');
    const embedModel = document.getElementById('cfg-embed-model');
    if (embedEndpoint && cfg.embed_endpoint) embedEndpoint.value = cfg.embed_endpoint;
    if (embedModel && cfg.embed_model) embedModel.value = cfg.embed_model;

    // Detect provider from endpoint
    const embedProvider = document.getElementById('cfg-embed-provider');
    if (embedProvider && cfg.embed_endpoint) {
      const matched = EMBED_PROVIDERS.find(p => p.id !== 'custom' && cfg.embed_endpoint.includes(p.endpoint.replace(/\/v1$/, '')));
      if (matched) {
        embedProvider.value = matched.id;
      } else {
        embedProvider.value = 'custom';
      }
      updateEmbedModelSuggestions();
    }

    // Embedder lock warning when data exists and embedder is configured
    const lockWarning = document.getElementById('embed-lock-warning');
    if (lockWarning) {
      if (hasData && cfg.embed_endpoint && cfg.embed_model) {
        lockWarning.style.display = 'block';
        lockWarning.innerHTML = `
          <div style="display:flex;align-items:center;gap:0.5rem;padding:0.6rem 0.75rem;background:rgba(227,160,8,0.1);border:1px solid rgba(227,160,8,0.3);border-radius:var(--radius-sm);font-size:0.85rem;color:var(--confidence-mid);margin-bottom:0.75rem">
            <i class="fa-solid fa-lock" style="flex-shrink:0"></i>
            <span><strong>Embedder locked.</strong> Your knowledge base has data. Changing the embedding model will invalidate all vectors and require a full reindex.</span>
          </div>`;
      } else {
        lockWarning.style.display = 'none';
      }
    }

    // Embedding status
    const embedStatus = document.getElementById('embed-status');
    if (embedStatus) {
      if (cfg.embed_endpoint && cfg.embed_model) {
        embedStatus.innerHTML = `
          <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem">
            <i class="fa-solid fa-circle-check" style="color:var(--success)"></i>
            <span>Connected</span>
            <span class="text-muted" style="font-size:0.8rem">${escapeHtml(cfg.embed_model)}</span>
          </div>`;
      } else {
        embedStatus.innerHTML = `
          <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem">
            <i class="fa-solid fa-circle-xmark" style="color:var(--text-muted)"></i>
            <span class="text-muted">Not configured</span>
          </div>`;
      }
    }

    // Dimension info from /compute
    try {
      const compute = await engram.compute();
      const dimDiv = document.getElementById('embed-dimensions');
      if (dimDiv && compute && compute.embedder_dim) {
        dimDiv.innerHTML = `
          <div style="display:flex;align-items:center;gap:0.5rem;color:var(--text-secondary)">
            <i class="fa-solid fa-ruler-combined"></i>
            <span>Dimensions: <strong>${compute.embedder_dim}</strong></span>
          </div>`;
      }
    } catch (_) {}

    // LLM fields
    const llmEndpoint = document.getElementById('cfg-llm-endpoint');
    const llmModel = document.getElementById('cfg-llm-model');
    const llmTemp = document.getElementById('cfg-llm-temperature');
    const llmTempValue = document.getElementById('llm-temp-value');
    if (llmEndpoint && cfg.llm_endpoint) llmEndpoint.value = cfg.llm_endpoint;
    if (llmModel && cfg.llm_model) llmModel.value = cfg.llm_model;
    if (llmTemp && cfg.llm_temperature != null) {
      llmTemp.value = cfg.llm_temperature;
      if (llmTempValue) llmTempValue.textContent = cfg.llm_temperature.toFixed(1);
    }

    // Detect LLM provider
    const llmProvider = document.getElementById('cfg-llm-provider');
    if (llmProvider && cfg.llm_endpoint) {
      const matched = LLM_PROVIDERS.find(p => p.id !== 'custom' && cfg.llm_endpoint.includes(p.endpoint.replace(/\/v1$/, '')));
      if (matched) {
        llmProvider.value = matched.id;
      } else {
        llmProvider.value = 'custom';
      }
      updateLlmModelSuggestions();
      updateLlmKeyVisibility();
    }

    // LLM API key indicator (check secrets store first, fallback to config)
    const keyIndicator = document.getElementById('llm-key-indicator');
    if (keyIndicator) {
      try {
        const keyCheck = await engram.checkSecret('llm.api_key');
        if (keyCheck.exists) {
          keyIndicator.innerHTML = '<span style="color:var(--success)"><i class="fa-solid fa-lock"></i> Encrypted</span>';
        } else if (cfg.has_llm_api_key) {
          keyIndicator.innerHTML = '<span style="color:var(--confidence-mid)"><i class="fa-solid fa-check"></i> In config (unencrypted)</span>';
        }
      } catch (_) {
        if (cfg.has_llm_api_key) {
          keyIndicator.innerHTML = '<span style="color:var(--success)"><i class="fa-solid fa-check"></i> Configured</span>';
        }
      }
    }

    // LLM status
    const llmStatus = document.getElementById('llm-status');
    if (llmStatus) {
      if (cfg.llm_endpoint && cfg.llm_model) {
        llmStatus.innerHTML = `
          <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem">
            <i class="fa-solid fa-circle-check" style="color:var(--success)"></i>
            <span>Configured</span>
            <span class="text-muted" style="font-size:0.8rem">${escapeHtml(cfg.llm_model)}</span>
          </div>`;
      } else {
        llmStatus.innerHTML = `
          <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem">
            <i class="fa-solid fa-circle-xmark" style="color:var(--text-muted)"></i>
            <span class="text-muted">Not configured</span>
          </div>`;
      }
    }

    // NER fields
    const nerProvider = cfg.ner_provider || 'builtin';
    const nerRadio = document.querySelector(`input[name="ner-provider"][value="${nerProvider}"]`);
    if (nerRadio) {
      nerRadio.checked = true;
      updateNerProviderUI(nerProvider);
    }
    if (cfg.ner_endpoint) document.getElementById('cfg-ner-endpoint').value = cfg.ner_endpoint;
    if (cfg.ner_model) document.getElementById('cfg-ner-model').value = cfg.ner_model;

    // NER status
    const nerStatus = document.getElementById('ner-status');
    if (nerStatus) {
      const provLabel = NER_PROVIDERS.find(p => p.id === nerProvider)?.label || nerProvider;
      nerStatus.innerHTML = `
        <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.9rem">
          <i class="fa-solid fa-circle-check" style="color:var(--success)"></i>
          <span>Active: ${escapeHtml(provLabel)}</span>
        </div>`;
    }

  } catch (err) {
    const embedStatus = document.getElementById('embed-status');
    if (embedStatus) {
      embedStatus.innerHTML = `
        <div style="display:flex;align-items:center;gap:0.5rem;font-size:0.85rem;color:var(--error)">
          <i class="fa-solid fa-circle-exclamation"></i>
          <span>Could not load configuration: ${escapeHtml(err.message)}</span>
        </div>`;
    }
  }
}

// --- Provider selector helpers ---
function updateEmbedModelSuggestions() {
  const provider = document.getElementById('cfg-embed-provider').value;
  const preset = EMBED_PROVIDERS.find(p => p.id === provider);
  const datalist = document.getElementById('embed-model-suggestions');
  datalist.innerHTML = (preset?.models || []).map(m => `<option value="${m}">`).join('');
}

function updateLlmModelSuggestions() {
  const provider = document.getElementById('cfg-llm-provider').value;
  const preset = LLM_PROVIDERS.find(p => p.id === provider);
  const datalist = document.getElementById('llm-model-suggestions');
  datalist.innerHTML = (preset?.models || []).map(m => `<option value="${m}">`).join('');
}

function updateLlmKeyVisibility() {
  const provider = document.getElementById('cfg-llm-provider').value;
  const preset = LLM_PROVIDERS.find(p => p.id === provider);
  const keyGroup = document.getElementById('llm-key-group');
  // Show API key field for providers that need it, or custom
  keyGroup.style.display = (preset?.needsKey || provider === 'custom') ? '' : 'none';
}

function updateNerProviderUI(provider) {
  const endpointGroup = document.getElementById('ner-endpoint-group');
  const modelGroup = document.getElementById('ner-model-group');
  const needsEndpoint = provider === 'spacy' || provider === 'anno';
  endpointGroup.style.display = needsEndpoint ? '' : 'none';
  modelGroup.style.display = needsEndpoint ? '' : 'none';

  // Highlight selected card
  document.querySelectorAll('#ner-provider-cards label').forEach(label => {
    const isSelected = label.dataset.provider === provider;
    label.style.borderColor = isSelected ? 'var(--accent-bright)' : 'var(--border)';
  });
}

// --- Config card events (Embedding + LLM + NER) ---
function setupConfigEvents() {
  // --- Embed provider change ---
  document.getElementById('cfg-embed-provider').addEventListener('change', (e) => {
    const preset = EMBED_PROVIDERS.find(p => p.id === e.target.value);
    if (preset && preset.endpoint) {
      document.getElementById('cfg-embed-endpoint').value = preset.endpoint;
    }
    updateEmbedModelSuggestions();
  });

  // --- LLM provider change ---
  document.getElementById('cfg-llm-provider').addEventListener('change', (e) => {
    const preset = LLM_PROVIDERS.find(p => p.id === e.target.value);
    if (preset && preset.endpoint) {
      document.getElementById('cfg-llm-endpoint').value = preset.endpoint;
    }
    updateLlmModelSuggestions();
    updateLlmKeyVisibility();
  });

  // Temperature slider live update
  const tempSlider = document.getElementById('cfg-llm-temperature');
  const tempLabel = document.getElementById('llm-temp-value');
  if (tempSlider && tempLabel) {
    tempSlider.addEventListener('input', () => {
      tempLabel.textContent = parseFloat(tempSlider.value).toFixed(1);
    });
  }

  // --- Embedding: Test Connection ---
  document.getElementById('btn-embed-test').addEventListener('click', async () => {
    const endpoint = document.getElementById('cfg-embed-endpoint').value.trim();
    const model = document.getElementById('cfg-embed-model').value.trim();
    const resultDiv = document.getElementById('embed-result');
    const btn = document.getElementById('btn-embed-test');

    if (!endpoint || !model) {
      showToast('Enter both endpoint URL and model name', 'error');
      return;
    }

    btn.disabled = true;
    resultDiv.innerHTML = loadingHTML('Testing connection...');

    try {
      await engram.setConfig({ embed_endpoint: endpoint, embed_model: model });
      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem">
          <i class="fa-solid fa-check"></i> Connection successful. Embedder configured.
        </div>`;
      showToast('Embedding connection verified', 'success');
      loadCurrentConfig();
    } catch (err) {
      resultDiv.innerHTML = `
        <div style="color:var(--error);font-size:0.85rem">
          <i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}
        </div>`;
      showToast('Embedding test failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- Embedding: Save ---
  document.getElementById('btn-embed-save').addEventListener('click', async () => {
    const endpoint = document.getElementById('cfg-embed-endpoint').value.trim();
    const model = document.getElementById('cfg-embed-model').value.trim();
    const resultDiv = document.getElementById('embed-result');
    const btn = document.getElementById('btn-embed-save');

    if (!endpoint || !model) {
      showToast('Enter both endpoint URL and model name', 'error');
      return;
    }

    btn.disabled = true;
    resultDiv.innerHTML = loadingHTML('Saving embedding configuration...');

    try {
      await engram.setConfig({ embed_endpoint: endpoint, embed_model: model });

      let dimText = '';
      try {
        const compute = await engram.compute();
        if (compute && compute.embedder_dim) {
          dimText = ' Detected dimensions: ' + compute.embedder_dim;
        }
      } catch (_) {}

      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem">
          <i class="fa-solid fa-check"></i> Embedding configuration saved.${dimText}
        </div>`;
      showToast('Embedding configuration saved', 'success');
      loadCurrentConfig();
    } catch (err) {
      resultDiv.innerHTML = `
        <div style="color:var(--error);font-size:0.85rem">
          <i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}
        </div>`;
      showToast('Save failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- LLM: Test Connection ---
  document.getElementById('btn-llm-test').addEventListener('click', async () => {
    const endpoint = document.getElementById('cfg-llm-endpoint').value.trim();
    const resultDiv = document.getElementById('llm-result');
    const btn = document.getElementById('btn-llm-test');

    if (!endpoint) {
      showToast('Enter an endpoint URL', 'error');
      return;
    }

    btn.disabled = true;
    resultDiv.innerHTML = loadingHTML('Testing LLM connection...');

    try {
      await engram._post('/proxy/llm', {
        messages: [{ role: 'user', content: 'test' }],
        max_tokens: 1
      });
      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem">
          <i class="fa-solid fa-check"></i> LLM connection successful.
        </div>`;
      showToast('LLM connection verified', 'success');
    } catch (err) {
      const is502 = err.message && err.message.includes('502');
      if (is502) {
        resultDiv.innerHTML = `
          <div style="color:var(--warning);font-size:0.85rem">
            <i class="fa-solid fa-triangle-exclamation"></i> Endpoint configured but LLM is not responding (502).
          </div>`;
      } else {
        resultDiv.innerHTML = `
          <div style="color:var(--error);font-size:0.85rem">
            <i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}
          </div>`;
      }
      showToast('LLM test failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- LLM: Save ---
  document.getElementById('btn-llm-save').addEventListener('click', async () => {
    const endpoint = document.getElementById('cfg-llm-endpoint').value.trim();
    const model = document.getElementById('cfg-llm-model').value.trim();
    const apiKey = document.getElementById('cfg-llm-api-key').value;
    const temperature = parseFloat(document.getElementById('cfg-llm-temperature').value);
    const resultDiv = document.getElementById('llm-result');
    const btn = document.getElementById('btn-llm-save');

    if (!endpoint) {
      showToast('Enter an endpoint URL', 'error');
      return;
    }

    btn.disabled = true;
    resultDiv.innerHTML = loadingHTML('Saving LLM configuration...');

    try {
      const patch = {
        llm_endpoint: endpoint,
        llm_temperature: isNaN(temperature) ? 0.7 : temperature,
      };
      if (model) patch.llm_model = model;

      await engram.setConfig(patch);

      // Store API key in encrypted secrets store (not in config)
      if (apiKey) {
        await engram.setSecret('llm.api_key', apiKey);
      }
      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem">
          <i class="fa-solid fa-check"></i> LLM configuration saved.
        </div>`;
      showToast('LLM configuration saved', 'success');

      document.getElementById('cfg-llm-api-key').value = '';
      loadCurrentConfig();
    } catch (err) {
      resultDiv.innerHTML = `
        <div style="color:var(--error);font-size:0.85rem">
          <i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}
        </div>`;
      showToast('Save failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });
}

// --- NER events ---
function setupNerEvents() {
  // Provider radio buttons
  document.querySelectorAll('input[name="ner-provider"]').forEach(radio => {
    radio.addEventListener('change', () => {
      updateNerProviderUI(radio.value);
    });
  });

  // Save NER
  document.getElementById('btn-ner-save').addEventListener('click', async () => {
    const selected = document.querySelector('input[name="ner-provider"]:checked');
    if (!selected) { showToast('Select a NER provider', 'error'); return; }

    const provider = selected.value;
    const resultDiv = document.getElementById('ner-result');
    const btn = document.getElementById('btn-ner-save');
    btn.disabled = true;
    resultDiv.innerHTML = loadingHTML('Saving NER configuration...');

    try {
      const patch = { ner_provider: provider };
      if (provider === 'spacy' || provider === 'anno') {
        const endpoint = document.getElementById('cfg-ner-endpoint').value.trim();
        const model = document.getElementById('cfg-ner-model').value.trim();
        if (!endpoint) { showToast('Enter the NER service endpoint', 'error'); btn.disabled = false; resultDiv.innerHTML = ''; return; }
        patch.ner_endpoint = endpoint;
        if (model) patch.ner_model = model;
      }

      await engram.setConfig(patch);
      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem">
          <i class="fa-solid fa-check"></i> NER configuration saved.
        </div>`;
      showToast('NER configuration saved', 'success');
      loadCurrentConfig();
    } catch (err) {
      resultDiv.innerHTML = `
        <div style="color:var(--error);font-size:0.85rem">
          <i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}
        </div>`;
      showToast('Save failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });
}

function setupSettingsEvents() {
  // --- Learning: Reinforce ---
  document.getElementById('btn-settings-reinforce').addEventListener('click', async () => {
    const label = document.getElementById('settings-reinforce-label').value.trim();
    if (!label) { showToast('Enter a fact name to reinforce', 'error'); return; }
    const resultDiv = document.getElementById('learning-result');
    try {
      const result = await engram.reinforce({ label });
      const newConf = result.confidence != null ? ' New strength: ' + Math.round(result.confidence * 100) + '%' : '';
      resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Reinforced "${escapeHtml(label)}".${newConf}</div>`;
      showToast('Fact reinforced', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
    }
  });

  // --- Learning: Correct ---
  document.getElementById('btn-settings-correct').addEventListener('click', async () => {
    const oldLabel = document.getElementById('settings-correct-old').value.trim();
    const newLabel = document.getElementById('settings-correct-new').value.trim();
    if (!oldLabel || !newLabel) { showToast('Enter both the wrong and correct fact', 'error'); return; }
    const resultDiv = document.getElementById('learning-result');
    try {
      await engram.correct({ old_label: oldLabel, new_label: newLabel });
      resultDiv.innerHTML = `<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Corrected "${escapeHtml(oldLabel)}" to "${escapeHtml(newLabel)}".</div>`;
      showToast('Fact corrected', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
    }
  });

  // --- Memory Decay ---
  document.getElementById('btn-settings-decay').addEventListener('click', async () => {
    if (!confirm('Run a decay cycle? This will reduce the strength of unreinforced facts.')) return;

    const btn = document.getElementById('btn-settings-decay');
    const resultDiv = document.getElementById('settings-decay-result');
    btn.disabled = true;
    resultDiv.innerHTML = loadingHTML('Running decay cycle...');

    try {
      const result = await engram.decay();
      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem">
          <i class="fa-solid fa-check"></i> Decay cycle complete.
          ${result.decayed != null ? result.decayed + ' facts decayed.' : ''}
          ${result.pruned != null ? result.pruned + ' facts pruned.' : ''}
        </div>`;
      showToast('Decay cycle complete', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Decay failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- Inference Rules ---
  document.getElementById('btn-settings-derive').addEventListener('click', async () => {
    const rulesText = document.getElementById('settings-rules').value.trim();
    if (!rulesText) { showToast('Please enter at least one rule', 'error'); return; }

    const rules = rulesText.split('\n').map(r => r.trim()).filter(r => r.length > 0);
    const resultDiv = document.getElementById('settings-derive-result');
    const btn = document.getElementById('btn-settings-derive');
    btn.disabled = true;
    resultDiv.innerHTML = loadingHTML('Running ' + rules.length + ' rules...');

    try {
      const result = await engram.derive({ rules });
      let html = '<div class="rule-results">';
      if (result.evaluated != null) html += '<div>Rules evaluated: <strong>' + result.evaluated + '</strong></div>';
      if (result.fired != null) html += '<div>Rules fired: <strong>' + result.fired + '</strong></div>';
      if (result.edges_created != null) html += '<div>Edges created: <strong>' + result.edges_created + '</strong></div>';
      if (result.flags && result.flags.length > 0) {
        html += '<div style="margin-top:0.5rem;color:var(--warning)"><strong>Flags:</strong></div>';
        result.flags.forEach(f => { html += '<div>  - ' + escapeHtml(f) + '</div>'; });
      }
      if (result.evaluated == null && result.fired == null) {
        html += '<pre>' + escapeHtml(JSON.stringify(result, null, 2)) + '</pre>';
      }
      html += '</div>';
      resultDiv.innerHTML = html;
      showToast('Rules executed', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Derive failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- Action Rules - Load ---
  document.getElementById('btn-load-action-rules').addEventListener('click', async () => {
    const text = document.getElementById('settings-action-rules').value.trim();
    if (!text) { showToast('Please enter rule definitions', 'error'); return; }

    let rules;
    try {
      rules = JSON.parse(text);
    } catch (err) {
      showToast('Invalid JSON: ' + err.message, 'error');
      return;
    }

    const resultDiv = document.getElementById('action-rules-result');
    try {
      await engram._post('/actions/rules', rules);
      resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Action rules loaded.</div>';
      showToast('Action rules loaded', 'success');
      loadActionRules();
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Failed to load rules: ' + err.message, 'error');
    }
  });

  // --- Quantization ---
  document.getElementById('btn-quantize-on').addEventListener('click', async () => {
    const resultDiv = document.getElementById('quantize-result');
    resultDiv.innerHTML = loadingHTML('Enabling quantization...');
    try {
      await engram._post('/quantize', { enabled: true });
      resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Quantization enabled.</div>';
      showToast('Quantization enabled', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Failed: ' + err.message, 'error');
    }
  });

  document.getElementById('btn-quantize-off').addEventListener('click', async () => {
    const resultDiv = document.getElementById('quantize-result');
    resultDiv.innerHTML = loadingHTML('Disabling quantization...');
    try {
      await engram._post('/quantize', { enabled: false });
      resultDiv.innerHTML = '<div style="color:var(--success);font-size:0.85rem"><i class="fa-solid fa-check"></i> Quantization disabled.</div>';
      showToast('Quantization disabled', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast('Failed: ' + err.message, 'error');
    }
  });

  // --- Explain Entity ---
  const explainInput = document.getElementById('settings-explain-label');
  document.getElementById('btn-settings-explain').addEventListener('click', doSettingsExplain);
  explainInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') doSettingsExplain(); });

  // --- Export JSON-LD ---
  document.getElementById('btn-export-jsonld').addEventListener('click', async () => {
    const btn = document.getElementById('btn-export-jsonld');
    const resultDiv = document.getElementById('export-result');
    btn.disabled = true;
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
      showToast('Export failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // --- Import JSON-LD ---
  document.getElementById('btn-import-jsonld').addEventListener('click', async () => {
    const text = document.getElementById('settings-import-jsonld').value.trim();
    if (!text) { showToast('Please paste JSON-LD data', 'error'); return; }

    let data;
    try {
      data = JSON.parse(text);
    } catch (err) {
      showToast('Invalid JSON: ' + err.message, 'error');
      return;
    }

    const btn = document.getElementById('btn-import-jsonld');
    const resultDiv = document.getElementById('import-result');
    btn.disabled = true;
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
      showToast('Import failed: ' + err.message, 'error');
    } finally {
      btn.disabled = false;
    }
  });
}

async function loadActionRules() {
  const container = document.getElementById('action-rules-list');
  if (!container) return;

  try {
    const rules = await engram._fetch('/actions/rules');
    const list = Array.isArray(rules) ? rules : (rules.rules || []);
    if (list.length > 0) {
      container.innerHTML = `
        <div style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.5rem">${list.length} rule${list.length !== 1 ? 's' : ''} loaded</div>
        <div style="display:flex;flex-direction:column;gap:0.3rem">
          ${list.map(r => `
            <div style="display:flex;justify-content:space-between;align-items:center;padding:0.4rem 0.5rem;background:var(--bg-secondary);border-radius:var(--radius-sm);font-size:0.85rem">
              <span><i class="fa-solid fa-bolt" style="color:var(--accent-bright);margin-right:0.4rem"></i>${escapeHtml(r.id || r.name || 'Unnamed rule')}</span>
              ${r.enabled !== false ? '<span style="color:var(--success)"><i class="fa-solid fa-circle-check"></i></span>' : '<span class="text-muted"><i class="fa-solid fa-circle-xmark"></i></span>'}
            </div>`).join('')}
        </div>`;
    } else {
      container.innerHTML = '<p class="text-muted" style="font-size:0.85rem">No action rules loaded.</p>';
    }
  } catch (_) {
    container.innerHTML = '<p class="text-muted" style="font-size:0.85rem">Action engine not available in this build.</p>';
  }
}

async function doSettingsExplain() {
  const label = document.getElementById('settings-explain-label').value.trim();
  if (!label) { showToast('Please enter a fact name', 'error'); return; }

  const resultDiv = document.getElementById('settings-explain-result');
  resultDiv.innerHTML = loadingHTML('Looking up provenance...');

  try {
    const data = await engram.explain(label);

    if (data.label) {
      let html = `
        <div class="card mt-1" style="background:var(--bg-input)">
          <h4 style="margin-bottom:0.5rem">${escapeHtml(data.label)}</h4>
          ${data.confidence != null ? confidenceBar(data.confidence) : ''}
          ${data.sources ? '<div class="mt-1"><strong>Sources:</strong> ' + data.sources.map(s => escapeHtml(s)).join(', ') + '</div>' : ''}
          ${data.co_occurrences && data.co_occurrences.length > 0
            ? '<div class="mt-1"><strong>Co-occurrences:</strong><ul class="edge-list">' + data.co_occurrences.map(c =>
                '<li><i class="fa-solid fa-link"></i> <a href="#/node/' + encodeURIComponent(c.label || c) + '">' + escapeHtml(c.label || c) + '</a>'
                + (c.count ? ' <span class="text-muted">(' + c.count + 'x)</span>' : '') + '</li>'
              ).join('') + '</ul></div>'
            : ''}
          <details class="mt-1" style="font-size:0.85rem">
            <summary class="text-muted" style="cursor:pointer">Raw JSON</summary>
            <pre style="margin-top:0.5rem;overflow-x:auto">${escapeHtml(JSON.stringify(data, null, 2))}</pre>
          </details>
        </div>`;
      resultDiv.innerHTML = html;
    } else {
      resultDiv.innerHTML = '<div class="rule-results"><pre>' + escapeHtml(JSON.stringify(data, null, 2)) + '</pre></div>';
    }
  } catch (err) {
    resultDiv.innerHTML = `<div style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
  }
}
