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

      // Determine embedder status
      const embedderConfigured = !!(compute && (compute.embedder_model || compute.embedder_endpoint));

      // Show onboarding area
      const onboardingArea = document.getElementById('home-onboarding-area');
      if (onboardingArea) {
        if (!embedderConfigured) {
          showEmbedderOnboarding(onboardingArea);
        } else if (nodeCount === 0) {
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

  // Embedder info
  let embedderHtml = '';
  if (compute && compute.embedder_model) {
    let embedText = escapeHtml(compute.embedder_model);
    if (compute.embedder_dim) embedText += ' (' + compute.embedder_dim + 'D)';
    embedderHtml = `<span style="color:var(--success)">${embedText}</span>`;
  } else if (compute && compute.embedder_endpoint) {
    embedderHtml = `<span style="color:var(--success)">${escapeHtml(compute.embedder_endpoint)}</span>`;
  } else {
    embedderHtml = `<span class="text-muted">Not configured</span> <a href="#/system" style="font-size:0.8rem;color:var(--accent-bright)">[Set up]</a>`;
  }

  // LLM info
  let llmHtml = '';
  if (config && config.llm_model) {
    llmHtml = `<span style="color:var(--success)">${escapeHtml(config.llm_model)}</span>`;
  } else if (config && config.llm_endpoint) {
    llmHtml = `<span style="color:var(--success)">${escapeHtml(config.llm_endpoint)}</span>`;
  } else {
    llmHtml = `<span class="text-muted">Not configured</span> <a href="#/system" style="font-size:0.8rem;color:var(--accent-bright)">[Set up]</a>`;
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
        <h3><i class="fa-solid fa-stethoscope"></i> System Health</h3>
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
          <i class="fa-solid fa-vector-square" style="color:var(--text-muted)"></i>
          <span class="text-secondary">Embedder:</span>
          ${embedderHtml}
        </div>
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-robot" style="color:var(--text-muted)"></i>
          <span class="text-secondary">LLM:</span>
          ${llmHtml}
        </div>
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-circle-nodes" style="color:var(--text-muted)"></i>
          <span class="text-secondary">Facts:</span>
          <span>${(nodeCount ?? 0).toLocaleString()}</span>
        </div>
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-arrow-right-arrow-left" style="color:var(--text-muted)"></i>
          <span class="text-secondary">Connections:</span>
          <span>${(edgeCount ?? 0).toLocaleString()}</span>
        </div>
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

// --- Embedder Onboarding (compact banner) ---
function showEmbedderOnboarding(container) {
  container.innerHTML = `
    <div class="card mb-2">
      <div class="card-header">
        <h3><i class="fa-solid fa-wand-magic-sparkles"></i> Set Up Embedding Model</h3>
        <span style="font-size:0.78rem;padding:0.15rem 0.5rem;border-radius:999px;background:var(--accent-bright);color:#fff">Step 1 of 2</span>
      </div>
      <p style="font-size:0.9rem;color:var(--text-secondary);margin-bottom:0.5rem">
        Choose your embedding model for semantic search. This determines how engram understands meaning.
      </p>
      <div style="display:flex;align-items:center;gap:0.4rem;padding:0.4rem 0.6rem;background:rgba(227,160,8,0.1);border:1px solid rgba(227,160,8,0.3);border-radius:var(--radius-sm);font-size:0.82rem;color:var(--confidence-mid);margin-bottom:0.75rem">
        <i class="fa-solid fa-triangle-exclamation"></i>
        <span>Changing your model after loading data requires a full reindex.</span>
      </div>
      <div style="display:flex;flex-wrap:wrap;gap:0.5rem">
        <a href="#/system" class="btn btn-sm btn-secondary">
          <i class="fa-solid fa-microchip"></i> ONNX Local
        </a>
        <a href="#/system" class="btn btn-sm btn-secondary">
          <i class="fa-solid fa-server"></i> Ollama
        </a>
        <a href="#/system" class="btn btn-sm btn-secondary">
          <i class="fa-solid fa-cloud"></i> OpenAI
        </a>
      </div>
    </div>`;
}

// --- Default + custom topics ---
function getDefaultTopics() {
  return [
    { id: 'tech',      icon: 'fa-microchip',       label: 'Technology',     examples: ['JavaScript is a programming language', 'React is a frontend framework', 'PostgreSQL is a relational database'] },
    { id: 'science',   icon: 'fa-flask',            label: 'Science',        examples: ['DNA stores genetic information', 'Photosynthesis converts light to energy', 'The speed of light is 299792458 m/s'] },
    { id: 'business',  icon: 'fa-briefcase',        label: 'Business',       examples: ['Revenue minus costs equals profit', 'Market cap measures company value', 'ROI stands for Return on Investment'] },
    { id: 'people',    icon: 'fa-users',            label: 'People & Orgs',  examples: ['Linus Torvalds created Linux', 'Tim Berners-Lee invented the web', 'Alan Turing pioneered computing'] },
    { id: 'geo',       icon: 'fa-earth-americas',   label: 'Geography',      examples: ['Tokyo is the capital of Japan', 'The Amazon is the largest river by volume', 'Mount Everest is 8849 meters tall'] },
    { id: 'politics',  icon: 'fa-landmark',         label: 'Politics',       examples: ['Democracy means rule by the people', 'The UN was founded in 1945', 'Separation of powers divides government into branches'] },
    { id: 'health',    icon: 'fa-heart-pulse',       label: 'Health',         examples: ['The human body has 206 bones', 'Insulin regulates blood sugar', 'Vaccines train the immune system'] },
    { id: 'personal',  icon: 'fa-user-pen',         label: 'Personal Notes', examples: ['My project deadline is next Friday', 'Meeting with team at 3pm', 'Remember to review the API docs'] },
  ];
}

function getCustomTopics() {
  try {
    const raw = localStorage.getItem('engram_topics');
    return raw ? JSON.parse(raw) : [];
  } catch (_) { return []; }
}

function saveCustomTopics(topics) {
  localStorage.setItem('engram_topics', JSON.stringify(topics));
}

const TOPIC_ICON_OPTIONS = [
  { value: 'fa-star',            label: 'Star' },
  { value: 'fa-book',            label: 'Book' },
  { value: 'fa-code',            label: 'Code' },
  { value: 'fa-music',           label: 'Music' },
  { value: 'fa-palette',         label: 'Art' },
  { value: 'fa-gamepad',         label: 'Gaming' },
  { value: 'fa-utensils',        label: 'Food' },
  { value: 'fa-plane',           label: 'Travel' },
  { value: 'fa-graduation-cap',  label: 'Education' },
  { value: 'fa-film',            label: 'Film' },
  { value: 'fa-futbol',          label: 'Sports' },
  { value: 'fa-leaf',            label: 'Nature' },
  { value: 'fa-gavel',           label: 'Law' },
  { value: 'fa-chart-line',      label: 'Finance' },
  { value: 'fa-wrench',          label: 'Tools' },
];

function getAllTopics() {
  return [...getDefaultTopics(), ...getCustomTopics()];
}

// --- Step 2: Topics Onboarding (empty DB, embedder configured) ---
function showTopicsOnboarding(container, compact) {
  const allTopics = getAllTopics();

  container.innerHTML = `
    <div class="card mb-2" id="onboarding-topics-card">
      <div class="card-header">
        <h3><i class="fa-solid fa-tags"></i> Seed Your Topics</h3>
        <span style="font-size:0.78rem;padding:0.15rem 0.5rem;border-radius:999px;background:var(--accent-bright);color:#fff">Step 2 of 2</span>
      </div>
      <p style="font-size:0.92rem;color:var(--text-secondary);margin-bottom:1rem">
        Pick a topic below to seed your knowledge base with starter facts, or create your own topics.
      </p>

      <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(180px, 1fr));gap:0.75rem;margin-bottom:1rem" id="onboarding-topics-grid">
        ${renderTopicButtons(allTopics)}
      </div>

      <div style="margin-bottom:1rem">
        <button class="btn btn-sm btn-secondary" id="add-topic-toggle">
          <i class="fa-solid fa-plus"></i> Add Topic
        </button>
      </div>

      <div id="add-topic-form" style="display:none;padding:0.75rem;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);margin-bottom:1rem">
        <div style="display:flex;gap:0.5rem;align-items:flex-end;flex-wrap:wrap">
          <div style="flex:1;min-width:150px">
            <label style="font-size:0.8rem;color:var(--text-muted);display:block;margin-bottom:0.25rem">Topic Name</label>
            <input type="text" id="new-topic-name" placeholder="e.g. Cooking" style="width:100%;padding:0.4rem 0.6rem;border:1px solid var(--border);border-radius:var(--radius-sm);background:var(--bg-input);color:var(--text-primary);font-size:0.9rem">
          </div>
          <div style="min-width:120px">
            <label style="font-size:0.8rem;color:var(--text-muted);display:block;margin-bottom:0.25rem">Icon</label>
            <select id="new-topic-icon" style="width:100%;padding:0.4rem 0.6rem;border:1px solid var(--border);border-radius:var(--radius-sm);background:var(--bg-input);color:var(--text-primary);font-size:0.9rem">
              ${TOPIC_ICON_OPTIONS.map(ic => `<option value="${ic.value}">${ic.label}</option>`).join('')}
            </select>
          </div>
          <button class="btn btn-sm btn-primary" id="add-topic-submit">
            <i class="fa-solid fa-plus"></i> Add
          </button>
        </div>
      </div>

      <div id="topic-preview" style="display:none">
        <div style="font-size:0.85rem;color:var(--text-muted);margin-bottom:0.5rem">
          <i class="fa-solid fa-eye"></i> Preview: these facts will be added
        </div>
        <div id="topic-preview-list" style="display:flex;flex-direction:column;gap:0.3rem;margin-bottom:1rem"></div>
        <div style="display:flex;gap:0.75rem;align-items:center">
          <button class="btn btn-primary" id="topic-add-facts">
            <i class="fa-solid fa-plus"></i> Add These Facts
          </button>
          <button class="btn btn-secondary" id="topic-back">
            <i class="fa-solid fa-arrow-left"></i> Back
          </button>
          <span id="topic-status" style="font-size:0.85rem"></span>
        </div>
      </div>
    </div>`;

  attachTopicHandlers(container);
}

// --- Compact topics (DB has data, embedder configured) ---
function showTopicsCompact(container) {
  const allTopics = getAllTopics();

  container.innerHTML = `
    <div class="card mb-2" id="onboarding-topics-card">
      <div class="card-header">
        <h3><i class="fa-solid fa-tags"></i> Topics</h3>
      </div>
      <div style="display:flex;flex-wrap:wrap;gap:0.5rem;margin-bottom:0.75rem" id="onboarding-topics-grid">
        ${allTopics.map(t => `
          <a href="#/explore?q=${encodeURIComponent(t.label)}" style="
            display:inline-flex;align-items:center;gap:0.4rem;padding:0.35rem 0.75rem;
            background:var(--bg-secondary);border:1px solid var(--border);border-radius:999px;
            font-size:0.85rem;color:var(--text-primary);text-decoration:none;transition:all 0.15s;
          " class="topic-pill">
            <i class="fa-solid ${t.icon}" style="color:var(--accent-bright);font-size:0.8rem"></i>
            ${escapeHtml(t.label)}
          </a>
        `).join('')}
      </div>
      <div style="margin-bottom:0.5rem">
        <button class="btn btn-sm btn-secondary" id="add-topic-toggle" style="font-size:0.8rem">
          <i class="fa-solid fa-plus"></i> Add Topic
        </button>
      </div>
      <div id="add-topic-form" style="display:none;padding:0.75rem;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);margin-bottom:0.5rem">
        <div style="display:flex;gap:0.5rem;align-items:flex-end;flex-wrap:wrap">
          <div style="flex:1;min-width:150px">
            <label style="font-size:0.8rem;color:var(--text-muted);display:block;margin-bottom:0.25rem">Topic Name</label>
            <input type="text" id="new-topic-name" placeholder="e.g. Cooking" style="width:100%;padding:0.4rem 0.6rem;border:1px solid var(--border);border-radius:var(--radius-sm);background:var(--bg-input);color:var(--text-primary);font-size:0.9rem">
          </div>
          <div style="min-width:120px">
            <label style="font-size:0.8rem;color:var(--text-muted);display:block;margin-bottom:0.25rem">Icon</label>
            <select id="new-topic-icon" style="width:100%;padding:0.4rem 0.6rem;border:1px solid var(--border);border-radius:var(--radius-sm);background:var(--bg-input);color:var(--text-primary);font-size:0.9rem">
              ${TOPIC_ICON_OPTIONS.map(ic => `<option value="${ic.value}">${ic.label}</option>`).join('')}
            </select>
          </div>
          <button class="btn btn-sm btn-primary" id="add-topic-submit">
            <i class="fa-solid fa-plus"></i> Add
          </button>
        </div>
      </div>
    </div>`;

  // Add topic toggle + submit handlers
  const toggleBtn = container.querySelector('#add-topic-toggle');
  const form = container.querySelector('#add-topic-form');
  if (toggleBtn && form) {
    toggleBtn.addEventListener('click', () => {
      form.style.display = form.style.display === 'none' ? 'block' : 'none';
    });
  }

  const submitBtn = container.querySelector('#add-topic-submit');
  if (submitBtn) {
    submitBtn.addEventListener('click', () => {
      const nameInput = document.getElementById('new-topic-name');
      const iconSelect = document.getElementById('new-topic-icon');
      const name = nameInput.value.trim();
      if (!name) { showToast('Please enter a topic name', 'error'); return; }

      const custom = getCustomTopics();
      const allExisting = getAllTopics();
      if (allExisting.some(t => t.label.toLowerCase() === name.toLowerCase())) {
        showToast('Topic already exists', 'error');
        return;
      }

      custom.push({
        id: 'custom_' + Date.now(),
        icon: iconSelect.value,
        label: name,
        examples: [],
      });
      saveCustomTopics(custom);
      showToast('Topic added', 'success');
      showTopicsCompact(container);
    });
  }
}

function renderTopicButtons(topics) {
  return topics.map(t => `
    <button class="wizard-topic-btn" data-topic="${escapeHtml(t.id)}" style="
      display:flex;align-items:center;gap:0.6rem;padding:0.75rem;
      background:var(--bg-secondary);border:2px solid var(--border);border-radius:var(--radius-sm);
      cursor:pointer;transition:all 0.15s;font-size:0.9rem;text-align:left;
    ">
      <i class="fa-solid ${t.icon}" style="font-size:1.1rem;color:var(--accent-bright);flex-shrink:0"></i>
      <span>${escapeHtml(t.label)}</span>
    </button>
  `).join('');
}

function attachTopicHandlers(container) {
  const topicMap = {};
  getAllTopics().forEach(t => { topicMap[t.id] = t; });

  let selectedTopic = null;

  // Add topic toggle
  const toggleBtn = container.querySelector('#add-topic-toggle');
  const form = container.querySelector('#add-topic-form');
  if (toggleBtn && form) {
    toggleBtn.addEventListener('click', () => {
      form.style.display = form.style.display === 'none' ? 'block' : 'none';
    });
  }

  // Add topic submit
  const submitBtn = container.querySelector('#add-topic-submit');
  if (submitBtn) {
    submitBtn.addEventListener('click', () => {
      const nameInput = document.getElementById('new-topic-name');
      const iconSelect = document.getElementById('new-topic-icon');
      const name = nameInput.value.trim();
      if (!name) { showToast('Please enter a topic name', 'error'); return; }

      const custom = getCustomTopics();
      const allExisting = getAllTopics();
      if (allExisting.some(t => t.label.toLowerCase() === name.toLowerCase())) {
        showToast('Topic already exists', 'error');
        return;
      }

      custom.push({
        id: 'custom_' + Date.now(),
        icon: iconSelect.value,
        label: name,
        examples: [],
      });
      saveCustomTopics(custom);
      showToast('Topic added', 'success');

      // Re-render
      showTopicsOnboarding(container, false);
    });
  }

  // Topic button clicks
  container.querySelectorAll('.wizard-topic-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      selectedTopic = topicMap[btn.dataset.topic];
      if (!selectedTopic) return;

      // Highlight selected
      container.querySelectorAll('.wizard-topic-btn').forEach(b => {
        b.style.borderColor = b === btn ? 'var(--accent-bright)' : 'var(--border)';
        b.style.background = b === btn ? 'var(--bg-input)' : 'var(--bg-secondary)';
      });

      const previewDiv = document.getElementById('topic-preview');
      const previewList = document.getElementById('topic-preview-list');

      if (!selectedTopic.examples || selectedTopic.examples.length === 0) {
        // Custom topic with no seed facts
        if (previewDiv) previewDiv.style.display = 'block';
        if (previewList) previewList.innerHTML = `
          <div style="font-size:0.85rem;color:var(--text-muted);padding:0.5rem">
            <i class="fa-solid fa-info-circle"></i> This is a custom topic with no seed facts.
            Head to <a href="#/add">Add Facts</a> to start adding knowledge.
          </div>`;
        const addBtn = document.getElementById('topic-add-facts');
        if (addBtn) addBtn.style.display = 'none';
        return;
      }

      // Show preview
      if (previewDiv) previewDiv.style.display = 'block';
      if (previewList) {
        previewList.innerHTML = selectedTopic.examples.map(ex => `
          <div style="display:flex;align-items:center;gap:0.5rem;padding:0.4rem 0.6rem;background:var(--bg-input);border-radius:var(--radius-sm);font-size:0.85rem">
            <i class="fa-solid fa-circle-plus" style="color:var(--success);flex-shrink:0"></i>
            <span>${escapeHtml(ex)}</span>
          </div>
        `).join('');
      }
      const addBtn = document.getElementById('topic-add-facts');
      if (addBtn) addBtn.style.display = '';
      document.getElementById('topic-status').innerHTML = '';
    });
  });

  // Add facts button
  container.querySelector('#topic-add-facts')?.addEventListener('click', async () => {
    if (!selectedTopic || !selectedTopic.examples || selectedTopic.examples.length === 0) return;
    const btn = document.getElementById('topic-add-facts');
    const statusEl = document.getElementById('topic-status');
    btn.disabled = true;
    statusEl.innerHTML = '<span class="spinner"></span> Adding facts...';

    let added = 0;
    for (const fact of selectedTopic.examples) {
      try {
        await engram.tell({ statement: fact });
        added++;
      } catch (_) {}
    }

    statusEl.innerHTML = `<span style="color:var(--success)"><i class="fa-solid fa-check"></i> Added ${added} facts</span>`;
    showToast(`Added ${added} starter facts for ${selectedTopic.label}`, 'success');
    btn.disabled = false;

    // Refresh stats
    setTimeout(() => loadHomeData(), 500);
  });

  // Back button
  container.querySelector('#topic-back')?.addEventListener('click', () => {
    const previewDiv = document.getElementById('topic-preview');
    if (previewDiv) previewDiv.style.display = 'none';
    container.querySelectorAll('.wizard-topic-btn').forEach(b => {
      b.style.borderColor = 'var(--border)';
      b.style.background = 'var(--bg-secondary)';
    });
    selectedTopic = null;
  });
}
