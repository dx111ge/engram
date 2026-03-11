/* ============================================
   engram - App Core: Router, API Client, Utilities
   ============================================ */

// --- Auth State ---
const auth = {
  get token()    { return sessionStorage.getItem('engram_token'); },
  set token(v)   { if (v) sessionStorage.setItem('engram_token', v); else sessionStorage.removeItem('engram_token'); },
  get user()     { const u = sessionStorage.getItem('engram_user'); return u ? JSON.parse(u) : null; },
  set user(v)    { if (v) sessionStorage.setItem('engram_user', JSON.stringify(v)); else sessionStorage.removeItem('engram_user'); },
  get loggedIn() { return !!this.token; },

  logout() {
    const tok = this.token;
    this.token = null;
    this.user = null;
    if (tok) engram._fetch('/auth/logout', { method: 'POST' }).catch(() => {});
    showAuthScreen();
  },
};

// --- API Client ---
const engram = {
  get apiBase() {
    return localStorage.getItem('engram_api') || 'http://localhost:3030';
  },

  async _fetch(path, options = {}) {
    const url = `${this.apiBase}${path}`;
    const headers = { 'Content-Type': 'application/json' };
    if (auth.token) headers['Authorization'] = `Bearer ${auth.token}`;
    const config = { headers, ...options };
    // Merge headers (don't let spread overwrite auth)
    if (options.headers) {
      config.headers = { ...headers, ...options.headers };
    }
    try {
      const resp = await fetch(url, config);
      if (resp.status === 401 && auth.loggedIn) {
        auth.token = null;
        auth.user = null;
        showAuthScreen();
        throw new Error('Session expired');
      }
      if (!resp.ok) {
        let msg = `HTTP ${resp.status}`;
        try { const body = await resp.text(); if (body) msg += `: ${body}`; } catch (_) {}
        throw new Error(msg);
      }
      const text = await resp.text();
      return text ? JSON.parse(text) : {};
    } catch (err) {
      if (err.message.startsWith('HTTP ') || err.message === 'Session expired') throw err;
      throw new Error(`Connection failed: ${err.message}`);
    }
  },

  _post(path, body) {
    return this._fetch(path, { method: 'POST', body: JSON.stringify(body) });
  },

  // Auth endpoints
  authStatus()         { return this._fetch('/auth/status'); },
  authSetup(data)      { return this._post('/auth/setup', data); },
  authLogin(data)      { return this._post('/auth/login', data); },
  authLogout()         { return this._post('/auth/logout', {}); },
  listUsers()          { return this._fetch('/auth/users'); },
  createUser(data)     { return this._post('/auth/users', data); },
  updateUser(username, data) {
    return this._fetch(`/auth/users/${encodeURIComponent(username)}`, { method: 'PUT', body: JSON.stringify(data) });
  },
  deleteUser(username) { return this._fetch(`/auth/users/${encodeURIComponent(username)}`, { method: 'DELETE' }); },
  changePassword(data) { return this._post('/auth/change-password', data); },

  // Core endpoints
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

  // Ingest
  ingest(data)           { return this._post('/ingest', data); },
  ingestAnalyze(text)    { return this._post('/ingest/analyze', { text }); },
  ingestFile(data)       { return this._post('/ingest/file', data); },
  ingestConfigure(data)  { return this._post('/ingest/configure', data); },

  // Sources
  listSources()          { return this._fetch('/sources'); },
  sourceUsage(name)      { return this._fetch(`/sources/${encodeURIComponent(name)}/usage`); },

  // Action rules
  loadActionRules(data)  { return this._post('/actions/rules', data); },
  listActionRules()      { return this._fetch('/actions/rules'); },
  dryRunAction(data)     { return this._post('/actions/dry-run', data); },

  // Reasoning
  reasonGaps()           { return this._fetch('/reason/gaps'); },
  reasonScan()           { return this._post('/reason/scan', {}); },
  reasonFrontier()       { return this._fetch('/reason/frontier'); },
  reasonSuggest(data)    { return this._post('/reason/suggest', data); },

  // Mesh
  meshProfiles()         { return this._fetch('/mesh/profiles'); },
  meshDiscover(topic)    { return this._fetch(`/mesh/discover?topic=${encodeURIComponent(topic)}`); },
  meshFederatedQuery(data) { return this._post('/mesh/query', data); },
  meshPeers()            { return this._fetch('/mesh/peers'); },
  meshIdentity()         { return this._fetch('/mesh/identity'); },
  meshAudit()            { return this._fetch('/mesh/audit'); },

  // Config
  getConfig()            { return this._fetch('/config'); },
  setConfig(data)        { return this._post('/config', data); },

  // Assessments
  listAssessments(params) {
    let url = '/assessments?';
    if (params?.category) url += `category=${encodeURIComponent(params.category)}&`;
    if (params?.status) url += `status=${encodeURIComponent(params.status)}&`;
    return this._fetch(url);
  },
  getAssessment(label)     { return this._fetch(`/assessments/${encodeURIComponent(label)}`); },
  createAssessment(data)   { return this._post('/assessments', data); },
  updateAssessment(label, data) {
    return this._fetch(`/assessments/${encodeURIComponent(label)}`, { method: 'PATCH', body: JSON.stringify(data), headers: { 'Content-Type': 'application/json' } });
  },
  deleteAssessment(label)  { return this._fetch(`/assessments/${encodeURIComponent(label)}`, { method: 'DELETE' }); },
  evaluateAssessment(label){ return this._post(`/assessments/${encodeURIComponent(label)}/evaluate`, {}); },
  addEvidence(label, data) { return this._post(`/assessments/${encodeURIComponent(label)}/evidence`, data); },
  removeEvidence(label, id){ return this._fetch(`/assessments/${encodeURIComponent(label)}/evidence/${id}`, { method: 'DELETE' }); },
  assessHistory(label)     { return this._fetch(`/assessments/${encodeURIComponent(label)}/history`); },
  addWatch(label, data)    { return this._post(`/assessments/${encodeURIComponent(label)}/watch`, data); },
  removeWatch(label, entity){ return this._fetch(`/assessments/${encodeURIComponent(label)}/watch/${encodeURIComponent(entity)}`, { method: 'DELETE' }); },

  // Secrets (keys only, never values)
  listSecrets()          { return this._fetch('/secrets'); },
  setSecret(key, value)  { return this._post(`/secrets/${encodeURIComponent(key)}`, { value }); },
  deleteSecret(key)      { return this._fetch(`/secrets/${encodeURIComponent(key)}`, { method: 'DELETE' }); },
  checkSecret(key)       { return this._fetch(`/secrets/${encodeURIComponent(key)}/check`); },
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
  const label = strengthLabel(confidence);
  return `
    ${showLabel ? `<div class="confidence-label"><span>${label}</span><span>${pct}%</span></div>` : ''}
    <div class="confidence-bar">
      <div class="confidence-bar-fill" style="width:${pct}%;background:${color}"></div>
    </div>`;
}

