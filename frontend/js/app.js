/* ============================================
   engram - App Core: Router, API Client, Utilities
   ============================================ */

// --- API Client ---
const engram = {
  get apiBase() {
    return localStorage.getItem('engram_api') || 'http://localhost:3030';
  },

  async _fetch(path, options = {}) {
    const url = `${this.apiBase}${path}`;
    const defaults = {
      headers: { 'Content-Type': 'application/json' },
    };
    const config = { ...defaults, ...options };
    try {
      const resp = await fetch(url, config);
      if (!resp.ok) {
        let msg = `HTTP ${resp.status}`;
        try { const body = await resp.text(); if (body) msg += `: ${body}`; } catch (_) {}
        throw new Error(msg);
      }
      const text = await resp.text();
      return text ? JSON.parse(text) : {};
    } catch (err) {
      if (err.message.startsWith('HTTP ')) throw err;
      throw new Error(`Connection failed: ${err.message}`);
    }
  },

  _post(path, body) {
    return this._fetch(path, { method: 'POST', body: JSON.stringify(body) });
  },

  // Endpoints
  health()          { return this._fetch('/health'); },
  stats()           { return this._fetch('/stats'); },
  compute()         { return this._fetch('/compute'); },
  getNode(label)    { return this._fetch(`/node/${encodeURIComponent(label)}`); },
  deleteNode(label) { return this._fetch(`/node/${encodeURIComponent(label)}`, { method: 'DELETE' }); },
  explain(label)    { return this._fetch(`/explain/${encodeURIComponent(label)}`); },

  store(data)       { return this._post('/store', data); },
  relate(data)      { return this._post('/relate', data); },
  batch(data)       { return this._post('/batch', data); },
  query(data)       { return this._post('/query', data); },
  search(data)      { return this._post('/search', data); },
  similar(data)     { return this._post('/similar', data); },
  ask(data)         { return this._post('/ask', data); },
  tell(data)        { return this._post('/tell', data); },

  reinforce(data)   { return this._post('/learn/reinforce', data); },
  correct(data)     { return this._post('/learn/correct', data); },
  decay()           { return this._post('/learn/decay', {}); },
  derive(data)      { return this._post('/learn/derive', data); },

  // JSON-LD
  exportJsonLd()    { return this._fetch('/export/jsonld'); },
  importJsonLd(data){ return this._post('/import/jsonld', data); },

  // Rules
  loadRules(data)   { return this._post('/rules', data); },
  listRules()       { return this._fetch('/rules'); },
  clearRules()      { return this._fetch('/rules', { method: 'DELETE' }); },
};


// --- Toast Notifications ---
function showToast(message, type = 'info') {
  const container = document.getElementById('toast-container');
  const iconMap = {
    success: 'fa-circle-check',
    error: 'fa-circle-exclamation',
    info: 'fa-circle-info',
  };
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  toast.innerHTML = `<i class="fa-solid ${iconMap[type] || iconMap.info}"></i><span>${escapeHtml(message)}</span>`;
  container.appendChild(toast);
  setTimeout(() => {
    toast.classList.add('removing');
    setTimeout(() => toast.remove(), 300);
  }, 4000);
}


// --- Utility Helpers ---
function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function confidenceColor(c) {
  if (c >= 0.7) return 'var(--confidence-high)';
  if (c >= 0.4) return 'var(--confidence-mid)';
  return 'var(--confidence-low)';
}

function confidenceBar(confidence, showLabel = true) {
  const pct = Math.round(confidence * 100);
  const color = confidenceColor(confidence);
  return `
    ${showLabel ? `<div class="confidence-label"><span>Confidence</span><span>${pct}%</span></div>` : ''}
    <div class="confidence-bar">
      <div class="confidence-bar-fill" style="width:${pct}%;background:${color}"></div>
    </div>`;
}

