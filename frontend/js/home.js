/* ============================================
   engram - Home View (Dashboard)
   ============================================ */

router.register('/', async () => {
  renderTo(`
    <div class="hero-bar">
      <div class="hero-inner">
        <h2 class="hero-title"><i class="fa-solid fa-brain"></i> Your Knowledge Base</h2>
        <p class="hero-subtitle">Explore connections and discover insights.</p>
      </div>
    </div>

    <div class="grid-3 mb-2" id="home-stats">
      <div class="card stat-card">
        <div class="stat-icon"><i class="fa-solid fa-circle-nodes"></i></div>
        <div class="stat-value glow-text" id="stat-nodes">--</div>
        <div class="stat-label">Facts stored</div>
      </div>
      <div class="card stat-card">
        <div class="stat-icon"><i class="fa-solid fa-arrow-right-arrow-left"></i></div>
        <div class="stat-value glow-text" id="stat-edges">--</div>
        <div class="stat-label">Connections</div>
      </div>
      <div class="card stat-card">
        <div class="stat-icon"><i class="fa-solid fa-heart-pulse"></i></div>
        <div class="stat-value glow-text" id="stat-health">--</div>
        <div class="stat-label">Status</div>
      </div>
    </div>

    <div id="home-system-area"></div>

    <div id="home-sources-area"></div>

    <div id="home-onboarding-area"></div>

    <div class="card mb-2" id="recent-activity-card">
      <div class="card-header">
        <h3><i class="fa-solid fa-clock-rotate-left"></i> Overview</h3>
      </div>
      <div id="recent-activity">
        ${loadingHTML('Loading overview...')}
      </div>
    </div>
  `);

  loadHomeData();
});

async function loadHomeData() {
  try {
    const [stats, health, compute, config] = await Promise.all([
      engram.stats().catch(() => null),
      engram.health().catch(() => null),
      engram.compute().catch(() => null),
      engram.getConfig().catch(() => null),
    ]);

    if (stats) {
      const nodeCount = stats.nodes ?? 0;
      const edgeCount = stats.edges ?? 0;
      document.getElementById('stat-nodes').textContent = nodeCount.toLocaleString();
      document.getElementById('stat-edges').textContent = edgeCount.toLocaleString();

      // Show onboarding area (topics only — embedder config lives on System tab)
      const onboardingArea = document.getElementById('home-onboarding-area');
      if (onboardingArea) {
        if (nodeCount === 0) {
          showTopicsOnboarding(onboardingArea, false);
        } else {
          showTopicsCompact(onboardingArea);
        }
      }

      // Load system summary (non-blocking)
      loadSystemSummary(compute, config, nodeCount, edgeCount);

      // Load sources (non-blocking)
      loadSourcesSummary();

      // Recent activity / overview
      const activityEl = document.getElementById('recent-activity');
      if (nodeCount === 0 && edgeCount === 0) {
        activityEl.innerHTML = `
          <div class="empty-state" style="padding:2rem">
            <i class="fa-solid fa-seedling" style="font-size:2rem;color:var(--confidence-mid)"></i>
            <p style="margin-top:0.75rem;font-size:1.05rem">Your knowledge base is empty.</p>
            <p class="text-muted">Complete the onboarding steps above to get started, or head to <a href="#/add">Add Facts</a> to begin.</p>
          </div>`;
      } else {
        activityEl.innerHTML = `
          <div style="padding:0.75rem 1rem">
            <div style="display:flex;gap:2rem;flex-wrap:wrap;align-items:center">
              <div>
                <i class="fa-solid fa-database" style="color:var(--accent-bright);margin-right:0.4rem"></i>
                <strong>${nodeCount.toLocaleString()}</strong> facts stored with
                <strong>${edgeCount.toLocaleString()}</strong> connections between them.
              </div>
              <a href="#/explore" class="btn btn-sm btn-primary">
                <i class="fa-solid fa-magnifying-glass"></i> Explore now
              </a>
            </div>
          </div>`;
      }
    } else {
      document.getElementById('stat-nodes').textContent = '?';
      document.getElementById('stat-edges').textContent = '?';
      loadSystemSummary(null, null, 0, 0);
      loadSourcesSummary();
      const activityEl = document.getElementById('recent-activity');
      activityEl.innerHTML = `
        <div class="empty-state" style="padding:1.5rem">
          <i class="fa-solid fa-plug-circle-exclamation" style="font-size:1.5rem;color:var(--confidence-low)"></i>
          <p class="text-muted">Could not connect to the knowledge base. Check your settings.</p>
        </div>`;
    }

    if (health) {
      const statusEl = document.getElementById('stat-health');
      statusEl.textContent = health.status === 'ok' ? 'Online' : 'Error';
      statusEl.style.color = health.status === 'ok' ? 'var(--confidence-high)' : 'var(--error)';
    } else {
      const statusEl = document.getElementById('stat-health');
      statusEl.textContent = 'Offline';
      statusEl.style.color = 'var(--error)';
    }
  } catch (_) {}
}

