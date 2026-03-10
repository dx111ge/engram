/* ============================================
   engram - Assessments View
   Hypothesis tracking and probability monitoring
   ============================================ */

let assessCurrentFilter = { category: '', status: 'active' };
let assessCurrentSort = 'last_evaluated';

router.register('/assess', async () => {
  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-scale-balanced"></i> Assessments</h1>
        <p class="text-secondary" style="margin-top:0.25rem">Track hypotheses and predictions with evidence-based probability</p>
      </div>
      <button class="btn btn-primary" id="assess-create-btn">
        <i class="fa-solid fa-plus"></i> New Assessment
      </button>
    </div>

    <!-- Filters -->
    <div class="toolbar" style="margin-bottom:1rem">
      <select id="assess-cat-filter" class="input-sm" style="min-width:140px">
        <option value="">All categories</option>
        <option value="financial">Financial</option>
        <option value="geopolitical">Geopolitical</option>
        <option value="technical">Technical</option>
        <option value="military">Military</option>
        <option value="social">Social</option>
        <option value="other">Other</option>
      </select>
      <select id="assess-status-filter" class="input-sm" style="min-width:120px">
        <option value="active" selected>Active</option>
        <option value="paused">Paused</option>
        <option value="archived">Archived</option>
        <option value="resolved">Resolved</option>
        <option value="">All</option>
      </select>
      <select id="assess-sort" class="input-sm" style="min-width:140px">
        <option value="last_evaluated">Last evaluated</option>
        <option value="probability">Probability</option>
        <option value="shift">Recent shift</option>
        <option value="title">Title</option>
      </select>
    </div>

    <div id="assess-list">${loadingHTML('Loading assessments...')}</div>

    <!-- Create Assessment Modal -->
    <div class="modal-overlay" id="assess-create-modal">
      <div class="modal" style="max-width:600px">
        <div class="modal-header">
          <h3><i class="fa-solid fa-scale-balanced"></i> New Assessment</h3>
          <button class="btn-icon" id="assess-modal-close"><i class="fa-solid fa-xmark"></i></button>
        </div>
        <div class="modal-body" id="assess-wizard-body">
          <!-- Wizard steps rendered here -->
        </div>
        <div class="modal-footer" id="assess-wizard-footer"></div>
      </div>
    </div>
  `);

  // Event listeners
  document.getElementById('assess-create-btn').addEventListener('click', openCreateWizard);
  document.getElementById('assess-modal-close').addEventListener('click', () => {
    document.getElementById('assess-create-modal').classList.remove('visible');
  });
  document.getElementById('assess-create-modal').addEventListener('click', (e) => {
    if (e.target === document.getElementById('assess-create-modal'))
      document.getElementById('assess-create-modal').classList.remove('visible');
  });

  document.getElementById('assess-cat-filter').addEventListener('change', (e) => {
    assessCurrentFilter.category = e.target.value;
    loadAssessList();
  });
  document.getElementById('assess-status-filter').addEventListener('change', (e) => {
    assessCurrentFilter.status = e.target.value;
    loadAssessList();
  });
  document.getElementById('assess-sort').addEventListener('change', (e) => {
    assessCurrentSort = e.target.value;
    loadAssessList();
  });

  await loadAssessList();
});

async function loadAssessList() {
  const container = document.getElementById('assess-list');
  if (!container) return;

  try {
    let url = '/assessments?';
    if (assessCurrentFilter.category) url += `category=${encodeURIComponent(assessCurrentFilter.category)}&`;
    if (assessCurrentFilter.status) url += `status=${encodeURIComponent(assessCurrentFilter.status)}&`;

    const data = await engram._fetch(url);
    const assessments = data.assessments || [];

    // Sort
    assessments.sort((a, b) => {
      switch (assessCurrentSort) {
        case 'probability': return b.current_probability - a.current_probability;
        case 'shift': return Math.abs(b.last_shift || 0) - Math.abs(a.last_shift || 0);
        case 'title': return (a.title || '').localeCompare(b.title || '');
        default: return (b.last_evaluated || 0) - (a.last_evaluated || 0);
      }
    });

    if (assessments.length === 0) {
      container.innerHTML = emptyStateHTML('fa-scale-balanced', 'No assessments yet. Create one to start tracking predictions.');
      return;
    }

    container.innerHTML = `<div class="assess-grid">${assessments.map(a => assessCard(a)).join('')}</div>`;

    // Click handlers for cards
    container.querySelectorAll('.assess-card').forEach(card => {
      card.addEventListener('click', () => {
        const label = card.dataset.label;
        location.hash = `#/assess/${encodeURIComponent(label)}`;
      });
    });
  } catch (err) {
    container.innerHTML = emptyStateHTML('fa-circle-exclamation', `Failed to load assessments: ${escapeHtml(err.message)}`);
  }
}

