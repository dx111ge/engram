/* ============================================
   engram - Home View
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
        <div class="stat-value" id="stat-nodes">--</div>
        <div class="stat-label">Facts stored</div>
      </div>
      <div class="card stat-card">
        <div class="stat-icon"><i class="fa-solid fa-arrow-right-arrow-left"></i></div>
        <div class="stat-value" id="stat-edges">--</div>
        <div class="stat-label">Connections</div>
      </div>
      <div class="card stat-card">
        <div class="stat-icon"><i class="fa-solid fa-heart-pulse"></i></div>
        <div class="stat-value" id="stat-health">--</div>
        <div class="stat-label">Status</div>
      </div>
    </div>

    <div id="home-system-area"></div>

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
    const [stats, health, compute] = await Promise.all([
      engram.stats().catch(() => null),
      engram.health().catch(() => null),
      engram.compute().catch(() => null),
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
      loadSystemSummary();

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

// --- System Summary ---
async function loadSystemSummary() {
  const container = document.getElementById('home-system-area');
  if (!container) return;

  let compute = null;
  let sourcesEnabled = false, actionsEnabled = false, reasonEnabled = false, meshEnabled = false;

  // Fetch with timeout helper for reason/gaps
  function fetchWithTimeout(path, timeoutMs) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    return engram._fetch(path, { signal: controller.signal })
      .finally(() => clearTimeout(timer));
  }

  await Promise.allSettled([
    engram.compute().then(r => { compute = r; }).catch(() => {}),
    engram._fetch('/sources').then(() => { sourcesEnabled = true; }).catch(() => {}),
    engram._fetch('/actions/rules').then(() => { actionsEnabled = true; }).catch(() => {}),
    fetchWithTimeout('/reason/gaps', 2000).then(() => { reasonEnabled = true; }).catch(() => {}),
    engram._fetch('/mesh/identity').then(() => { meshEnabled = true; }).catch(() => {}),
  ]);

  // CPU info
  let cpuText = 'Unknown';
  if (compute && compute.cpu_cores) {
    cpuText = compute.cpu_cores + ' cores';
    if (compute.has_avx2) cpuText += ', AVX2';
    else if (compute.has_neon) cpuText += ', NEON';
  }

  // GPU info
  let gpuText = 'none';
  if (compute && compute.has_gpu && compute.gpu_name) {
    gpuText = escapeHtml(compute.gpu_name);
    if (compute.gpu_backend) gpuText += ' (' + escapeHtml(compute.gpu_backend) + ')';
  }

  // NPU info
  let npuText = null;
  if (compute && compute.has_npu) {
    npuText = compute.npu_name ? escapeHtml(compute.npu_name) : 'available';
    if (compute.npu_backend) npuText += ' (' + escapeHtml(compute.npu_backend) + ')';
  }

  // Embedder info
  let embedText = 'not configured';
  if (compute && compute.embedder_model) {
    embedText = escapeHtml(compute.embedder_model);
    if (compute.embedder_dim) embedText += ' (' + compute.embedder_dim + 'D)';
  } else if (compute && compute.embedder_endpoint) {
    embedText = escapeHtml(compute.embedder_endpoint);
  }

  // Feature badge helper
  function badge(label, enabled) {
    if (enabled) {
      return '<span style="display:inline-flex;align-items:center;gap:0.3rem;padding:0.15rem 0.5rem;border-radius:999px;font-size:0.78rem;background:rgba(46,160,67,0.15);color:var(--success);border:1px solid rgba(46,160,67,0.3)">'
        + '<i class="fa-solid fa-check" style="font-size:0.65rem"></i> ' + label + '</span>';
    }
    return '<span style="display:inline-flex;align-items:center;gap:0.3rem;padding:0.15rem 0.5rem;border-radius:999px;font-size:0.78rem;background:var(--bg-secondary);color:var(--text-muted);border:1px solid var(--border)">'
      + label + '</span>';
  }

  let npuRow = '';
  if (npuText) {
    npuRow = `
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-brain" style="color:var(--text-muted)"></i>
          <span style="color:var(--text-secondary)">NPU:</span>
          <span>${npuText}</span>
        </div>`;
  }

  container.innerHTML = `
    <div class="card mb-2">
      <div class="card-header">
        <h3><i class="fa-solid fa-microchip"></i> System</h3>
      </div>
      <div style="display:flex;flex-wrap:wrap;gap:1.5rem;font-size:0.85rem;padding:0.25rem 0">
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-server" style="color:var(--text-muted)"></i>
          <span style="color:var(--text-secondary)">CPU:</span>
          <span>${cpuText}</span>
        </div>
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-display" style="color:var(--text-muted)"></i>
          <span style="color:var(--text-secondary)">GPU:</span>
          <span>${gpuText}</span>
        </div>${npuRow}
        <div style="display:flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-vector-square" style="color:var(--text-muted)"></i>
          <span style="color:var(--text-secondary)">Embedder:</span>
          <span>${embedText}</span>
        </div>
      </div>
      <div style="display:flex;flex-wrap:wrap;gap:0.5rem;margin-top:0.75rem;padding-bottom:0.25rem">
        ${badge('Ingest', sourcesEnabled)}
        ${badge('Actions', actionsEnabled)}
        ${badge('Reasoning', reasonEnabled)}
        ${badge('Mesh', meshEnabled)}
      </div>
    </div>`;
}

// --- Step 1: Embedder Onboarding ---
function showEmbedderOnboarding(container) {
  const options = [
    {
      id: 'onnx',
      icon: 'fa-microchip',
      title: 'ONNX Local',
      quality: 'Good',
      performance: 'Excellent',
      cost: 'Free',
      license: 'MIT',
      description: 'Runs in-process, fastest option, no external dependencies.',
    },
    {
      id: 'ollama',
      icon: 'fa-server',
      title: 'Ollama Local',
      quality: 'Good',
      performance: 'Good',
      cost: 'Free',
      license: 'Apache/MIT',
      description: 'More model variety, runs as a separate local service.',
    },
    {
      id: 'openai',
      icon: 'fa-cloud',
      title: 'OpenAI API',
      quality: 'Excellent',
      performance: 'Network-dependent',
      cost: 'Paid per token',
      license: 'Commercial',
      description: 'Highest quality embeddings, requires API key.',
    },
  ];

  function ratingDot(level) {
    const colors = {
      'Excellent': 'var(--success)',
      'Good': 'var(--confidence-mid)',
      'Free': 'var(--success)',
      'Paid per token': 'var(--confidence-low)',
      'Network-dependent': 'var(--confidence-mid)',
      'MIT': 'var(--success)',
      'Apache/MIT': 'var(--success)',
      'Commercial': 'var(--confidence-mid)',
    };
    const color = colors[level] || 'var(--text-muted)';
    return `<span style="color:${color}">${escapeHtml(level)}</span>`;
  }

  container.innerHTML = `
    <div class="card mb-2">
      <div class="card-header">
        <h3><i class="fa-solid fa-wand-magic-sparkles"></i> Set Up Your Embedding Model</h3>
        <span style="font-size:0.78rem;padding:0.15rem 0.5rem;border-radius:999px;background:var(--accent-bright);color:#fff">Step 1 of 2</span>
      </div>
      <p style="font-size:0.92rem;color:var(--text-secondary);margin-bottom:0.5rem">
        This is the most important decision before adding data. Your embedding model determines how engram
        understands meaning. This cannot be easily changed later without re-processing all data.
      </p>
      <div style="display:flex;align-items:center;gap:0.4rem;padding:0.5rem 0.75rem;background:rgba(227,160,8,0.1);border:1px solid rgba(227,160,8,0.3);border-radius:var(--radius-sm);font-size:0.85rem;color:var(--confidence-mid);margin-bottom:1.25rem">
        <i class="fa-solid fa-triangle-exclamation"></i>
        <span>Choose carefully -- changing your embedding model after loading data requires a full reindex.</span>
      </div>
      <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(260px, 1fr));gap:1rem">
        ${options.map(opt => `
          <div style="border:2px solid var(--border);border-radius:var(--radius-sm);padding:1rem;background:var(--bg-secondary);display:flex;flex-direction:column;gap:0.6rem">
            <div style="display:flex;align-items:center;gap:0.5rem;font-size:1.05rem;font-weight:600">
              <i class="fa-solid ${opt.icon}" style="color:var(--accent-bright);font-size:1.2rem"></i>
              ${escapeHtml(opt.title)}
            </div>
            <p style="font-size:0.85rem;color:var(--text-secondary);margin:0">${escapeHtml(opt.description)}</p>
            <div style="font-size:0.8rem;display:flex;flex-direction:column;gap:0.25rem;margin-top:auto">
              <div><span style="color:var(--text-muted)">Quality:</span> ${ratingDot(opt.quality)}</div>
              <div><span style="color:var(--text-muted)">Performance:</span> ${ratingDot(opt.performance)}</div>
              <div><span style="color:var(--text-muted)">Cost:</span> ${ratingDot(opt.cost)}</div>
              <div><span style="color:var(--text-muted)">License:</span> ${ratingDot(opt.license)}</div>
            </div>
            <a href="#/settings" class="btn btn-sm btn-primary" style="margin-top:0.5rem;text-align:center">
              <i class="fa-solid fa-gear"></i> Configure
            </a>
          </div>
        `).join('')}
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
        <h3><i class="fa-solid fa-tags"></i> Your Topics</h3>
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

// --- Step 3: Compact topics (DB has data, embedder configured) ---
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

  // Add topic toggle + submit handlers (no preview needed in compact mode)
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