// --- System Summary (health-focused) ---
async function loadSystemSummary(compute, config, nodeCount, edgeCount) {
  const container = document.getElementById('home-system-area');
  if (!container) return;

  // If compute wasn't passed, fetch it
  if (compute === undefined) {
    compute = await engram.compute().catch(() => null);
  }
  if (config === undefined) {
    config = await engram.getConfig().catch(() => null);
  }

  let sourcesEnabled = false, actionsEnabled = false, reasonEnabled = false, meshEnabled = false;

  function fetchWithTimeout(path, timeoutMs) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    return engram._fetch(path, { signal: controller.signal })
      .finally(() => clearTimeout(timer));
  }

  await Promise.allSettled([
    engram._fetch('/sources').then(() => { sourcesEnabled = true; }).catch(() => {}),
    engram._fetch('/actions/rules').then(() => { actionsEnabled = true; }).catch(() => {}),
    fetchWithTimeout('/reason/gaps', 2000).then(() => { reasonEnabled = true; }).catch(() => {}),
    engram._fetch('/mesh/identity').then(() => { meshEnabled = true; }).catch(() => {}),
  ]);

  // API status
  const apiOnline = !!(compute || config);

  // Hardware info
  let cpuHtml = '--';
  let gpuHtml = '<span class="text-muted">None</span>';
  let npuHtml = '';
  if (compute) {
    cpuHtml = `${compute.cpu_cores} cores`;
    if (compute.has_avx2) cpuHtml += ' <span class="text-muted" style="font-size:0.75rem">AVX2</span>';
    if (compute.has_gpu && compute.gpu_name) {
      gpuHtml = `<span style="color:var(--success)">${escapeHtml(compute.gpu_name)}</span>`;
      if (compute.gpu_backend) gpuHtml += ` <span class="text-muted" style="font-size:0.75rem">${escapeHtml(compute.gpu_backend)}</span>`;
    }
    if (compute.has_npu && compute.npu_name) {
      npuHtml = `
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-brain" style="color:var(--text-muted)"></i>
          <span class="text-secondary">NPU:</span>
          <span style="color:var(--success)">${escapeHtml(compute.npu_name)}</span>
        </div>`;
    }
  }

  // Feature dot helper
  function featureDot(label, enabled) {
    if (enabled) {
      return `<span class="feature-status" style="display:inline-flex;align-items:center;gap:0.3rem;font-size:0.8rem">
        <i class="fa-solid fa-circle" style="font-size:0.45rem;color:var(--success)"></i> ${escapeHtml(label)}
      </span>`;
    }
    return `<span class="feature-status" style="display:inline-flex;align-items:center;gap:0.3rem;font-size:0.8rem;color:var(--text-muted)">
      <i class="fa-solid fa-circle" style="font-size:0.45rem;color:var(--accent-bright)"></i> ${escapeHtml(label)}
    </span>`;
  }

  container.innerHTML = `
    <div class="card mb-2">
      <div class="card-header">
        <h3><i class="fa-solid fa-microchip"></i> Hardware</h3>
      </div>
      <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(220px, 1fr));gap:0.6rem 1.5rem;font-size:0.85rem;padding:0.25rem 0">
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-signal" style="color:var(--text-muted)"></i>
          <span class="text-secondary">API:</span>
          ${apiOnline
            ? '<span style="color:var(--success)">Online</span>'
            : '<span style="color:var(--error)">Offline</span>'}
        </div>
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-microchip" style="color:var(--text-muted)"></i>
          <span class="text-secondary">CPU:</span>
          <span>${cpuHtml}</span>
        </div>
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-display" style="color:var(--text-muted)"></i>
          <span class="text-secondary">GPU:</span>
          ${gpuHtml}
        </div>
        ${npuHtml}
      </div>
      <div style="display:flex;flex-wrap:wrap;gap:0.75rem;margin-top:0.75rem;padding-bottom:0.25rem">
        ${featureDot('Ingest', sourcesEnabled)}
        ${featureDot('Actions', actionsEnabled)}
        ${featureDot('Reasoning', reasonEnabled)}
        ${featureDot('Mesh', meshEnabled)}
      </div>
    </div>`;
}