function assessCard(a) {
  const pct = Math.round((a.current_probability || 0.5) * 100);
  const cat = a.category || 'uncategorized';
  const status = a.status || 'active';
  const shift = a.last_shift || 0;
  const shiftPct = Math.round(Math.abs(shift) * 100);
  const shiftIcon = shift > 0.01 ? 'fa-arrow-up' : shift < -0.01 ? 'fa-arrow-down' : 'fa-minus';
  const shiftColor = shift > 0.01 ? 'var(--confidence-high)' : shift < -0.01 ? 'var(--confidence-low)' : 'var(--text-secondary)';

  return `
    <div class="assess-card card" data-label="${escapeHtml(a.label)}" style="cursor:pointer">
      <div style="display:flex;align-items:center;gap:1rem;margin-bottom:0.75rem">
        <div class="assess-gauge" style="position:relative;width:60px;height:60px">
          <canvas class="assess-mini-gauge" data-pct="${pct}" width="60" height="60"></canvas>
          <span style="position:absolute;inset:0;display:flex;align-items:center;justify-content:center;font-weight:700;font-size:0.85rem">${pct}%</span>
        </div>
        <div style="flex:1;min-width:0">
          <h4 style="margin:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">${escapeHtml(a.title || a.label)}</h4>
          <div style="display:flex;gap:0.5rem;margin-top:0.25rem;flex-wrap:wrap">
            <span class="badge badge-sm">${escapeHtml(cat)}</span>
            <span class="badge badge-sm ${status === 'active' ? 'badge-active' : ''}">${escapeHtml(status)}</span>
          </div>
        </div>
        <div style="text-align:right">
          <div style="color:${shiftColor};font-weight:600">
            <i class="fa-solid ${shiftIcon}"></i> ${shiftPct > 0 ? shiftPct + '%' : '--'}
          </div>
          <div class="text-secondary" style="font-size:0.75rem">${a.evidence_count || 0} evidence</div>
        </div>
      </div>
      <canvas class="assess-sparkline" data-label="${escapeHtml(a.label)}" width="280" height="30"></canvas>
    </div>`;
}

// Draw mini gauges after DOM update
function drawAssessGauges() {
  document.querySelectorAll('.assess-mini-gauge').forEach(canvas => {
    const pct = parseInt(canvas.dataset.pct) || 50;
    const ctx = canvas.getContext('2d');
    const cx = 30, cy = 30, r = 24;

    ctx.clearRect(0, 0, 60, 60);

    // Background arc
    ctx.beginPath();
    ctx.arc(cx, cy, r, Math.PI * 0.75, Math.PI * 2.25, false);
    ctx.lineWidth = 5;
    ctx.strokeStyle = 'var(--border)';
    ctx.stroke();

    // Value arc
    const angle = Math.PI * 0.75 + (pct / 100) * Math.PI * 1.5;
    ctx.beginPath();
    ctx.arc(cx, cy, r, Math.PI * 0.75, angle, false);
    ctx.lineWidth = 5;
    const color = pct >= 70 ? '#22c55e' : pct >= 40 ? '#f59e0b' : '#ef4444';
    ctx.strokeStyle = color;
    ctx.stroke();
  });
}

// --- Detail View ---