function tierBadge(confidence) {
  if (confidence >= 0.8) return '<span class="badge badge-core"><i class="fa-solid fa-star"></i> Core</span>';
  if (confidence >= 0.4) return '<span class="badge badge-active"><i class="fa-solid fa-bolt"></i> Active</span>';
  return '<span class="badge badge-archival"><i class="fa-solid fa-box-archive"></i> Archival</span>';
}

function strengthLabel(confidence) {
  if (confidence >= 0.7) return 'Strong';
  if (confidence >= 0.4) return 'Moderate';
  return 'Weak';
}

function strengthBadge(confidence) {
  const label = strengthLabel(confidence);
  const color = confidenceColor(confidence);
  let icon;
  if (confidence >= 0.7) {
    icon = 'fa-shield-check';
  } else if (confidence >= 0.4) {
    icon = 'fa-shield-halved';
  } else {
    icon = 'fa-shield';
  }
  return `<span class="badge" style="background:${color};color:#fff"><i class="fa-solid ${icon}"></i> ${label}</span>`;
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

    // Default: home
    if (this.routes['/']) {
      this.routes['/']();
    }
  }
};


// --- Auth UI ---
function showAuthScreen() {
  document.getElementById('main-nav').style.display = 'none';
  document.getElementById('chat-toggle').style.display = 'none';
  document.getElementById('chat-panel').classList.remove('open');
  document.getElementById('chat-panel').classList.add('closed');
  // Auth status determines if we show setup or login
  engram._fetch('/auth/status').then(status => {
    if (status.status === 'setup_required') {
      renderAuthSetup();
    } else {
      renderAuthLogin();
    }
  }).catch(() => {
    renderTo(`<div class="auth-container">
      <div class="auth-card">
        <div class="auth-header"><i class="fa-solid fa-brain"></i><h1>engram</h1></div>
        <div class="auth-error"><i class="fa-solid fa-circle-exclamation"></i> Cannot connect to server</div>
        <button class="btn btn-primary btn-block" onclick="showAuthScreen()"><i class="fa-solid fa-rotate"></i> Retry</button>
      </div>
    </div>`);
  });
}

function renderAuthLogin() {
  renderTo(`<div class="auth-container">
    <div class="auth-card">
      <div class="auth-header"><i class="fa-solid fa-brain"></i><h1>engram</h1></div>
      <p class="auth-subtitle">Sign in to your knowledge base</p>
      <form id="login-form" autocomplete="on">
        <div class="form-group">
          <label><i class="fa-solid fa-user"></i> Username</label>
          <input type="text" id="login-user" autocomplete="username" required>
        </div>
        <div class="form-group">
          <label><i class="fa-solid fa-lock"></i> Password</label>
          <input type="password" id="login-pass" autocomplete="current-password" required>
        </div>
        <div id="login-error" class="auth-error" style="display:none"></div>
        <button type="submit" class="btn btn-primary btn-block"><i class="fa-solid fa-right-to-bracket"></i> Sign In</button>
      </form>
    </div>
  </div>`);
  document.getElementById('login-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const errEl = document.getElementById('login-error');
    errEl.style.display = 'none';
    try {
      const res = await engram.authLogin({
        username: document.getElementById('login-user').value,
        password: document.getElementById('login-pass').value,
      });
      auth.token = res.token;
      auth.user = { username: res.username, role: res.role, trust_level: res.trust_level };
      enterApp();
    } catch (err) {
      errEl.textContent = err.message.replace(/^HTTP \d+:\s*/, '').replace(/^\{"error":"(.*)"\}$/, '$1');
      errEl.style.display = 'block';
    }
  });
  document.getElementById('login-user').focus();
}