// --- Sources Summary ---
async function loadSourcesSummary() {
  const container = document.getElementById('home-sources-area');
  if (!container) return;

  let sources = null;
  try {
    sources = await engram.listSources();
  } catch (_) {
    // listSources not available or errored
    container.innerHTML = `
      <div class="card mb-2">
        <div class="card-header">
          <h3><i class="fa-solid fa-database"></i> Sources</h3>
        </div>
        <div style="padding:0.5rem 0;font-size:0.85rem;display:flex;align-items:center;gap:0.5rem">
          <i class="fa-solid fa-circle" style="font-size:0.45rem;color:var(--accent-bright)"></i>
          <span>Available</span>
          <a href="#/sources" style="font-size:0.8rem;color:var(--accent-bright)">[Set up]</a>
        </div>
      </div>`;
    return;
  }

  if (!sources || !Array.isArray(sources) || sources.length === 0) {
    container.innerHTML = `
      <div class="card mb-2">
        <div class="card-header">
          <h3><i class="fa-solid fa-database"></i> Active Sources</h3>
          <button class="btn btn-sm btn-primary" onclick="if(typeof openSourceWizard==='function')openSourceWizard();else location.hash='#/sources'">
            <i class="fa-solid fa-plus"></i> Add Source
          </button>
        </div>
        <div style="padding:0.75rem 0">
          ${emptyStateHTML('fa-plug', 'No sources configured yet. Add a source to start ingesting knowledge.')}
        </div>
      </div>`;
    return;
  }

  // Fetch usage info for each source (non-blocking, best-effort)
  const usageMap = {};
  await Promise.allSettled(
    sources.map(async (src) => {
      const name = typeof src === 'string' ? src : (src.name || src.id || '');
      if (!name) return;
      try {
        usageMap[name] = await engram.sourceUsage(name);
      } catch (_) {}
    })
  );

  function renderSourceRow(src) {
    const name = typeof src === 'string' ? src : (src.name || src.id || 'unknown');
    const status = (typeof src === 'object' && src.status) ? src.status : 'active';
    const isError = status === 'error' || status === 'failed';
    const usage = usageMap[name];

    const dotColor = isError ? 'var(--error)' : 'var(--success)';
    const statusLabel = isError ? 'Error' : 'Active';

    let statsHtml = '';
    if (usage) {
      const facts = usage.facts ?? usage.fact_count ?? usage.nodes ?? '--';
      const errors = usage.errors ?? usage.error_count ?? 0;
      const lastRun = usage.last_run || usage.last_fetch || null;
      let lastRunText = '--';
      if (lastRun) {
        try {
          const d = new Date(lastRun);
          const diff = Date.now() - d.getTime();
          if (diff < 60000) lastRunText = 'just now';
          else if (diff < 3600000) lastRunText = Math.floor(diff / 60000) + ' min ago';
          else if (diff < 86400000) lastRunText = Math.floor(diff / 3600000) + 'h ago';
          else lastRunText = Math.floor(diff / 86400000) + 'd ago';
        } catch (_) { lastRunText = '--'; }
      }
      statsHtml = `<span class="text-muted" style="font-size:0.8rem">${facts} facts | ${lastRunText} | ${errors} errors</span>`;
    }

    return `
      <div class="source-compact" style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
        <div class="source-status" style="display:flex;align-items:center;gap:0.4rem;min-width:0;flex:1">
          <i class="fa-solid fa-circle" style="font-size:0.45rem;color:${dotColor}"></i>
          <span style="font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">${escapeHtml(name)}</span>
          <span style="font-size:0.78rem;color:${dotColor}">${escapeHtml(statusLabel)}</span>
        </div>
        <div class="source-usage" style="flex-shrink:0">
          ${statsHtml}
        </div>
      </div>`;
  }

  container.innerHTML = `
    <div class="card mb-2">
      <div class="card-header">
        <h3><i class="fa-solid fa-database"></i> Active Sources</h3>
        <button class="btn btn-sm btn-primary" onclick="if(typeof openSourceWizard==='function')openSourceWizard();else location.hash='#/sources'">
          <i class="fa-solid fa-plus"></i> Add Source
        </button>
      </div>
      <div style="padding:0.25rem 0">
        ${sources.map(renderSourceRow).join('')}
      </div>
    </div>`;
}