router.register('/assess/:label', async (label) => {
  renderTo(loadingHTML('Loading assessment...'));

  try {
    const data = await engram._fetch(`/assessments/${encodeURIComponent(label)}`);

    const pct = Math.round((data.current_probability || 0.5) * 100);
    const cat = data.category || 'uncategorized';
    const status = data.status || 'active';
    const lastEval = data.last_evaluated ? new Date(data.last_evaluated * 1000).toLocaleString() : 'Never';

    renderTo(`
      <div class="view-header">
        <div>
          <a href="#/assess" class="btn btn-ghost btn-sm" style="margin-bottom:0.5rem">
            <i class="fa-solid fa-arrow-left"></i> Back to Assessments
          </a>
          <h1>${escapeHtml(data.title || label)}</h1>
          <div style="display:flex;gap:0.5rem;margin-top:0.25rem">
            <span class="badge">${escapeHtml(cat)}</span>
            <span class="badge ${status === 'active' ? 'badge-active' : ''}">${escapeHtml(status)}</span>
            ${data.timeframe ? `<span class="badge"><i class="fa-solid fa-clock"></i> ${escapeHtml(data.timeframe)}</span>` : ''}
          </div>
        </div>
        <div style="display:flex;gap:0.5rem">
          <button class="btn btn-primary btn-sm" id="assess-eval-btn"><i class="fa-solid fa-rotate"></i> Evaluate Now</button>
          <button class="btn btn-secondary btn-sm" id="assess-evidence-btn"><i class="fa-solid fa-plus"></i> Add Evidence</button>
          <button class="btn btn-secondary btn-sm" id="assess-watch-btn"><i class="fa-solid fa-eye"></i> Add Watch</button>
        </div>
      </div>

      ${data.description ? `<p class="text-secondary" style="margin-bottom:1rem">${escapeHtml(data.description)}</p>` : ''}

      <!-- Probability display -->
      <div class="card" style="margin-bottom:1.5rem">
        <div style="display:flex;align-items:center;gap:2rem">
          <div style="text-align:center;min-width:120px">
            <div style="font-size:2.5rem;font-weight:700;color:${pct >= 70 ? 'var(--confidence-high)' : pct >= 40 ? 'var(--confidence-mid)' : 'var(--confidence-low)'}">${pct}%</div>
            <div class="text-secondary">Current probability</div>
          </div>
          <div style="flex:1">
            <canvas id="assess-chart" width="700" height="200"></canvas>
          </div>
        </div>
        <div class="text-secondary" style="margin-top:0.5rem;font-size:0.8rem">
          <i class="fa-solid fa-clock"></i> Last evaluated: ${escapeHtml(lastEval)}
          &nbsp;&bull;&nbsp; ${(data.history || []).length} score points
        </div>
      </div>

      <!-- Evidence panels -->
      <div class="grid-2" style="margin-bottom:1.5rem">
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-thumbs-up" style="color:var(--confidence-high)"></i> Supporting Evidence (${(data.evidence_for || []).length})</h3></div>
          <div id="assess-evidence-for">
            ${(data.evidence_for || []).map(e => evidenceItemHtml(e, 'for')).join('') || '<p class="text-secondary">No supporting evidence yet</p>'}
          </div>
        </div>
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-thumbs-down" style="color:var(--confidence-low)"></i> Contradicting Evidence (${(data.evidence_against || []).length})</h3></div>
          <div id="assess-evidence-against">
            ${(data.evidence_against || []).map(e => evidenceItemHtml(e, 'against')).join('') || '<p class="text-secondary">No contradicting evidence yet</p>'}
          </div>
        </div>
      </div>

      <!-- Watched entities -->
      <div class="card">
        <div class="card-header"><h3><i class="fa-solid fa-eye"></i> Watched Entities (${(data.watches || []).length})</h3></div>
        <div style="display:flex;flex-wrap:wrap;gap:0.5rem;padding:0.5rem 0">
          ${(data.watches || []).map(w => `
            <a href="#/node/${encodeURIComponent(w)}" class="badge badge-lg" style="text-decoration:none">
              <i class="fa-solid fa-link"></i> ${escapeHtml(w)}
            </a>
          `).join('') || '<p class="text-secondary">No watched entities</p>'}
        </div>
      </div>
    `);

    // Draw chart
    drawAssessChart(data.history || []);

    // Event handlers
    document.getElementById('assess-eval-btn').addEventListener('click', async () => {
      try {
        const result = await engram._post(`/assessments/${encodeURIComponent(label)}/evaluate`, {});
        showToast(`Re-evaluated: ${Math.round(result.new_probability * 100)}% (shift: ${result.shift >= 0 ? '+' : ''}${Math.round(result.shift * 100)}%)`, 'success');
        location.hash = `#/assess/${encodeURIComponent(label)}`;
      } catch (err) {
        showToast(`Evaluation failed: ${err.message}`, 'error');
      }
    });

    document.getElementById('assess-evidence-btn').addEventListener('click', () => {
      promptAddEvidence(label);
    });

    document.getElementById('assess-watch-btn').addEventListener('click', () => {
      promptAddWatch(label);
    });

  } catch (err) {
    renderTo(emptyStateHTML('fa-circle-exclamation', `Failed to load assessment: ${escapeHtml(err.message)}`));
  }
});

