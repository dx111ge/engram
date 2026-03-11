/* ============================================
   engram - Security View
   Users, API keys, password management
   ============================================ */

router.register('/security', async () => {
  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-shield-halved"></i> Security</h1>
        <p class="text-secondary" style="margin-top:0.25rem">Users, API keys, access control</p>
      </div>
    </div>
    <div id="security-content">${loadingHTML('Loading security settings...')}</div>
  `);
  await loadSecurityView();
});

async function loadSecurityView() {
  const container = document.getElementById('security-content');
  const isAdmin = auth.user && auth.user.role === 'admin';
  let users = [], apiKeys = [];

  const gathers = [
    engram.listUsers().then(r => {
      users = Array.isArray(r) ? r : (r && r.users ? r.users : []);
    }).catch(() => {}),
    engram._fetch('/auth/api-keys').then(r => {
      apiKeys = Array.isArray(r) ? r : [];
    }).catch(() => {}),
  ];
  await Promise.allSettled(gathers);

  let html = '';

  // ── 1. My Account ──
  html += secSection('account', 'fa-user-circle', 'My Account', true, buildAccountSection());

  // ── 2. API Keys ──
  html += secSection('apikeys', 'fa-key', 'API Keys', true, buildApiKeysSection(apiKeys));

  // ── 3. User Management (admin only) ──
  if (isAdmin) {
    html += secSection('users', 'fa-users-gear', 'User Management', true, buildUserMgmtSection(users));
  }

  // ── 4. Integration Guide ──
  html += secSection('integration', 'fa-plug-circle-bolt', 'Integration Guide', false, buildIntegrationGuide());

  container.innerHTML = html;
  bindSecurityEvents(isAdmin);
}

// ── Section wrapper ──

function secSection(id, icon, title, open, content) {
  return `
    <div class="card" style="margin-bottom:1rem">
      <div class="section-header" onclick="toggleSecSection('${id}')" style="cursor:pointer;display:flex;align-items:center;gap:0.75rem;padding:0.75rem 0">
        <i class="fa-solid ${icon}" style="color:var(--accent-bright);font-size:1.1rem"></i>
        <span style="font-weight:600;flex:1">${title}</span>
        <i class="fa-solid fa-chevron-${open ? 'up' : 'down'}" id="sec-chevron-${id}"></i>
      </div>
      <div id="sec-body-${id}" style="${open ? '' : 'display:none'}">
        ${content}
      </div>
    </div>`;
}

function toggleSecSection(id) {
  const body = document.getElementById('sec-body-' + id);
  const chevron = document.getElementById('sec-chevron-' + id);
  if (!body) return;
  const hidden = body.style.display === 'none';
  body.style.display = hidden ? '' : 'none';
  chevron.className = 'fa-solid fa-chevron-' + (hidden ? 'up' : 'down');
}

// ── Section Builders ──

function buildAccountSection() {
  const u = auth.user;
  const roleIcon = u.role === 'admin' ? 'fa-user-shield' : u.role === 'analyst' ? 'fa-user-pen' : 'fa-user';
  return `
    <div style="display:flex;align-items:center;gap:1rem;margin-bottom:1.5rem;padding:0.75rem;background:var(--bg-hover);border-radius:var(--radius-sm)">
      <i class="fa-solid ${roleIcon}" style="font-size:2rem;color:var(--accent-bright)"></i>
      <div>
        <div style="font-weight:600;font-size:1.1rem">${escapeHtml(u.username)}</div>
        <div style="color:var(--text-secondary);font-size:0.85rem">Role: <span class="role-badge ${u.role}"><i class="fa-solid ${roleIcon}"></i> ${escapeHtml(u.role.charAt(0).toUpperCase() + u.role.slice(1))}</span></div>
        ${typeof u.trust_level === 'number' ? `<div style="color:var(--text-secondary);font-size:0.85rem;margin-top:0.2rem">Trust Level: ${u.trust_level.toFixed(2)}</div>` : ''}
      </div>
    </div>
    <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.5rem"><i class="fa-solid fa-lock" style="margin-right:0.3rem"></i> Change Password</div>
    <div style="display:flex;gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
      <div class="form-group" style="margin-bottom:0;flex:1;min-width:140px">
        <label style="font-size:0.75rem">Current Password</label>
        <input type="password" id="sec-pw-old" autocomplete="current-password">
      </div>
      <div class="form-group" style="margin-bottom:0;flex:1;min-width:140px">
        <label style="font-size:0.75rem">New Password</label>
        <input type="password" id="sec-pw-new" autocomplete="new-password">
      </div>
      <button class="btn btn-secondary" id="sec-pw-change"><i class="fa-solid fa-key"></i> Change</button>
    </div>
    <div id="sec-pw-result" class="mt-1"></div>`;
}

function buildApiKeysSection(apiKeys) {
  let html = `
    <p style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.75rem">
      API keys provide persistent access for integrations (HTTP API, MCP server, scripts).
      Keys inherit your role and trust level.
    </p>`;

  if (apiKeys.length > 0) {
    html += `<table class="users-table" style="margin-bottom:1rem">
      <thead><tr><th>Label</th><th>Key ID</th><th>Created</th><th></th></tr></thead>
      <tbody>`;
    for (const k of apiKeys) {
      const date = k.created_at ? new Date(k.created_at * 1000).toLocaleDateString() : '-';
      html += `<tr>
        <td><i class="fa-solid fa-key" style="margin-right:0.3rem;color:var(--text-muted)"></i>${escapeHtml(k.label)}</td>
        <td><code style="font-size:0.8rem;color:var(--text-muted)">${escapeHtml(k.id)}...</code></td>
        <td style="font-size:0.85rem;color:var(--text-secondary)">${date}</td>
        <td style="text-align:right">
          <button class="btn-icon sec-key-revoke" data-id="${escapeHtml(k.id)}" title="Revoke key"><i class="fa-solid fa-trash"></i></button>
        </td>
      </tr>`;
    }
    html += `</tbody></table>`;
  } else {
    html += `<div style="color:var(--text-muted);font-size:0.85rem;margin-bottom:1rem"><i class="fa-solid fa-info-circle"></i> No API keys yet</div>`;
  }

  html += `
    <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.5rem"><i class="fa-solid fa-plus" style="margin-right:0.3rem"></i> Generate New Key</div>
    <div style="display:flex;gap:0.5rem;align-items:flex-end">
      <div class="form-group" style="margin-bottom:0;flex:1;min-width:200px">
        <label style="font-size:0.75rem">Label (e.g. "MCP Server", "CI Pipeline")</label>
        <input type="text" id="sec-key-label" placeholder="My integration">
      </div>
      <button class="btn btn-primary" id="sec-key-generate"><i class="fa-solid fa-wand-magic-sparkles"></i> Generate</button>
    </div>
    <div id="sec-key-result" class="mt-1"></div>`;

  return html;
}

function buildUserMgmtSection(users) {
  const roleOptions = ['admin', 'analyst', 'reader'].map(r =>
    `<option value="${r}">${r.charAt(0).toUpperCase() + r.slice(1)}</option>`
  ).join('');

  let html = `<table class="users-table" style="margin-bottom:1rem">
    <thead><tr><th>User</th><th>Role</th><th>Trust</th><th>Status</th><th>API Keys</th><th></th></tr></thead>
    <tbody>`;
  for (const u of users) {
    const roleClass = u.role || 'reader';
    const roleIcon = roleClass === 'admin' ? 'fa-user-shield' : roleClass === 'analyst' ? 'fa-user-pen' : 'fa-user';
    const keyCount = u.api_key_count || 0;
    html += `<tr>
      <td><i class="fa-solid ${roleIcon}" style="margin-right:0.3rem;color:var(--text-muted)"></i>${escapeHtml(u.username)}</td>
      <td><span class="role-badge ${roleClass}"><i class="fa-solid ${roleIcon}"></i> ${escapeHtml(roleClass.charAt(0).toUpperCase() + roleClass.slice(1))}</span></td>
      <td>${typeof u.trust_level === 'number' ? u.trust_level.toFixed(2) : '-'}</td>
      <td>${u.enabled !== false ? '<span style="color:var(--success)"><i class="fa-solid fa-circle-check"></i></span>' : '<span style="color:var(--text-muted)"><i class="fa-solid fa-circle-minus"></i></span>'}</td>
      <td style="text-align:center">${keyCount}</td>
      <td style="text-align:right">
        ${u.username !== auth.user?.username ? `<button class="btn-icon sec-user-del" data-user="${escapeHtml(u.username)}" title="Delete user"><i class="fa-solid fa-trash"></i></button>` : ''}
      </td>
    </tr>`;
  }
  html += `</tbody></table>`;

  // Add user form
  html += `
    <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.5rem"><i class="fa-solid fa-user-plus" style="margin-right:0.3rem"></i> Add User</div>
    <div style="display:flex;gap:0.5rem;flex-wrap:wrap;align-items:flex-end">
      <div class="form-group" style="margin-bottom:0;flex:1;min-width:120px">
        <label style="font-size:0.75rem">Username</label>
        <input type="text" id="sec-user-name">
      </div>
      <div class="form-group" style="margin-bottom:0;flex:1;min-width:120px">
        <label style="font-size:0.75rem">Password</label>
        <input type="password" id="sec-user-pass">
      </div>
      <div class="form-group" style="margin-bottom:0;min-width:100px">
        <label style="font-size:0.75rem">Role</label>
        <select id="sec-user-role">${roleOptions}</select>
      </div>
      <div class="form-group" style="margin-bottom:0;min-width:70px">
        <label style="font-size:0.75rem">Trust</label>
        <input type="number" id="sec-user-trust" value="0.50" min="0" max="1" step="0.05" style="width:70px">
      </div>
      <button class="btn btn-primary" id="sec-user-add"><i class="fa-solid fa-plus"></i> Add</button>
    </div>
    <div id="sec-user-result" class="mt-1"></div>`;

  return html;
}

function buildIntegrationGuide() {
  const apiBase = engram.apiBase;
  return `
    <p style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:1rem">
      Use API keys to authenticate programmatic access. All API endpoints accept keys via the
      <code>Authorization: Bearer egk_...</code> header or the <code>X-Api-Key: egk_...</code> header.
    </p>

    <div style="margin-bottom:1rem">
      <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.4rem"><i class="fa-solid fa-terminal" style="margin-right:0.3rem"></i> HTTP API (curl)</div>
      <pre style="background:var(--bg-input);padding:0.75rem;border-radius:var(--radius-sm);font-size:0.8rem;overflow-x:auto;color:var(--text-secondary)">curl ${escapeHtml(apiBase)}/health \\
  -H "Authorization: Bearer egk_YOUR_KEY_HERE"

curl ${escapeHtml(apiBase)}/store \\
  -H "Authorization: Bearer egk_YOUR_KEY_HERE" \\
  -H "Content-Type: application/json" \\
  -d '{"label":"Example","content":"test fact","confidence":0.8}'</pre>
    </div>

    <div style="margin-bottom:1rem">
      <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.4rem"><i class="fa-solid fa-robot" style="margin-right:0.3rem"></i> MCP Server</div>
      <pre style="background:var(--bg-input);padding:0.75rem;border-radius:var(--radius-sm);font-size:0.8rem;overflow-x:auto;color:var(--text-secondary)">{
  "mcpServers": {
    "engram": {
      "command": "engram",
      "args": ["mcp"],
      "env": {
        "ENGRAM_API": "${escapeHtml(apiBase)}",
        "ENGRAM_API_KEY": "egk_YOUR_KEY_HERE"
      }
    }
  }
}</pre>
    </div>

    <div style="margin-bottom:1rem">
      <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.4rem"><i class="fa-brands fa-python" style="margin-right:0.3rem"></i> Python</div>
      <pre style="background:var(--bg-input);padding:0.75rem;border-radius:var(--radius-sm);font-size:0.8rem;overflow-x:auto;color:var(--text-secondary)">import requests

API = "${escapeHtml(apiBase)}"
KEY = "egk_YOUR_KEY_HERE"
headers = {"Authorization": f"Bearer {KEY}"}

r = requests.post(f"{API}/store", json={
    "label": "Example",
    "content": "test fact",
    "confidence": 0.8,
}, headers=headers)
print(r.json())</pre>
    </div>

    <div>
      <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.4rem"><i class="fa-solid fa-circle-info" style="margin-right:0.3rem"></i> Role Permissions</div>
      <table class="users-table" style="max-width:500px">
        <thead><tr><th>Permission</th><th>Admin</th><th>Analyst</th><th>Reader</th></tr></thead>
        <tbody>
          <tr><td>Read data (query, search, explore)</td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td></tr>
          <tr><td>Write data (store, relate, ingest)</td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-minus" style="color:var(--text-muted)"></i></td></tr>
          <tr><td>Delete data</td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-minus" style="color:var(--text-muted)"></i></td></tr>
          <tr><td>Configuration, secrets, reindex</td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-minus" style="color:var(--text-muted)"></i></td><td><i class="fa-solid fa-minus" style="color:var(--text-muted)"></i></td></tr>
          <tr><td>User management</td><td><i class="fa-solid fa-check" style="color:var(--success)"></i></td><td><i class="fa-solid fa-minus" style="color:var(--text-muted)"></i></td><td><i class="fa-solid fa-minus" style="color:var(--text-muted)"></i></td></tr>
        </tbody>
      </table>
    </div>`;
}

// ── Event Binding ──

function bindSecurityEvents(isAdmin) {
  // Password change
  const pwBtn = document.getElementById('sec-pw-change');
  if (pwBtn) {
    pwBtn.addEventListener('click', async () => {
      const oldPw = document.getElementById('sec-pw-old').value;
      const newPw = document.getElementById('sec-pw-new').value;
      const result = document.getElementById('sec-pw-result');
      if (!oldPw || !newPw) { result.innerHTML = '<span style="color:var(--error)">Both fields required</span>'; return; }
      if (newPw.length < 8) { result.innerHTML = '<span style="color:var(--error)">Password must be at least 8 characters</span>'; return; }
      try {
        await engram.changePassword({ old_password: oldPw, new_password: newPw });
        result.innerHTML = '<span style="color:var(--success)"><i class="fa-solid fa-circle-check"></i> Password changed</span>';
        document.getElementById('sec-pw-old').value = '';
        document.getElementById('sec-pw-new').value = '';
        showToast('Password changed', 'success');
      } catch (err) {
        result.innerHTML = `<span style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</span>`;
      }
    });
  }

  // Generate API key
  const genBtn = document.getElementById('sec-key-generate');
  if (genBtn) {
    genBtn.addEventListener('click', async () => {
      const label = document.getElementById('sec-key-label').value.trim();
      const result = document.getElementById('sec-key-result');
      if (!label) { result.innerHTML = '<span style="color:var(--error)">Label is required</span>'; return; }
      try {
        const res = await engram._post('/auth/api-keys', { label });
        result.innerHTML = `
          <div style="background:var(--bg-input);border:1px solid var(--success);border-radius:var(--radius-sm);padding:0.75rem;margin-top:0.5rem">
            <div style="font-weight:600;font-size:0.85rem;margin-bottom:0.4rem;color:var(--success)"><i class="fa-solid fa-circle-check"></i> API Key Generated</div>
            <div style="font-size:0.8rem;color:var(--text-secondary);margin-bottom:0.5rem"><i class="fa-solid fa-triangle-exclamation"></i> Copy this key now. It will not be shown again.</div>
            <div style="display:flex;gap:0.5rem;align-items:center">
              <code id="sec-key-value" style="flex:1;padding:0.4rem 0.6rem;background:var(--bg-primary);border-radius:4px;font-size:0.85rem;word-break:break-all">${escapeHtml(res.key)}</code>
              <button class="btn btn-secondary" onclick="navigator.clipboard.writeText(document.getElementById('sec-key-value').textContent);showToast('Key copied','success')"><i class="fa-solid fa-copy"></i></button>
            </div>
            <button class="btn btn-secondary" style="margin-top:0.75rem" onclick="loadSecurityView()"><i class="fa-solid fa-check"></i> Done</button>
          </div>`;
        document.getElementById('sec-key-label').value = '';
      } catch (err) {
        result.innerHTML = `<span style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</span>`;
      }
    });
  }

  // Revoke API key buttons
  document.querySelectorAll('.sec-key-revoke').forEach(btn => {
    btn.addEventListener('click', async () => {
      const keyId = btn.dataset.id;
      if (!confirm(`Revoke API key "${keyId}..."? This cannot be undone.`)) return;
      try {
        await engram._fetch(`/auth/api-keys/${encodeURIComponent(keyId)}`, { method: 'DELETE' });
        showToast('API key revoked', 'success');
        loadSecurityView();
      } catch (err) {
        showToast(`Failed: ${err.message}`, 'error');
      }
    });
  });

  // Add user (admin)
  const addBtn = document.getElementById('sec-user-add');
  if (addBtn) {
    addBtn.addEventListener('click', async () => {
      const name = document.getElementById('sec-user-name').value.trim();
      const pass = document.getElementById('sec-user-pass').value;
      const role = document.getElementById('sec-user-role').value;
      const trust = parseFloat(document.getElementById('sec-user-trust').value) || 0.5;
      const result = document.getElementById('sec-user-result');
      if (!name || !pass) { result.innerHTML = '<span style="color:var(--error)">Username and password required</span>'; return; }
      try {
        await engram.createUser({ username: name, password: pass, role, trust_level: trust });
        showToast(`User "${name}" created`, 'success');
        loadSecurityView();
      } catch (err) {
        result.innerHTML = `<span style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</span>`;
      }
    });
  }

  // Delete user buttons
  document.querySelectorAll('.sec-user-del').forEach(btn => {
    btn.addEventListener('click', async () => {
      const username = btn.dataset.user;
      if (!confirm(`Delete user "${username}"? This cannot be undone.`)) return;
      try {
        await engram.deleteUser(username);
        showToast(`User "${username}" deleted`, 'success');
        loadSecurityView();
      } catch (err) {
        showToast(`Failed: ${err.message}`, 'error');
      }
    });
  });
}