// --- Seed Knowledge: statement-first approach with NER analysis ---

const SEED_INSPIRATIONS = [
  { label: 'Ukraine-Russia war', text: 'Russia invaded Ukraine in February 2022, leading to NATO expansion with Finland and Sweden joining the alliance. The European Union imposed sanctions on Moscow while the United States provided military aid to Kyiv.' },
  { label: 'Gold investment', text: 'Gold prices surged past $2,400 per ounce as central banks in China, India and Turkey increased reserves amid inflation fears. The Federal Reserve interest rate decisions directly impact gold demand.' },
  { label: 'AI chip exports', text: 'The United States imposed export controls on NVIDIA and AMD AI chips to China, affecting companies like TSMC and Samsung. Huawei developed the Ascend 910B as an alternative processor.' },
  { label: 'EU energy policy', text: 'The European Union committed to 42.5% renewable energy by 2030, with Germany and France leading offshore wind investment. Norway supplies natural gas while Russia was cut from the Nord Stream pipeline.' },
  { label: 'SpaceX Mars', text: 'SpaceX launched Starship from Boca Chica, Texas with CEO Elon Musk targeting Mars colonization by 2030. NASA partnered with SpaceX for the Artemis lunar program.' },
  { label: 'BRICS expansion', text: 'BRICS expanded to include Saudi Arabia, Egypt and Ethiopia as members push for alternatives to the US dollar. China and Brazil signed bilateral trade agreements bypassing the dollar.' },
];

function showTopicsOnboarding(container) {
  const inspiration = SEED_INSPIRATIONS[Math.floor(Math.random() * SEED_INSPIRATIONS.length)];

  container.innerHTML = `
    <div class="card mb-2" id="seed-card">
      <div class="card-header">
        <h3><i class="fa-solid fa-seedling"></i> Seed Your Knowledge</h3>
      </div>
      <p style="font-size:0.92rem;color:var(--text-secondary);margin-bottom:0.75rem">
        Describe what you want to track. Write a statement, paste a paragraph, or pick an example below.
      </p>

      <textarea id="seed-text" rows="4" placeholder="e.g. ${inspiration.label} — ${inspiration.text.substring(0, 80)}..."
        style="width:100%;padding:0.6rem 0.75rem;border:1px solid var(--border);border-radius:var(--radius-sm);
        background:var(--bg-input);color:var(--text-primary);font-size:0.9rem;resize:vertical;
        font-family:inherit;line-height:1.5"></textarea>

      <div style="display:flex;align-items:center;gap:0.75rem;margin-top:0.75rem;flex-wrap:wrap">
        <button class="btn btn-primary" id="seed-analyze-btn">
          <i class="fa-solid fa-magnifying-glass-chart"></i> Analyze
        </button>
        <span style="font-size:0.8rem;color:var(--text-muted)">or try:</span>
        <div style="display:flex;flex-wrap:wrap;gap:0.4rem" id="seed-suggestions">
          ${SEED_INSPIRATIONS.slice(0, 4).map(s => `
            <button class="seed-suggestion" data-text="${escapeHtml(s.text)}" style="
              padding:0.3rem 0.65rem;font-size:0.78rem;border:1px solid var(--border);
              border-radius:999px;background:var(--bg-secondary);color:var(--text-secondary);
              cursor:pointer;transition:all 0.15s;white-space:nowrap;
            ">${escapeHtml(s.label)}</button>
          `).join('')}
        </div>
      </div>

      <div id="seed-results" style="display:none;margin-top:1rem">
        <div style="font-size:0.85rem;color:var(--text-muted);margin-bottom:0.5rem">
          <i class="fa-solid fa-diagram-project"></i> Extracted entities
          <span id="seed-lang" style="margin-left:0.5rem"></span>
          <span id="seed-timing" style="margin-left:0.5rem"></span>
        </div>
        <div id="seed-entity-list" style="display:flex;flex-direction:column;gap:0.3rem;margin-bottom:1rem"></div>
        <div style="display:flex;gap:0.75rem;align-items:center">
          <button class="btn btn-primary" id="seed-commit-btn">
            <i class="fa-solid fa-plus"></i> Seed Knowledge Base
          </button>
          <button class="btn btn-secondary" id="seed-reset-btn">
            <i class="fa-solid fa-arrow-left"></i> Back
          </button>
          <span id="seed-status" style="font-size:0.85rem"></span>
        </div>
      </div>
    </div>`;

  attachSeedHandlers(container);
}