function evidenceItemHtml(e, direction) {
  return `
    <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
      <a href="#/node/${encodeURIComponent(e.node_label)}" style="flex:1;text-decoration:none;color:var(--text-primary)">
        ${escapeHtml(e.node_label)}
      </a>
      <span style="font-weight:600">${Math.round(e.confidence * 100)}%</span>
    </div>`;
}

function drawAssessChart(history) {
  const canvas = document.getElementById('assess-chart');
  if (!canvas || history.length === 0) return;

  const ctx = canvas.getContext('2d');
  const w = canvas.width, h = canvas.height;
  const pad = { top: 10, bottom: 25, left: 40, right: 10 };
  const cw = w - pad.left - pad.right;
  const ch = h - pad.top - pad.bottom;

  ctx.clearRect(0, 0, w, h);

  // Y-axis (0% - 100%)
  ctx.strokeStyle = '#444';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(pad.left, pad.top);
  ctx.lineTo(pad.left, h - pad.bottom);
  ctx.lineTo(w - pad.right, h - pad.bottom);
  ctx.stroke();

  // Y labels
  ctx.fillStyle = '#888';
  ctx.font = '10px monospace';
  ctx.textAlign = 'right';
  for (let p = 0; p <= 100; p += 25) {
    const y = pad.top + ch - (p / 100) * ch;
    ctx.fillText(p + '%', pad.left - 4, y + 3);
    ctx.beginPath();
    ctx.strokeStyle = '#333';
    ctx.moveTo(pad.left, y);
    ctx.lineTo(w - pad.right, y);
    ctx.stroke();
  }

  if (history.length < 2) {
    // Single point
    const pct = history[0].probability;
    const x = pad.left + cw / 2;
    const y = pad.top + ch - pct * ch;
    ctx.beginPath();
    ctx.arc(x, y, 4, 0, Math.PI * 2);
    ctx.fillStyle = '#3b82f6';
    ctx.fill();
    return;
  }

  // Plot line
  const minT = history[0].timestamp;
  const maxT = history[history.length - 1].timestamp;
  const timeRange = maxT - minT || 1;

  // Filled area
  ctx.beginPath();
  ctx.moveTo(pad.left + ((history[0].timestamp - minT) / timeRange) * cw, h - pad.bottom);
  for (const p of history) {
    const x = pad.left + ((p.timestamp - minT) / timeRange) * cw;
    const y = pad.top + ch - p.probability * ch;
    ctx.lineTo(x, y);
  }
  ctx.lineTo(pad.left + ((history[history.length - 1].timestamp - minT) / timeRange) * cw, h - pad.bottom);
  ctx.closePath();
  ctx.fillStyle = 'rgba(59, 130, 246, 0.1)';
  ctx.fill();

  // Line
  ctx.beginPath();
  for (let i = 0; i < history.length; i++) {
    const p = history[i];
    const x = pad.left + ((p.timestamp - minT) / timeRange) * cw;
    const y = pad.top + ch - p.probability * ch;
    if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
  }
  ctx.strokeStyle = '#3b82f6';
  ctx.lineWidth = 2;
  ctx.stroke();

  // Points + shift annotations
  for (const p of history) {
    const x = pad.left + ((p.timestamp - minT) / timeRange) * cw;
    const y = pad.top + ch - p.probability * ch;
    const color = p.shift > 0.02 ? '#22c55e' : p.shift < -0.02 ? '#ef4444' : '#3b82f6';
    ctx.beginPath();
    ctx.arc(x, y, 3, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
  }
}

// --- Create Wizard ---
let wizardStep = 1;
let wizardData = {};

function openCreateWizard() {
  wizardStep = 1;
  wizardData = { title: '', description: '', category: '', timeframe: '', watches: [], probability: 0.50 };
  renderWizardStep();
  document.getElementById('assess-create-modal').classList.add('visible');
}

function renderWizardStep() {
  const body = document.getElementById('assess-wizard-body');
  const footer = document.getElementById('assess-wizard-footer');

  if (wizardStep === 1) {
    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="active">1</span> <span>2</span> <span>3</span></div>
      <label>Title <span class="text-secondary">(the hypothesis)</span></label>
      <input type="text" id="wiz-title" class="input" placeholder="e.g. NVIDIA stock > $200 by Q3 2026" value="${escapeHtml(wizardData.title)}">
      <label style="margin-top:1rem">Description <span class="text-secondary">(optional)</span></label>
      <textarea id="wiz-desc" class="input" rows="2" placeholder="Additional context...">${escapeHtml(wizardData.description)}</textarea>
      <div class="grid-2" style="margin-top:1rem">
        <div>
          <label>Category</label>
          <select id="wiz-cat" class="input">
            <option value="">Select...</option>
            <option value="financial" ${wizardData.category === 'financial' ? 'selected' : ''}>Financial</option>
            <option value="geopolitical" ${wizardData.category === 'geopolitical' ? 'selected' : ''}>Geopolitical</option>
            <option value="technical" ${wizardData.category === 'technical' ? 'selected' : ''}>Technical</option>
            <option value="military" ${wizardData.category === 'military' ? 'selected' : ''}>Military</option>
            <option value="social" ${wizardData.category === 'social' ? 'selected' : ''}>Social</option>
            <option value="other" ${wizardData.category === 'other' ? 'selected' : ''}>Other</option>
          </select>
        </div>
        <div>
          <label>Timeframe</label>
          <input type="text" id="wiz-time" class="input" placeholder="e.g. Q3 2026" value="${escapeHtml(wizardData.timeframe)}">
        </div>
      </div>`;
    footer.innerHTML = `
      <button class="btn btn-secondary" id="wiz-cancel">Cancel</button>
      <button class="btn btn-primary" id="wiz-next1">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    footer.querySelector('#wiz-cancel').addEventListener('click', () => {
      document.getElementById('assess-create-modal').classList.remove('visible');
    });
    footer.querySelector('#wiz-next1').addEventListener('click', () => {
      wizardData.title = document.getElementById('wiz-title').value.trim();
      wizardData.description = document.getElementById('wiz-desc').value.trim();
      wizardData.category = document.getElementById('wiz-cat').value;
      wizardData.timeframe = document.getElementById('wiz-time').value.trim();
      if (!wizardData.title) { showToast('Title is required', 'error'); return; }
      wizardStep = 2;
      renderWizardStep();
    });

  } else if (wizardStep === 2) {
    body.innerHTML = `
      <div class="wizard-step-indicator"><span>1</span> <span class="active">2</span> <span>3</span></div>
      <label>Watch Entities</label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">Search and select entities to monitor. New facts about these will trigger re-evaluation.</p>
      <div style="display:flex;gap:0.5rem;margin-bottom:1rem">
        <input type="text" id="wiz-entity-search" class="input" placeholder="Search entities...">
        <button class="btn btn-secondary" id="wiz-entity-add"><i class="fa-solid fa-plus"></i></button>
      </div>
      <div id="wiz-entity-results"></div>
      <div id="wiz-selected-entities" style="margin-top:1rem;display:flex;flex-wrap:wrap;gap:0.5rem">
        ${wizardData.watches.map(w => `<span class="badge badge-lg">${escapeHtml(w)} <button class="btn-icon btn-xs wiz-remove-entity" data-entity="${escapeHtml(w)}"><i class="fa-solid fa-xmark"></i></button></span>`).join('')}
      </div>`;
    footer.innerHTML = `
      <button class="btn btn-secondary" id="wiz-back2"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="wiz-next2">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    footer.querySelector('#wiz-back2').addEventListener('click', () => { wizardStep = 1; renderWizardStep(); });
    footer.querySelector('#wiz-next2').addEventListener('click', () => { wizardStep = 3; renderWizardStep(); });

    const searchInput = document.getElementById('wiz-entity-search');
    const addBtn = document.getElementById('wiz-entity-add');

    async function searchEntities() {
      const q = searchInput.value.trim();
      if (!q) return;
      try {
        const results = await engram.search({ query: q, limit: 8 });
        const hits = results.results || [];
        document.getElementById('wiz-entity-results').innerHTML = hits.map(h => `
          <div class="wiz-entity-hit" data-label="${escapeHtml(h.label)}" style="padding:0.4rem 0.5rem;cursor:pointer;border-bottom:1px solid var(--border)">
            ${escapeHtml(h.label)} <span class="text-secondary">(${Math.round(h.confidence * 100)}%)</span>
          </div>`).join('');

        document.querySelectorAll('.wiz-entity-hit').forEach(el => {
          el.addEventListener('click', () => {
            const label = el.dataset.label;
            if (!wizardData.watches.includes(label)) {
              wizardData.watches.push(label);
              renderWizardStep();
            }
          });
        });
      } catch (_) {}
    }

    searchInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') searchEntities(); });
    addBtn.addEventListener('click', searchEntities);

    document.querySelectorAll('.wiz-remove-entity').forEach(btn => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        const entity = btn.dataset.entity;
        wizardData.watches = wizardData.watches.filter(w => w !== entity);
        renderWizardStep();
      });
    });

  } else if (wizardStep === 3) {
    const pct = Math.round(wizardData.probability * 100);
    body.innerHTML = `
      <div class="wizard-step-indicator"><span>1</span> <span>2</span> <span class="active">3</span></div>
      <label>Initial Probability</label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">Your starting estimate. This will adjust as evidence is added.</p>
      <div style="text-align:center;margin:1.5rem 0">
        <div id="wiz-prob-value" style="font-size:2rem;font-weight:700">${pct}%</div>
        <input type="range" id="wiz-prob-slider" min="5" max="95" value="${pct}" style="width:100%;margin-top:0.5rem">
        <div style="display:flex;justify-content:space-between;font-size:0.75rem;color:var(--text-secondary)">
          <span>Very unlikely (5%)</span>
          <span>Even odds (50%)</span>
          <span>Very likely (95%)</span>
        </div>
      </div>
      <div class="card" style="background:var(--bg-tertiary);padding:1rem">
        <h4 style="margin-top:0"><i class="fa-solid fa-clipboard-check"></i> Summary</h4>
        <p><strong>${escapeHtml(wizardData.title)}</strong></p>
        ${wizardData.category ? `<p>Category: ${escapeHtml(wizardData.category)}</p>` : ''}
        ${wizardData.timeframe ? `<p>Timeframe: ${escapeHtml(wizardData.timeframe)}</p>` : ''}
        <p>Watching ${wizardData.watches.length} entities</p>
      </div>`;
    footer.innerHTML = `
      <button class="btn btn-secondary" id="wiz-back3"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="wiz-submit"><i class="fa-solid fa-check"></i> Create Assessment</button>`;

    const slider = document.getElementById('wiz-prob-slider');
    const probDisplay = document.getElementById('wiz-prob-value');
    slider.addEventListener('input', () => {
      wizardData.probability = parseInt(slider.value) / 100;
      probDisplay.textContent = slider.value + '%';
    });

    footer.querySelector('#wiz-back3').addEventListener('click', () => { wizardStep = 2; renderWizardStep(); });
    footer.querySelector('#wiz-submit').addEventListener('click', submitAssessment);
  }
}

async function submitAssessment() {
  try {
    await engram._post('/assessments', {
      title: wizardData.title,
      description: wizardData.description || undefined,
      category: wizardData.category || undefined,
      timeframe: wizardData.timeframe || undefined,
      initial_probability: wizardData.probability,
      watches: wizardData.watches,
    });
    document.getElementById('assess-create-modal').classList.remove('visible');
    showToast('Assessment created', 'success');
    await loadAssessList();
  } catch (err) {
    showToast(`Failed: ${err.message}`, 'error');
  }
}

// --- Prompt helpers ---
async function promptAddEvidence(label) {
  const nodeLabel = prompt('Entity label for evidence:');
  if (!nodeLabel) return;
  const direction = prompt('Direction: supports / contradicts', 'supports');
  if (direction !== 'supports' && direction !== 'contradicts') {
    showToast('Direction must be "supports" or "contradicts"', 'error');
    return;
  }
  try {
    await engram._post(`/assessments/${encodeURIComponent(label)}/evidence`, {
      node_label: nodeLabel,
      direction,
    });
    showToast('Evidence added', 'success');
    location.hash = `#/assess/${encodeURIComponent(label)}`;
  } catch (err) {
    showToast(`Failed: ${err.message}`, 'error');
  }
}

async function promptAddWatch(label) {
  const entityLabel = prompt('Entity label to watch:');
  if (!entityLabel) return;
  try {
    await engram._post(`/assessments/${encodeURIComponent(label)}/watch`, {
      entity_label: entityLabel,
    });
    showToast('Watch added', 'success');
    location.hash = `#/assess/${encodeURIComponent(label)}`;
  } catch (err) {
    showToast(`Failed: ${err.message}`, 'error');
  }
}