function renderAuthSetup() {
  renderTo(`<div class="auth-container">
    <div class="auth-card">
      <div class="auth-header"><i class="fa-solid fa-brain"></i><h1>engram</h1></div>
      <p class="auth-subtitle">Create the admin account</p>
      <form id="setup-form" autocomplete="on">
        <div class="form-group">
          <label><i class="fa-solid fa-user-shield"></i> Admin Username</label>
          <input type="text" id="setup-user" autocomplete="username" value="admin" required>
        </div>
        <div class="form-group">
          <label><i class="fa-solid fa-lock"></i> Password</label>
          <input type="password" id="setup-pass" autocomplete="new-password" required minlength="8">
        </div>
        <div class="form-group">
          <label><i class="fa-solid fa-lock"></i> Confirm Password</label>
          <input type="password" id="setup-pass2" autocomplete="new-password" required minlength="8">
        </div>
        <div id="setup-error" class="auth-error" style="display:none"></div>
        <button type="submit" class="btn btn-primary btn-block"><i class="fa-solid fa-shield-check"></i> Create Admin Account</button>
      </form>
    </div>
  </div>`);
  document.getElementById('setup-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const errEl = document.getElementById('setup-error');
    errEl.style.display = 'none';
    const pass = document.getElementById('setup-pass').value;
    const pass2 = document.getElementById('setup-pass2').value;
    if (pass !== pass2) {
      errEl.textContent = 'Passwords do not match';
      errEl.style.display = 'block';
      return;
    }
    try {
      const res = await engram.authSetup({
        username: document.getElementById('setup-user').value,
        password: pass,
      });
      auth.token = res.token;
      auth.user = { username: res.username, role: res.role, trust_level: res.trust_level };
      showToast('Admin account created', 'success');
      enterApp();
    } catch (err) {
      errEl.textContent = err.message.replace(/^HTTP \d+:\s*/, '').replace(/^\{"error":"(.*)"\}$/, '$1');
      errEl.style.display = 'block';
    }
  });
  document.getElementById('setup-user').focus();
}

function enterApp() {
  document.getElementById('main-nav').style.display = '';
  document.getElementById('chat-toggle').style.display = '';
  updateUserNav();
  router.resolve();
  checkHealth();
}

function updateUserNav() {
  const u = auth.user;
  let el = document.getElementById('user-nav-item');
  if (!el) {
    const nav = document.querySelector('.nav-status');
    el = document.createElement('div');
    el.id = 'user-nav-item';
    el.className = 'nav-user';
    nav.insertBefore(el, nav.firstChild);
  }
  if (u) {
    const roleIcon = u.role === 'admin' ? 'fa-user-shield' : u.role === 'analyst' ? 'fa-user-pen' : 'fa-user';
    el.innerHTML = `<span class="nav-user-badge" title="${escapeHtml(u.username)} (${u.role})"><i class="fa-solid ${roleIcon}"></i> ${escapeHtml(u.username)}</span>
      <button class="btn-icon" id="logout-btn" title="Sign Out"><i class="fa-solid fa-right-from-bracket"></i></button>`;
    document.getElementById('logout-btn').addEventListener('click', () => auth.logout());
  } else {
    el.innerHTML = '';
  }
}

// --- App Initialization ---
document.addEventListener('DOMContentLoaded', () => {
  // Nav toggle for smaller desktop windows
  document.getElementById('nav-toggle').addEventListener('click', () => {
    document.getElementById('nav-links').classList.toggle('open');
  });

  // Close nav on link click
  document.querySelectorAll('.nav-links a').forEach(a => {
    a.addEventListener('click', () => {
      document.getElementById('nav-links').classList.remove('open');
    });
  });

  // Gear button navigates to system page
  document.getElementById('settings-btn').addEventListener('click', () => {
    location.hash = '#/system';
  });

  // Chat panel toggle
  document.getElementById('chat-toggle').addEventListener('click', () => {
    const panel = document.getElementById('chat-panel');
    panel.classList.toggle('open');
    panel.classList.toggle('closed');
    if (panel.classList.contains('open')) {
      document.getElementById('chat-input').focus();
      initChatIfNeeded();
    }
  });

  document.getElementById('chat-close-btn').addEventListener('click', () => {
    document.getElementById('chat-panel').classList.remove('open');
    document.getElementById('chat-panel').classList.add('closed');
  });

  // Check auth state before entering app
  engram._fetch('/auth/status').then(status => {
    if (status.status === 'setup_required') {
      showAuthScreen();
    } else if (auth.loggedIn) {
      enterApp();
    } else {
      showAuthScreen();
    }
  }).catch(() => {
    // Server unreachable — enter app anyway, health check will show offline
    enterApp();
  });

  // Health check loop
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