// When DB has data, show compact add-more section
function showTopicsCompact(container) {
  container.innerHTML = `
    <div class="card mb-2" id="seed-card">
      <div class="card-header">
        <h3><i class="fa-solid fa-plus-circle"></i> Add Knowledge</h3>
      </div>
      <div style="display:flex;gap:0.5rem;align-items:stretch">
        <textarea id="seed-text" rows="2" placeholder="Paste or type new information to analyze and add..."
          style="flex:1;padding:0.5rem 0.75rem;border:1px solid var(--border);border-radius:var(--radius-sm);
          background:var(--bg-input);color:var(--text-primary);font-size:0.85rem;resize:none;
          font-family:inherit;line-height:1.4"></textarea>
        <button class="btn btn-primary" id="seed-analyze-btn" style="align-self:stretch;white-space:nowrap">
          <i class="fa-solid fa-magnifying-glass-chart"></i> Analyze
        </button>
      </div>
      <div id="seed-results" style="display:none;margin-top:0.75rem">
        <div style="font-size:0.85rem;color:var(--text-muted);margin-bottom:0.5rem">
          <i class="fa-solid fa-diagram-project"></i> Extracted entities
          <span id="seed-lang" style="margin-left:0.5rem"></span>
          <span id="seed-timing" style="margin-left:0.5rem"></span>
        </div>
        <div id="seed-entity-list" style="display:flex;flex-direction:column;gap:0.3rem;margin-bottom:0.75rem"></div>
        <div style="display:flex;gap:0.75rem;align-items:center">
          <button class="btn btn-primary btn-sm" id="seed-commit-btn">
            <i class="fa-solid fa-plus"></i> Add to Knowledge Base
          </button>
          <button class="btn btn-secondary btn-sm" id="seed-reset-btn">
            <i class="fa-solid fa-xmark"></i> Clear
          </button>
          <span id="seed-status" style="font-size:0.85rem"></span>
        </div>
      </div>
    </div>`;

  attachSeedHandlers(container);
}