function tierBadge(confidence) {
  if (confidence >= 0.8) return '<span class="badge badge-core"><i class="fa-solid fa-star"></i> Core</span>';
  if (confidence >= 0.4) return '<span class="badge badge-active"><i class="fa-solid fa-bolt"></i> Active</span>';
  return '<span class="badge badge-archival"><i class="fa-solid fa-box-archive"></i> Archival</span>';
}

function loadingHTML(message = 'Loading...') {
  return `<div class="loading-center"><span class="spinner"></span> ${escapeHtml(message)}</div>`;
}

function emptyStateHTML(icon, text) {
  return `<div class="empty-state"><i class="fa-solid ${icon}"></i><p>${text}</p></div>`;
}


// --- Router ---
const router = {
  routes: {},
  currentView: null,

  register(hash, handler) {
    this.routes[hash] = handler;
  },

  start() {
    window.addEventListener('hashchange', () => this.resolve());
    this.resolve();
  },

  resolve() {
    const hash = location.hash || '#/';
    let route = hash.slice(1); // remove #

    // Update nav active state
    document.querySelectorAll('.nav-links a').forEach(a => {
      const r = a.getAttribute('data-route');
      if (r) {
        a.classList.toggle('active', route === r || (r !== '/' && route.startsWith(r)));
      }
    });

    // Check exact routes first
    if (this.routes[route]) {
      this.routes[route]();
      return;
    }

    // Check pattern routes (e.g., /node/:label)
    for (const [pattern, handler] of Object.entries(this.routes)) {
      if (pattern.includes(':')) {
        const regex = new RegExp('^' + pattern.replace(/:([^/]+)/g, '([^/]+)') + '$');
        const match = route.match(regex);
        if (match) {
          handler(...match.slice(1).map(decodeURIComponent));
          return;
        }
      }
    }

    // Default: dashboard
    if (this.routes['/']) {
      this.routes['/']();
    }
  }
};


// --- App Initialization ---
document.addEventListener('DOMContentLoaded', () => {
  // Mobile nav toggle
  document.getElementById('nav-toggle').addEventListener('click', () => {
    document.getElementById('nav-links').classList.toggle('open');
  });

  // Close mobile nav on link click
  document.querySelectorAll('.nav-links a').forEach(a => {
    a.addEventListener('click', () => {
      document.getElementById('nav-links').classList.remove('open');
    });
  });

  // Settings modal
  const settingsBtn = document.getElementById('settings-btn');
  const settingsModal = document.getElementById('settings-modal');
  const apiInput = document.getElementById('api-url-input');

  settingsBtn.addEventListener('click', () => {
    apiInput.value = engram.apiBase;
    settingsModal.classList.add('visible');
  });

  document.querySelectorAll('.modal-close').forEach(btn => {
    btn.addEventListener('click', () => {
      const modalId = btn.getAttribute('data-modal');
      if (modalId) document.getElementById(modalId).classList.remove('visible');
    });
  });

  settingsModal.addEventListener('click', (e) => {
    if (e.target === settingsModal) settingsModal.classList.remove('visible');
  });

  document.getElementById('save-api-url').addEventListener('click', () => {
    const val = apiInput.value.trim().replace(/\/+$/, '');
    if (val) {
      localStorage.setItem('engram_api', val);
      showToast('API URL updated', 'success');
    } else {
      localStorage.removeItem('engram_api');
      showToast('API URL reset to default', 'info');
    }
    settingsModal.classList.remove('visible');
    checkHealth();
  });

  // Health check loop
  checkHealth();
  setInterval(checkHealth, 30000);

  // Register routes (views register themselves)
  router.start();
});

async function checkHealth() {
  const dot = document.getElementById('health-indicator');
  try {
    await engram.health();
    dot.className = 'health-dot online';
    dot.title = 'API Connected';
  } catch (_) {
    dot.className = 'health-dot offline';
    dot.title = 'API Offline';
  }
}

function renderTo(html) {
  document.getElementById('app').innerHTML = html;
}