function attachSeedHandlers(container) {
  let analyzedEntities = [];
  let analyzedText = '';

  // Suggestion clicks fill the textarea
  container.querySelectorAll('.seed-suggestion').forEach(btn => {
    btn.addEventListener('click', () => {
      const ta = document.getElementById('seed-text');
      if (ta) ta.value = btn.dataset.text || btn.textContent.trim();
    });
  });

  // Analyze button
  const analyzeBtn = container.querySelector('#seed-analyze-btn');
  if (analyzeBtn) {
    analyzeBtn.addEventListener('click', async () => {
      const ta = document.getElementById('seed-text');
      const text = ta?.value.trim();
      if (!text) { showToast('Enter some text to analyze', 'error'); return; }

      analyzeBtn.disabled = true;
      analyzeBtn.innerHTML = '<span class="spinner"></span> Analyzing...';

      try {
        const res = await engram.ingestAnalyze(text);
        analyzedEntities = res.entities || [];
        analyzedText = text;

        const resultsDiv = document.getElementById('seed-results');
        const entityList = document.getElementById('seed-entity-list');
        const langSpan = document.getElementById('seed-lang');
        const timingSpan = document.getElementById('seed-timing');

        if (langSpan) langSpan.textContent = res.language ? `[${res.language}]` : '';
        if (timingSpan) timingSpan.textContent = res.duration_ms != null ? `${res.duration_ms}ms` : '';

        if (analyzedEntities.length === 0) {
          entityList.innerHTML = `
            <div style="padding:0.5rem;font-size:0.85rem;color:var(--text-muted)">
              <i class="fa-solid fa-info-circle"></i> No entities extracted. Try a more descriptive statement with names, places, or organizations.
            </div>`;
        } else {
          entityList.innerHTML = analyzedEntities.map((e, i) => `
            <div class="seed-entity-row" data-idx="${i}" style="
              display:flex;align-items:center;gap:0.6rem;padding:0.45rem 0.6rem;
              background:var(--bg-input);border-radius:var(--radius-sm);font-size:0.85rem;
            ">
              <input type="checkbox" checked data-entity-idx="${i}"
                style="flex-shrink:0;accent-color:var(--accent-bright)">
              <span style="font-weight:500;flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis">${escapeHtml(e.text)}</span>
              <span style="
                padding:0.15rem 0.45rem;border-radius:999px;font-size:0.72rem;font-weight:600;
                background:var(--bg-secondary);border:1px solid var(--border);color:var(--text-secondary);
                white-space:nowrap;text-transform:uppercase;
              ">${escapeHtml(e.entity_type)}</span>
              <span style="font-size:0.78rem;color:var(--text-muted);white-space:nowrap">${(e.confidence * 100).toFixed(0)}%</span>
              ${e.resolved_to != null
                ? '<i class="fa-solid fa-link" style="color:var(--success);font-size:0.7rem" title="Resolved to existing node"></i>'
                : '<i class="fa-solid fa-sparkles" style="color:var(--accent-bright);font-size:0.7rem" title="New entity"></i>'}
            </div>
          `).join('');
        }

        if (resultsDiv) resultsDiv.style.display = 'block';
      } catch (err) {
        showToast('Analysis failed: ' + (err.message || err), 'error');
      } finally {
        analyzeBtn.disabled = false;
        analyzeBtn.innerHTML = '<i class="fa-solid fa-magnifying-glass-chart"></i> Analyze';
      }
    });
  }

  // Commit button — ingest selected entities
  const commitBtn = container.querySelector('#seed-commit-btn');
  if (commitBtn) {
    commitBtn.addEventListener('click', async () => {
      // Get checked entity indices
      const checked = [];
      container.querySelectorAll('input[data-entity-idx]').forEach(cb => {
        if (cb.checked) checked.push(parseInt(cb.dataset.entityIdx));
      });

      if (checked.length === 0) { showToast('Select at least one entity', 'error'); return; }

      const statusEl = document.getElementById('seed-status');
      commitBtn.disabled = true;
      if (statusEl) statusEl.innerHTML = '<span class="spinner"></span> Storing...';

      try {
        // Send the original text through the full ingest pipeline
        const res = await engram.ingest({
          items: [analyzedText],
          source: 'seed',
        });

        const count = res.facts_stored || 0;
        if (statusEl) statusEl.innerHTML = `<span style="color:var(--success)"><i class="fa-solid fa-check"></i> ${count} entities stored</span>`;
        showToast(`Seeded ${count} entities into knowledge base`, 'success');

        setTimeout(() => loadHomeData(), 500);
      } catch (err) {
        if (statusEl) statusEl.innerHTML = `<span style="color:var(--error)"><i class="fa-solid fa-xmark"></i> ${err.message || err}</span>`;
        showToast('Failed to store: ' + (err.message || err), 'error');
      } finally {
        commitBtn.disabled = false;
      }
    });
  }

  // Reset button
  const resetBtn = container.querySelector('#seed-reset-btn');
  if (resetBtn) {
    resetBtn.addEventListener('click', () => {
      const resultsDiv = document.getElementById('seed-results');
      if (resultsDiv) resultsDiv.style.display = 'none';
      analyzedEntities = [];
      analyzedText = '';
      const statusEl = document.getElementById('seed-status');
      if (statusEl) statusEl.innerHTML = '';
    });
  }
}
