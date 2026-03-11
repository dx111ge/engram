/* ============================================
   engram - Insights View
   Knowledge intelligence, gap analysis,
   assessments, inference rules, action rules
   ============================================ */

// --- Pagination state for gaps ---
let insightsGapPage = 0;
const GAPS_PER_PAGE = 10;
let insightsGapsAll = [];

const GAP_KIND_LABELS = {
  frontier_node:        'Needs more connections',
  isolated:             'Isolated fact',
  weak:                 'Low confidence',
  missing_type:         'No type assigned',
  structural_hole:      'Missing bridge connection',
  temporal_gap:         'Outdated information',
  confidence_desert:    'Weak knowledge area',
  asymmetric_cluster:   'Unbalanced cluster',
  coordinated_cluster:  'Coordinated cluster',
};

function gapKindLabel(gap) {
  if (gap.kind && GAP_KIND_LABELS[gap.kind]) return GAP_KIND_LABELS[gap.kind];
  if (gap.description || gap.message) return gap.description || gap.message;
  return 'Knowledge gap detected';
}

// --- Assessment state ---
let assessCurrentFilter = { category: '', status: 'active' };
let assessCurrentSort = 'last_evaluated';

// --- Inference Rule wizard state ---
let inferenceWizardStep = 1;
let inferenceWizardData = { name: '', conditions: [], actions: [] };

// --- Action Rule wizard state ---
let actionWizardStep = 1;
let actionWizardData = { name: '', trigger: '', conditions: [], actions: [] };

// --- Assessment create wizard state ---
let assessWizardStep = 1;
let assessWizardData = {};

// ============================================================
//  MAIN INSIGHTS ROUTE
// ============================================================

router.register('/insights', async () => {
  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-chart-line"></i> Insights</h1>
        <p class="text-secondary" style="margin-top:0.25rem">What your knowledge base needs attention on</p>
      </div>
    </div>
    <div id="insights-content">${loadingHTML('Analyzing your knowledge base...')}</div>
  `);

  await loadInsightsView();
});

async function loadInsightsView() {
  const container = document.getElementById('insights-content');
  let html = '';

  // -------------------------------------------------------
  // 1. Knowledge Health summary
  // -------------------------------------------------------
  let stats = null;
  try { stats = await engram.stats(); } catch (_) {}

  html += `
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-heart-pulse"></i> Knowledge Health</h3>
      </div>
      <div class="grid-4">
        <div class="stat-card">
          <div class="stat-value">${stats ? (stats.nodes || stats.node_count || 0) : '--'}</div>
          <div class="stat-label">Facts</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${stats ? (stats.edges || stats.edge_count || 0) : '--'}</div>
          <div class="stat-label">Connections</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${stats ? (stats.types || stats.type_count || '--') : '--'}</div>
          <div class="stat-label">Types</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${stats && stats.avg_confidence != null ? Math.round(stats.avg_confidence * 100) + '%' : '--'}</div>
          <div class="stat-label">Avg Strength</div>
        </div>
      </div>
    </div>`;

  // -------------------------------------------------------
  // 2. Knowledge Gaps
  // -------------------------------------------------------
  let gapsData = null;
  try {
    const gapsPromise = engram._fetch('/reason/gaps');
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000));
    gapsData = await Promise.race([gapsPromise, timeout]);
  } catch (_) {}

  insightsGapsAll = [];
  if (gapsData) {
    insightsGapsAll = Array.isArray(gapsData) ? gapsData : (gapsData.gaps || []);
  }
  insightsGapPage = 0;

  html += `
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header" style="display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:0.5rem">
        <h3><i class="fa-solid fa-triangle-exclamation"></i> Things to Improve</h3>
        <div style="display:flex;gap:0.5rem;align-items:center">
          <button class="btn btn-secondary" id="btn-resolve-selected" style="font-size:0.8rem;padding:0.3rem 0.6rem;display:none">
            <i class="fa-solid fa-check-double"></i> Resolve Selected
          </button>
        </div>
      </div>
      <div id="gaps-list-area"></div>
    </div>`;

  // -------------------------------------------------------
  // 3. Frontier nodes
  // -------------------------------------------------------
  let frontierData = null;
  try {
    const frontierPromise = engram._fetch('/reason/frontier');
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000));
    frontierData = await Promise.race([frontierPromise, timeout]);
  } catch (_) {}

  if (frontierData) {
    const frontier = Array.isArray(frontierData) ? frontierData : (frontierData.nodes || []);
    if (frontier.length > 0) {
      html += `
        <div class="card" style="margin-bottom:1.5rem">
          <div class="card-header">
            <h3><i class="fa-solid fa-border-none"></i> Facts Needing More Context</h3>
          </div>
          <p class="text-secondary mb-2" style="font-size:0.85rem">
            These facts have few connections. Consider adding more relationships to strengthen your knowledge base.
          </p>
          <div style="display:flex;flex-wrap:wrap;gap:0.5rem">
            ${frontier.map(n => {
              const label = n.label || n;
              return `<a href="#/node/${encodeURIComponent(label)}" class="badge badge-active" style="cursor:pointer">${escapeHtml(label)}</a>`;
            }).join('')}
          </div>
        </div>`;
    }
  }

  // -------------------------------------------------------
  // 4. Assessments section (absorbed from assess tab)
  // -------------------------------------------------------
  html += `
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-scale-balanced"></i> Assessments</h3>
        <div style="display:flex;gap:0.5rem;align-items:center">
          <select id="assess-cat-filter" class="input-sm" style="min-width:120px;font-size:0.8rem;padding:0.25rem 0.4rem">
            <option value="">All categories</option>
            <option value="financial">Financial</option>
            <option value="geopolitical">Geopolitical</option>
            <option value="technical">Technical</option>
            <option value="military">Military</option>
            <option value="social">Social</option>
            <option value="other">Other</option>
          </select>
          <select id="assess-status-filter" class="input-sm" style="min-width:100px;font-size:0.8rem;padding:0.25rem 0.4rem">
            <option value="active" selected>Active</option>
            <option value="paused">Paused</option>
            <option value="archived">Archived</option>
            <option value="resolved">Resolved</option>
            <option value="">All</option>
          </select>
          <select id="assess-sort" class="input-sm" style="min-width:120px;font-size:0.8rem;padding:0.25rem 0.4rem">
            <option value="last_evaluated">Last evaluated</option>
            <option value="probability">Probability</option>
            <option value="shift">Recent shift</option>
            <option value="title">Title</option>
          </select>
          <button class="btn btn-sm btn-primary" id="assess-create-btn">
            <i class="fa-solid fa-plus"></i> New
          </button>
        </div>
      </div>
      <div id="assess-list"></div>
    </div>`;

  // -------------------------------------------------------
  // 5. Full Analysis scan
  // -------------------------------------------------------
  html += `
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-radar"></i> Full Analysis</h3>
      </div>
      <p class="text-secondary mb-2" style="font-size:0.85rem">
        Run a comprehensive scan of your knowledge base to find conflicts, gaps, and areas for improvement.
      </p>
      <button class="btn btn-primary" id="btn-scan">
        <i class="fa-solid fa-magnifying-glass-chart"></i> Scan Now
      </button>
      <div id="scan-result" class="mt-1"></div>
    </div>`;

  // -------------------------------------------------------
  // 6. Recommended Actions
  // -------------------------------------------------------
  html += `
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-lightbulb"></i> Recommended Actions</h3>
      </div>
      <div id="suggestions-content">
        <button class="btn btn-secondary" id="btn-suggest">
          <i class="fa-solid fa-wand-magic-sparkles"></i> Get Recommendations
        </button>
        <div id="suggest-result" class="mt-1"></div>
      </div>
    </div>`;

  // -------------------------------------------------------
  // 7. Inference Rules (wizard-style)
  // -------------------------------------------------------
  html += `
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-code-branch"></i> Inference Rules</h3>
        <button class="btn btn-sm btn-primary" id="btn-add-inference-rule">
          <i class="fa-solid fa-plus"></i> Add Rule
        </button>
      </div>
      <div id="inference-rules-list"></div>
    </div>`;

  // -------------------------------------------------------
  // 8. Action Rules (wizard-style)
  // -------------------------------------------------------
  html += `
    <div class="card" style="margin-bottom:1.5rem">
      <div class="card-header">
        <h3><i class="fa-solid fa-bolt"></i> Action Rules</h3>
        <button class="btn btn-sm btn-primary" id="btn-add-action-rule">
          <i class="fa-solid fa-plus"></i> Add Rule
        </button>
      </div>
      <div id="action-rules-list"></div>
    </div>`;

  // -------------------------------------------------------
  // Assessment Create Modal (shared across the page)
  // -------------------------------------------------------
  html += `
    <div class="modal-overlay" id="assess-create-modal">
      <div class="modal" style="max-width:600px">
        <div class="modal-header">
          <h3><i class="fa-solid fa-scale-balanced"></i> New Assessment</h3>
          <button class="btn-icon" id="assess-modal-close"><i class="fa-solid fa-xmark"></i></button>
        </div>
        <div class="modal-body" id="assess-wizard-body"></div>
        <div class="modal-footer" id="assess-wizard-footer"></div>
      </div>
    </div>`;

  // -------------------------------------------------------
  // Inference Rule Wizard Modal
  // -------------------------------------------------------
  html += `
    <div class="modal-overlay" id="inference-rule-modal">
      <div class="modal" style="max-width:700px">
        <div class="modal-header">
          <h3><i class="fa-solid fa-code-branch"></i> Inference Rule</h3>
          <button class="btn-icon" id="inference-modal-close"><i class="fa-solid fa-xmark"></i></button>
        </div>
        <div class="modal-body" id="inference-wizard-body"></div>
        <div class="modal-footer" id="inference-wizard-footer"></div>
      </div>
    </div>`;

  // -------------------------------------------------------
  // Action Rule Wizard Modal
  // -------------------------------------------------------
  html += `
    <div class="modal-overlay" id="action-rule-modal">
      <div class="modal" style="max-width:700px">
        <div class="modal-header">
          <h3><i class="fa-solid fa-bolt"></i> Action Rule</h3>
          <button class="btn-icon" id="action-modal-close"><i class="fa-solid fa-xmark"></i></button>
        </div>
        <div class="modal-body" id="action-wizard-body"></div>
        <div class="modal-footer" id="action-wizard-footer"></div>
      </div>
    </div>`;

  // -------------------------------------------------------
  // Inject HTML
  // -------------------------------------------------------
  container.innerHTML = html;

  // -------------------------------------------------------
  // Post-render: gaps
  // -------------------------------------------------------
  if (insightsGapsAll.length > 0) {
    renderGapsPage();
    setupResolveButton();
  } else {
    const gapsArea = document.getElementById('gaps-list-area');
    if (gapsArea) {
      gapsArea.innerHTML = '<div class="feature-status" style="padding:0.75rem;font-size:0.85rem;color:var(--text-secondary)"><i class="fa-solid fa-circle" style="color:var(--accent-bright);font-size:0.5rem;vertical-align:middle;margin-right:0.4rem"></i>No gaps detected. Your knowledge base looks healthy.</div>';
    }
  }

  // -------------------------------------------------------
  // Post-render: assessments
  // -------------------------------------------------------
  await loadInsightsAssessList();
  setupAssessListeners();

  // -------------------------------------------------------
  // Post-render: scan button
  // -------------------------------------------------------
  const scanBtn = document.getElementById('btn-scan');
  if (scanBtn) {
    scanBtn.addEventListener('click', async () => {
      scanBtn.disabled = true;
      const resultDiv = document.getElementById('scan-result');
      resultDiv.innerHTML = loadingHTML('Scanning knowledge base...');
      try {
        const result = await engram._post('/reason/scan', {});
        let scanHtml = '<div class="rule-results">';
        scanHtml += '<pre>' + escapeHtml(JSON.stringify(result, null, 2)) + '</pre>';
        scanHtml += '</div>';
        resultDiv.innerHTML = scanHtml;
        showToast('Scan complete', 'success');
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
        showToast('Scan failed: ' + err.message, 'error');
      } finally {
        scanBtn.disabled = false;
      }
    });
  }

  // -------------------------------------------------------
  // Post-render: suggest button
  // -------------------------------------------------------
  const suggestBtn = document.getElementById('btn-suggest');
  if (suggestBtn) {
    suggestBtn.addEventListener('click', async () => {
      suggestBtn.disabled = true;
      const resultDiv = document.getElementById('suggest-result');
      resultDiv.innerHTML = loadingHTML('Generating recommendations...');
      try {
        const result = await engram._post('/reason/suggest', {});
        const suggestions = Array.isArray(result) ? result : (result.suggestions || []);
        if (suggestions.length > 0) {
          resultDiv.innerHTML = '<div style="display:flex;flex-direction:column;gap:0.5rem;margin-top:0.5rem">'
            + suggestions.map(s => `
              <div style="padding:0.5rem 0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border);font-size:0.9rem">
                <i class="fa-solid fa-lightbulb" style="color:var(--accent-bright);margin-right:0.4rem"></i>
                ${escapeHtml(typeof s === 'string' ? s : s.description || JSON.stringify(s))}
              </div>`).join('')
            + '</div>';
        } else {
          resultDiv.innerHTML = '<p class="text-muted mt-1" style="font-size:0.85rem">No recommendations at this time. Your knowledge base is in good shape.</p>';
        }
        showToast('Recommendations generated', 'success');
      } catch (err) {
        resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      } finally {
        suggestBtn.disabled = false;
      }
    });
  }

  // -------------------------------------------------------
  // Post-render: inference rules
  // -------------------------------------------------------
  await loadInferenceRulesList();
  document.getElementById('btn-add-inference-rule').addEventListener('click', openInferenceRuleWizard);
  setupInferenceModalClose();

  // -------------------------------------------------------
  // Post-render: action rules
  // -------------------------------------------------------
  await loadActionRulesList();
  document.getElementById('btn-add-action-rule').addEventListener('click', openActionRuleWizard);
  setupActionModalClose();
}


// ============================================================
//  GAPS RENDERING (kept from original)
// ============================================================

function renderGapsPage() {
  const area = document.getElementById('gaps-list-area');
  if (!area) return;

  const total = insightsGapsAll.length;

  if (total === 0) {
    area.innerHTML = emptyStateHTML('fa-circle-check', 'No issues detected. Your knowledge base looks healthy.');
    const resolveBtn = document.getElementById('btn-resolve-selected');
    if (resolveBtn) resolveBtn.style.display = 'none';
    return;
  }

  const start = insightsGapPage * GAPS_PER_PAGE;
  const end = Math.min(start + GAPS_PER_PAGE, total);
  const pageGaps = insightsGapsAll.slice(start, end);
  const totalPages = Math.ceil(total / GAPS_PER_PAGE);

  let html = '';

  // Select-all header
  html += `
    <div style="display:flex;align-items:center;gap:0.5rem;padding:0.5rem 0;border-bottom:1px solid var(--border);margin-bottom:0.75rem">
      <input type="checkbox" id="gap-select-all" style="cursor:pointer;width:16px;height:16px">
      <label for="gap-select-all" style="font-size:0.85rem;color:var(--text-secondary);cursor:pointer">Select all on this page</label>
      <span style="margin-left:auto;font-size:0.8rem;color:var(--text-muted)">Showing ${start + 1}-${end} of ${total}</span>
    </div>`;

  // Gap items
  html += '<div style="display:flex;flex-direction:column;gap:0.75rem">';
  for (let i = 0; i < pageGaps.length; i++) {
    const gap = pageGaps[i];
    const globalIdx = start + i;
    const severity = getGapSeverity(gap);
    const borderColor = severity === 'high' ? 'var(--error)' : severity === 'medium' ? 'var(--warning)' : 'var(--confidence-mid)';
    const title = gapKindLabel(gap);
    const description = gap.description || gap.message || '';
    const suggestedQueries = gap.suggested_queries || [];

    html += `
      <div style="border-left:3px solid ${borderColor};padding:0.75rem 1rem;background:var(--bg-secondary);border-radius:0 var(--radius-sm) var(--radius-sm) 0;display:flex;gap:0.75rem;align-items:flex-start">
        <input type="checkbox" class="gap-checkbox" data-idx="${globalIdx}" style="cursor:pointer;width:16px;height:16px;flex-shrink:0;margin-top:0.15rem">
        <div style="flex:1;min-width:0">
          <div style="font-weight:600;margin-bottom:0.3rem">${escapeHtml(title)}</div>`;

    if (description && description !== title) {
      html += `<div style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.3rem">${escapeHtml(description)}</div>`;
    }

    if (gap.entities && gap.entities.length > 0) {
      html += `
        <div style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.3rem">
          <i class="fa-solid fa-diagram-project"></i> Affected: ${gap.entities.map(e => '<a href="#/node/' + encodeURIComponent(e) + '">' + escapeHtml(e) + '</a>').join(', ')}
        </div>`;
    }

    if (gap.suggestion) {
      html += `<div style="font-size:0.85rem;color:var(--accent-bright)"><i class="fa-solid fa-lightbulb"></i> ${escapeHtml(gap.suggestion)}</div>`;
    }

    if (suggestedQueries.length > 0) {
      html += `<div style="margin-top:0.4rem">`;
      for (const sq of suggestedQueries) {
        html += `
          <div style="font-size:0.8rem;color:var(--accent-bright);margin-bottom:0.2rem">
            <i class="fa-solid fa-arrow-right" style="margin-right:0.3rem"></i>${escapeHtml(sq)}
          </div>`;
      }
      html += '</div>';
    }

    html += `</div></div>`;
  }
  html += '</div>';

  // Pagination controls
  if (totalPages > 1) {
    html += `
      <div style="display:flex;justify-content:center;align-items:center;gap:0.75rem;margin-top:1rem;padding-top:0.75rem;border-top:1px solid var(--border)">
        <button class="btn btn-secondary" id="btn-gaps-prev" style="font-size:0.8rem;padding:0.3rem 0.6rem" ${insightsGapPage === 0 ? 'disabled' : ''}>
          <i class="fa-solid fa-chevron-left"></i> Previous
        </button>
        <span style="font-size:0.85rem;color:var(--text-secondary)">Page ${insightsGapPage + 1} of ${totalPages}</span>
        <button class="btn btn-secondary" id="btn-gaps-next" style="font-size:0.8rem;padding:0.3rem 0.6rem" ${insightsGapPage >= totalPages - 1 ? 'disabled' : ''}>
          Next <i class="fa-solid fa-chevron-right"></i>
        </button>
      </div>`;
  }

  area.innerHTML = html;

  // Pagination event listeners
  const prevBtn = document.getElementById('btn-gaps-prev');
  if (prevBtn) {
    prevBtn.addEventListener('click', () => {
      if (insightsGapPage > 0) { insightsGapPage--; renderGapsPage(); setupResolveButton(); }
    });
  }
  const nextBtn = document.getElementById('btn-gaps-next');
  if (nextBtn) {
    nextBtn.addEventListener('click', () => {
      const tp = Math.ceil(insightsGapsAll.length / GAPS_PER_PAGE);
      if (insightsGapPage < tp - 1) { insightsGapPage++; renderGapsPage(); setupResolveButton(); }
    });
  }

  // Select-all checkbox
  const selectAll = document.getElementById('gap-select-all');
  if (selectAll) {
    selectAll.addEventListener('change', () => {
      const boxes = area.querySelectorAll('.gap-checkbox');
      boxes.forEach(cb => { cb.checked = selectAll.checked; });
      updateResolveButtonVisibility();
    });
  }

  // Individual checkbox change
  const boxes = area.querySelectorAll('.gap-checkbox');
  boxes.forEach(cb => {
    cb.addEventListener('change', () => {
      updateResolveButtonVisibility();
      if (selectAll) {
        selectAll.checked = Array.from(boxes).every(b => b.checked);
      }
    });
  });
}

function updateResolveButtonVisibility() {
  const resolveBtn = document.getElementById('btn-resolve-selected');
  if (!resolveBtn) return;
  const checked = document.querySelectorAll('.gap-checkbox:checked');
  resolveBtn.style.display = checked.length > 0 ? 'inline-flex' : 'none';
}

function setupResolveButton() {
  const resolveBtn = document.getElementById('btn-resolve-selected');
  if (!resolveBtn) return;
  updateResolveButtonVisibility();

  const newBtn = resolveBtn.cloneNode(true);
  resolveBtn.parentNode.replaceChild(newBtn, resolveBtn);
  newBtn.style.display = 'none';
  updateResolveButtonVisibility();

  newBtn.addEventListener('click', () => {
    const checked = document.querySelectorAll('.gap-checkbox:checked');
    const indices = Array.from(checked).map(cb => parseInt(cb.dataset.idx, 10));

    indices.sort((a, b) => b - a);
    for (const idx of indices) {
      insightsGapsAll.splice(idx, 1);
    }

    const totalPages = Math.ceil(insightsGapsAll.length / GAPS_PER_PAGE);
    if (insightsGapPage >= totalPages && insightsGapPage > 0) {
      insightsGapPage = totalPages - 1;
    }

    renderGapsPage();
    setupResolveButton();
    showToast(indices.length + ' item' + (indices.length !== 1 ? 's' : '') + ' resolved', 'success');
  });
}

function getGapSeverity(gap) {
  if (typeof gap.severity === 'string') return gap.severity.toLowerCase();
  const score = typeof gap.severity === 'number' ? gap.severity : gap.score;
  if (score != null) {
    if (score > 0.7) return 'high';
    if (score > 0.4) return 'medium';
  }
  return 'low';
}

function insightCapabilityCard(icon, title, description) {
  return `
    <div style="display:flex;gap:0.75rem;align-items:flex-start;padding:0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border)">
      <i class="fa-solid ${icon}" style="color:var(--accent-bright);font-size:1.2rem;margin-top:0.15rem;flex-shrink:0"></i>
      <div>
        <div style="font-weight:600;font-size:0.9rem;margin-bottom:0.2rem">${escapeHtml(title)}</div>
        <div style="font-size:0.8rem;color:var(--text-muted)">${escapeHtml(description)}</div>
      </div>
    </div>`;
}


// ============================================================
//  ASSESSMENTS - LIST (inline within Insights)
// ============================================================

function setupAssessListeners() {
  // Create button
  const createBtn = document.getElementById('assess-create-btn');
  if (createBtn) createBtn.addEventListener('click', openAssessCreateWizard);

  // Modal close
  const closeBtn = document.getElementById('assess-modal-close');
  if (closeBtn) closeBtn.addEventListener('click', () => {
    document.getElementById('assess-create-modal').classList.remove('visible');
  });
  const modalOverlay = document.getElementById('assess-create-modal');
  if (modalOverlay) modalOverlay.addEventListener('click', (e) => {
    if (e.target === modalOverlay) modalOverlay.classList.remove('visible');
  });

  // Filter/sort listeners
  const catFilter = document.getElementById('assess-cat-filter');
  if (catFilter) catFilter.addEventListener('change', (e) => {
    assessCurrentFilter.category = e.target.value;
    loadInsightsAssessList();
  });
  const statusFilter = document.getElementById('assess-status-filter');
  if (statusFilter) statusFilter.addEventListener('change', (e) => {
    assessCurrentFilter.status = e.target.value;
    loadInsightsAssessList();
  });
  const sortSelect = document.getElementById('assess-sort');
  if (sortSelect) sortSelect.addEventListener('change', (e) => {
    assessCurrentSort = e.target.value;
    loadInsightsAssessList();
  });
}

async function loadInsightsAssessList() {
  const container = document.getElementById('assess-list');
  if (!container) return;

  try {
    const params = {};
    if (assessCurrentFilter.category) params.category = assessCurrentFilter.category;
    if (assessCurrentFilter.status) params.status = assessCurrentFilter.status;
    const data = await engram.listAssessments(params);
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
      container.innerHTML = '<div style="padding:0.75rem;font-size:0.85rem;color:var(--text-secondary)"><i class="fa-solid fa-circle" style="color:var(--accent-bright);font-size:0.5rem;vertical-align:middle;margin-right:0.4rem"></i>No assessments match the current filter. Create one to start tracking predictions.</div>';
      return;
    }

    container.innerHTML = `<div class="assess-grid">${assessments.map(a => assessCard(a)).join('')}</div>`;

    // Draw mini gauges
    drawAssessGauges();

    // Click handlers for cards
    container.querySelectorAll('.assess-card').forEach(card => {
      card.addEventListener('click', () => {
        const label = card.dataset.label;
        location.hash = `#/assess/${encodeURIComponent(label)}`;
      });
    });
  } catch (err) {
    container.innerHTML = '<div class="feature-status" style="padding:0.75rem;font-size:0.85rem;color:var(--text-secondary)"><i class="fa-solid fa-circle" style="color:var(--accent-bright);font-size:0.5rem;vertical-align:middle;margin-right:0.4rem"></i>Assessments available. Add your first assessment to start tracking predictions.</div>';
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
    </div>`;
}

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


// ============================================================
//  ASSESSMENTS - CREATE WIZARD
// ============================================================

function openAssessCreateWizard() {
  assessWizardStep = 1;
  assessWizardData = { title: '', description: '', category: '', timeframe: '', watches: [], probability: 0.50 };
  renderAssessWizardStep();
  document.getElementById('assess-create-modal').classList.add('visible');
}

function renderAssessWizardStep() {
  const body = document.getElementById('assess-wizard-body');
  const footer = document.getElementById('assess-wizard-footer');

  if (assessWizardStep === 1) {
    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="active">1</span> <span>2</span> <span>3</span></div>
      <label>Title <span class="text-secondary">(the hypothesis)</span></label>
      <input type="text" id="wiz-title" class="input" placeholder="e.g. NVIDIA stock > $200 by Q3 2026" value="${escapeHtml(assessWizardData.title)}">
      <label style="margin-top:1rem">Description <span class="text-secondary">(optional)</span></label>
      <textarea id="wiz-desc" class="input" rows="2" placeholder="Additional context...">${escapeHtml(assessWizardData.description)}</textarea>
      <div class="grid-2" style="margin-top:1rem">
        <div>
          <label>Category</label>
          <select id="wiz-cat" class="input">
            <option value="">Select...</option>
            <option value="financial" ${assessWizardData.category === 'financial' ? 'selected' : ''}>Financial</option>
            <option value="geopolitical" ${assessWizardData.category === 'geopolitical' ? 'selected' : ''}>Geopolitical</option>
            <option value="technical" ${assessWizardData.category === 'technical' ? 'selected' : ''}>Technical</option>
            <option value="military" ${assessWizardData.category === 'military' ? 'selected' : ''}>Military</option>
            <option value="social" ${assessWizardData.category === 'social' ? 'selected' : ''}>Social</option>
            <option value="other" ${assessWizardData.category === 'other' ? 'selected' : ''}>Other</option>
          </select>
        </div>
        <div>
          <label>Timeframe</label>
          <input type="text" id="wiz-time" class="input" placeholder="e.g. Q3 2026" value="${escapeHtml(assessWizardData.timeframe)}">
        </div>
      </div>`;
    footer.innerHTML = `
      <button class="btn btn-secondary" id="wiz-cancel">Cancel</button>
      <button class="btn btn-primary" id="wiz-next1">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    footer.querySelector('#wiz-cancel').addEventListener('click', () => {
      document.getElementById('assess-create-modal').classList.remove('visible');
    });
    footer.querySelector('#wiz-next1').addEventListener('click', () => {
      assessWizardData.title = document.getElementById('wiz-title').value.trim();
      assessWizardData.description = document.getElementById('wiz-desc').value.trim();
      assessWizardData.category = document.getElementById('wiz-cat').value;
      assessWizardData.timeframe = document.getElementById('wiz-time').value.trim();
      if (!assessWizardData.title) { showToast('Title is required', 'error'); return; }
      assessWizardStep = 2;
      renderAssessWizardStep();
    });

  } else if (assessWizardStep === 2) {
    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="active">2</span> <span>3</span></div>
      <label>Watch Entities</label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">Search and select entities to monitor. New facts about these will trigger re-evaluation.</p>
      <div style="display:flex;gap:0.5rem;margin-bottom:1rem">
        <input type="text" id="wiz-entity-search" class="input" placeholder="Search entities...">
        <button class="btn btn-secondary" id="wiz-entity-add"><i class="fa-solid fa-plus"></i></button>
      </div>
      <div id="wiz-entity-results"></div>
      <div id="wiz-selected-entities" style="margin-top:1rem;display:flex;flex-wrap:wrap;gap:0.5rem">
        ${assessWizardData.watches.map(w => `<span class="badge badge-lg">${escapeHtml(w)} <button class="btn-icon btn-xs wiz-remove-entity" data-entity="${escapeHtml(w)}"><i class="fa-solid fa-xmark"></i></button></span>`).join('')}
      </div>`;
    footer.innerHTML = `
      <button class="btn btn-secondary" id="wiz-back2"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="wiz-next2">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    footer.querySelector('#wiz-back2').addEventListener('click', () => { assessWizardStep = 1; renderAssessWizardStep(); });
    footer.querySelector('#wiz-next2').addEventListener('click', () => { assessWizardStep = 3; renderAssessWizardStep(); });

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
            if (!assessWizardData.watches.includes(label)) {
              assessWizardData.watches.push(label);
              renderAssessWizardStep();
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
        assessWizardData.watches = assessWizardData.watches.filter(w => w !== entity);
        renderAssessWizardStep();
      });
    });

  } else if (assessWizardStep === 3) {
    const pct = Math.round(assessWizardData.probability * 100);
    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="complete">2</span> <span class="active">3</span></div>
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
        <p><strong>${escapeHtml(assessWizardData.title)}</strong></p>
        ${assessWizardData.category ? `<p>Category: ${escapeHtml(assessWizardData.category)}</p>` : ''}
        ${assessWizardData.timeframe ? `<p>Timeframe: ${escapeHtml(assessWizardData.timeframe)}</p>` : ''}
        <p>Watching ${assessWizardData.watches.length} entities</p>
      </div>`;
    footer.innerHTML = `
      <button class="btn btn-secondary" id="wiz-back3"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="wiz-submit"><i class="fa-solid fa-check"></i> Create Assessment</button>`;

    const slider = document.getElementById('wiz-prob-slider');
    const probDisplay = document.getElementById('wiz-prob-value');
    slider.addEventListener('input', () => {
      assessWizardData.probability = parseInt(slider.value) / 100;
      probDisplay.textContent = slider.value + '%';
    });

    footer.querySelector('#wiz-back3').addEventListener('click', () => { assessWizardStep = 2; renderAssessWizardStep(); });
    footer.querySelector('#wiz-submit').addEventListener('click', submitAssessment);
  }
}

async function submitAssessment() {
  try {
    await engram._post('/assessments', {
      title: assessWizardData.title,
      description: assessWizardData.description || undefined,
      category: assessWizardData.category || undefined,
      timeframe: assessWizardData.timeframe || undefined,
      initial_probability: assessWizardData.probability,
      watches: assessWizardData.watches,
    });
    document.getElementById('assess-create-modal').classList.remove('visible');
    showToast('Assessment created', 'success');
    await loadInsightsAssessList();
  } catch (err) {
    showToast(`Failed: ${err.message}`, 'error');
  }
}


// ============================================================
//  ASSESSMENTS - DETAIL VIEW (separate route)
// ============================================================

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
          <a href="#/insights" class="btn btn-ghost btn-sm" style="margin-bottom:0.5rem">
            <i class="fa-solid fa-arrow-left"></i> Back to Insights
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
    const pctVal = history[0].probability;
    const x = pad.left + cw / 2;
    const y = pad.top + ch - pctVal * ch;
    ctx.beginPath();
    ctx.arc(x, y, 4, 0, Math.PI * 2);
    ctx.fillStyle = '#3b82f6';
    ctx.fill();
    return;
  }

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

  // Points
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

// --- Evidence/Watch prompt helpers ---
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


// ============================================================
//  INFERENCE RULES - LIST + WIZARD
// ============================================================

async function loadInferenceRulesList() {
  const container = document.getElementById('inference-rules-list');
  if (!container) return;

  try {
    const data = await engram.listRules();
    const rules = data.rules || [];

    if (rules.length === 0) {
      container.innerHTML = '<div class="feature-status" style="padding:0.75rem;font-size:0.85rem;color:var(--text-secondary)"><i class="fa-solid fa-circle" style="color:var(--accent-bright);font-size:0.5rem;vertical-align:middle;margin-right:0.4rem"></i>No inference rules defined. Add a rule to automatically derive new knowledge from patterns in your graph.</div>';
      return;
    }

    let html = '<div style="display:flex;flex-direction:column;gap:0.5rem">';
    for (let i = 0; i < rules.length; i++) {
      const rule = rules[i];
      const ruleName = extractRuleName(rule);
      html += `
        <div style="display:flex;align-items:center;gap:0.75rem;padding:0.6rem 0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border)">
          <i class="fa-solid fa-code-branch" style="color:var(--accent-bright);flex-shrink:0"></i>
          <div style="flex:1;min-width:0">
            <div style="font-weight:600;font-size:0.9rem">${escapeHtml(ruleName)}</div>
            <div style="font-size:0.75rem;color:var(--text-muted);font-family:monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">${escapeHtml(typeof rule === 'string' ? rule : JSON.stringify(rule))}</div>
          </div>
          <button class="btn btn-ghost btn-sm remove-btn inference-rule-delete" data-idx="${i}" title="Delete rule">
            <i class="fa-solid fa-trash" style="color:var(--error)"></i>
          </button>
        </div>`;
    }
    html += '</div>';
    container.innerHTML = html;

    // Delete handlers
    container.querySelectorAll('.inference-rule-delete').forEach(btn => {
      btn.addEventListener('click', async () => {
        if (!confirm('Delete this inference rule?')) return;
        try {
          // Reload all rules, remove this one, re-save
          const current = await engram.listRules();
          const remaining = (current.rules || []).filter((_, idx) => idx !== parseInt(btn.dataset.idx));
          await engram.clearRules();
          if (remaining.length > 0) {
            await engram.loadRules({ rules: remaining });
          }
          showToast('Inference rule deleted', 'success');
          await loadInferenceRulesList();
        } catch (err) {
          showToast('Failed to delete rule: ' + err.message, 'error');
        }
      });
    });
  } catch (err) {
    container.innerHTML = '<div class="feature-status" style="padding:0.75rem;font-size:0.85rem;color:var(--text-secondary)"><i class="fa-solid fa-circle" style="color:var(--accent-bright);font-size:0.5rem;vertical-align:middle;margin-right:0.4rem"></i>Inference rules available. Add a rule to automatically derive new knowledge from patterns.</div>';
  }
}

function extractRuleName(rule) {
  if (typeof rule === 'string') {
    const match = rule.match(/^rule\s+(\S+)/);
    if (match) return match[1];
    return rule.substring(0, 40);
  }
  if (rule && rule.name) return rule.name;
  return 'Unnamed rule';
}

function setupInferenceModalClose() {
  const closeBtn = document.getElementById('inference-modal-close');
  if (closeBtn) closeBtn.addEventListener('click', () => {
    document.getElementById('inference-rule-modal').classList.remove('visible');
  });
  const overlay = document.getElementById('inference-rule-modal');
  if (overlay) overlay.addEventListener('click', (e) => {
    if (e.target === overlay) overlay.classList.remove('visible');
  });
}

function openInferenceRuleWizard() {
  inferenceWizardStep = 1;
  inferenceWizardData = {
    name: '',
    conditions: [{ type: 'edge', from_var: 'a', rel: '', to_var: 'b' }],
    actions: [{ type: 'create_edge', from_var: 'a', rel: '', to_var: 'b', confidence_mode: 'min', confidence_val: '' }],
  };
  renderInferenceWizardStep();
  document.getElementById('inference-rule-modal').classList.add('visible');
}

function renderInferenceWizardStep() {
  const body = document.getElementById('inference-wizard-body');
  const footer = document.getElementById('inference-wizard-footer');

  if (inferenceWizardStep === 1) {
    // Step 1: Name
    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="active">1</span> <span>2</span> <span>3</span> <span>4</span></div>
      <label>Rule Name</label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">A short identifier for this inference rule (no spaces).</p>
      <input type="text" id="inf-wiz-name" class="input" placeholder="e.g. transitive_trust" value="${escapeHtml(inferenceWizardData.name)}">`;
    footer.innerHTML = `
      <button class="btn btn-secondary" id="inf-wiz-cancel">Cancel</button>
      <button class="btn btn-primary" id="inf-wiz-next1">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    footer.querySelector('#inf-wiz-cancel').addEventListener('click', () => {
      document.getElementById('inference-rule-modal').classList.remove('visible');
    });
    footer.querySelector('#inf-wiz-next1').addEventListener('click', () => {
      inferenceWizardData.name = document.getElementById('inf-wiz-name').value.trim().replace(/\s+/g, '_');
      if (!inferenceWizardData.name) { showToast('Rule name is required', 'error'); return; }
      inferenceWizardStep = 2;
      renderInferenceWizardStep();
    });

  } else if (inferenceWizardStep === 2) {
    // Step 2: Conditions
    let condHtml = '';
    inferenceWizardData.conditions.forEach((cond, i) => {
      condHtml += `<div class="condition-row" style="display:flex;gap:0.5rem;align-items:center;margin-bottom:0.5rem;flex-wrap:wrap">`;
      condHtml += `<select class="input-sm inf-cond-type" data-idx="${i}" style="min-width:140px;font-size:0.8rem;padding:0.25rem 0.4rem">
        <option value="edge" ${cond.type === 'edge' ? 'selected' : ''}>Edge exists</option>
        <option value="property" ${cond.type === 'property' ? 'selected' : ''}>Property equals</option>
        <option value="confidence" ${cond.type === 'confidence' ? 'selected' : ''}>Confidence threshold</option>
      </select>`;

      if (cond.type === 'edge') {
        condHtml += `
          <input type="text" class="input-sm inf-cond-from" data-idx="${i}" placeholder="from var" value="${escapeHtml(cond.from_var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-cond-rel" data-idx="${i}" placeholder="relationship" value="${escapeHtml(cond.rel || '')}" style="width:120px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-cond-to" data-idx="${i}" placeholder="to var" value="${escapeHtml(cond.to_var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (cond.type === 'property') {
        condHtml += `
          <input type="text" class="input-sm inf-cond-node" data-idx="${i}" placeholder="node var" value="${escapeHtml(cond.node_var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-cond-key" data-idx="${i}" placeholder="key" value="${escapeHtml(cond.key || '')}" style="width:100px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-cond-val" data-idx="${i}" placeholder="value" value="${escapeHtml(cond.val || '')}" style="width:100px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (cond.type === 'confidence') {
        condHtml += `
          <input type="text" class="input-sm inf-cond-var" data-idx="${i}" placeholder="var" value="${escapeHtml(cond.var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <select class="input-sm inf-cond-op" data-idx="${i}" style="width:60px;font-size:0.8rem;padding:0.25rem 0.4rem">
            <option value=">" ${cond.op === '>' ? 'selected' : ''}>&gt;</option>
            <option value=">=" ${cond.op === '>=' ? 'selected' : ''}>&gt;=</option>
            <option value="<" ${cond.op === '<' ? 'selected' : ''}>&lt;</option>
            <option value="<=" ${cond.op === '<=' ? 'selected' : ''}>&lt;=</option>
          </select>
          <input type="number" class="input-sm inf-cond-thresh" data-idx="${i}" placeholder="0.5" value="${cond.threshold || ''}" step="0.05" min="0" max="1" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      }

      condHtml += `<button class="btn btn-ghost btn-sm remove-btn inf-cond-remove" data-idx="${i}" title="Remove condition"><i class="fa-solid fa-xmark" style="color:var(--error)"></i></button>`;
      condHtml += `</div>`;
    });

    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="active">2</span> <span>3</span> <span>4</span></div>
      <label>Conditions <span class="text-secondary">(when)</span></label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">Define what patterns to match in the knowledge graph.</p>
      <div id="inf-conditions-list">${condHtml}</div>
      <button class="btn btn-secondary btn-sm" id="inf-add-condition" style="margin-top:0.5rem">
        <i class="fa-solid fa-plus"></i> Add Condition
      </button>`;

    footer.innerHTML = `
      <button class="btn btn-secondary" id="inf-wiz-back2"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="inf-wiz-next2">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    // Save current state on type change
    body.querySelectorAll('.inf-cond-type').forEach(sel => {
      sel.addEventListener('change', () => {
        saveInferenceConditions();
        const idx = parseInt(sel.dataset.idx);
        inferenceWizardData.conditions[idx].type = sel.value;
        // Reset fields for new type
        if (sel.value === 'edge') {
          Object.assign(inferenceWizardData.conditions[idx], { from_var: 'a', rel: '', to_var: 'b' });
        } else if (sel.value === 'property') {
          Object.assign(inferenceWizardData.conditions[idx], { node_var: 'a', key: '', val: '' });
        } else if (sel.value === 'confidence') {
          Object.assign(inferenceWizardData.conditions[idx], { var: 'a', op: '>', threshold: '' });
        }
        renderInferenceWizardStep();
      });
    });

    body.querySelectorAll('.inf-cond-remove').forEach(btn => {
      btn.addEventListener('click', () => {
        saveInferenceConditions();
        inferenceWizardData.conditions.splice(parseInt(btn.dataset.idx), 1);
        renderInferenceWizardStep();
      });
    });

    document.getElementById('inf-add-condition').addEventListener('click', () => {
      saveInferenceConditions();
      inferenceWizardData.conditions.push({ type: 'edge', from_var: '', rel: '', to_var: '' });
      renderInferenceWizardStep();
    });

    footer.querySelector('#inf-wiz-back2').addEventListener('click', () => {
      saveInferenceConditions();
      inferenceWizardStep = 1;
      renderInferenceWizardStep();
    });
    footer.querySelector('#inf-wiz-next2').addEventListener('click', () => {
      saveInferenceConditions();
      if (inferenceWizardData.conditions.length === 0) { showToast('Add at least one condition', 'error'); return; }
      inferenceWizardStep = 3;
      renderInferenceWizardStep();
    });

  } else if (inferenceWizardStep === 3) {
    // Step 3: Actions
    let actHtml = '';
    inferenceWizardData.actions.forEach((act, i) => {
      actHtml += `<div class="action-row" style="display:flex;gap:0.5rem;align-items:center;margin-bottom:0.5rem;flex-wrap:wrap">`;
      actHtml += `<select class="input-sm inf-act-type" data-idx="${i}" style="min-width:130px;font-size:0.8rem;padding:0.25rem 0.4rem">
        <option value="create_edge" ${act.type === 'create_edge' ? 'selected' : ''}>Create edge</option>
        <option value="set_property" ${act.type === 'set_property' ? 'selected' : ''}>Set property</option>
        <option value="flag_review" ${act.type === 'flag_review' ? 'selected' : ''}>Flag for review</option>
      </select>`;

      if (act.type === 'create_edge') {
        actHtml += `
          <input type="text" class="input-sm inf-act-from" data-idx="${i}" placeholder="from var" value="${escapeHtml(act.from_var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-act-rel" data-idx="${i}" placeholder="relationship" value="${escapeHtml(act.rel || '')}" style="width:120px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-act-to" data-idx="${i}" placeholder="to var" value="${escapeHtml(act.to_var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <select class="input-sm inf-act-conf-mode" data-idx="${i}" style="width:80px;font-size:0.8rem;padding:0.25rem 0.4rem" title="Confidence expression">
            <option value="min" ${act.confidence_mode === 'min' ? 'selected' : ''}>min()</option>
            <option value="product" ${act.confidence_mode === 'product' ? 'selected' : ''}>product()</option>
            <option value="literal" ${act.confidence_mode === 'literal' ? 'selected' : ''}>literal</option>
          </select>`;
        if (act.confidence_mode === 'literal') {
          actHtml += `<input type="number" class="input-sm inf-act-conf-val" data-idx="${i}" placeholder="0.5" value="${act.confidence_val || ''}" step="0.05" min="0" max="1" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
        }
      } else if (act.type === 'set_property') {
        actHtml += `
          <input type="text" class="input-sm inf-act-node" data-idx="${i}" placeholder="node var" value="${escapeHtml(act.node_var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-act-key" data-idx="${i}" placeholder="key" value="${escapeHtml(act.key || '')}" style="width:100px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-act-propval" data-idx="${i}" placeholder="value" value="${escapeHtml(act.val || '')}" style="width:100px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (act.type === 'flag_review') {
        actHtml += `
          <input type="text" class="input-sm inf-act-flagvar" data-idx="${i}" placeholder="node var" value="${escapeHtml(act.node_var || '')}" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm inf-act-reason" data-idx="${i}" placeholder="reason" value="${escapeHtml(act.reason || '')}" style="width:180px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      }

      actHtml += `<button class="btn btn-ghost btn-sm remove-btn inf-act-remove" data-idx="${i}" title="Remove action"><i class="fa-solid fa-xmark" style="color:var(--error)"></i></button>`;
      actHtml += `</div>`;
    });

    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="complete">2</span> <span class="active">3</span> <span>4</span></div>
      <label>Actions <span class="text-secondary">(then)</span></label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">Define what to do when conditions match.</p>
      <div id="inf-actions-list">${actHtml}</div>
      <button class="btn btn-secondary btn-sm" id="inf-add-action" style="margin-top:0.5rem">
        <i class="fa-solid fa-plus"></i> Add Action
      </button>`;

    footer.innerHTML = `
      <button class="btn btn-secondary" id="inf-wiz-back3"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="inf-wiz-next3">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    body.querySelectorAll('.inf-act-type').forEach(sel => {
      sel.addEventListener('change', () => {
        saveInferenceActions();
        const idx = parseInt(sel.dataset.idx);
        inferenceWizardData.actions[idx].type = sel.value;
        if (sel.value === 'create_edge') {
          Object.assign(inferenceWizardData.actions[idx], { from_var: '', rel: '', to_var: '', confidence_mode: 'min', confidence_val: '' });
        } else if (sel.value === 'set_property') {
          Object.assign(inferenceWizardData.actions[idx], { node_var: '', key: '', val: '' });
        } else if (sel.value === 'flag_review') {
          Object.assign(inferenceWizardData.actions[idx], { node_var: '', reason: '' });
        }
        renderInferenceWizardStep();
      });
    });

    body.querySelectorAll('.inf-act-conf-mode').forEach(sel => {
      sel.addEventListener('change', () => {
        saveInferenceActions();
        renderInferenceWizardStep();
      });
    });

    body.querySelectorAll('.inf-act-remove').forEach(btn => {
      btn.addEventListener('click', () => {
        saveInferenceActions();
        inferenceWizardData.actions.splice(parseInt(btn.dataset.idx), 1);
        renderInferenceWizardStep();
      });
    });

    document.getElementById('inf-add-action').addEventListener('click', () => {
      saveInferenceActions();
      inferenceWizardData.actions.push({ type: 'create_edge', from_var: '', rel: '', to_var: '', confidence_mode: 'min', confidence_val: '' });
      renderInferenceWizardStep();
    });

    footer.querySelector('#inf-wiz-back3').addEventListener('click', () => {
      saveInferenceActions();
      inferenceWizardStep = 2;
      renderInferenceWizardStep();
    });
    footer.querySelector('#inf-wiz-next3').addEventListener('click', () => {
      saveInferenceActions();
      if (inferenceWizardData.actions.length === 0) { showToast('Add at least one action', 'error'); return; }
      inferenceWizardStep = 4;
      renderInferenceWizardStep();
    });

  } else if (inferenceWizardStep === 4) {
    // Step 4: Test & Save
    const ruleString = buildInferenceRuleString();

    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="complete">2</span> <span class="complete">3</span> <span class="active">4</span></div>
      <label>Generated Rule</label>
      <pre style="background:var(--bg-tertiary);padding:0.75rem;border-radius:var(--radius-sm);border:1px solid var(--border);font-size:0.8rem;white-space:pre-wrap;overflow-x:auto">${escapeHtml(ruleString)}</pre>
      <div style="margin-top:1rem">
        <button class="btn btn-secondary" id="inf-test-btn">
          <i class="fa-solid fa-flask"></i> Test Rule
        </button>
      </div>
      <div id="inf-test-results" class="mt-1"></div>`;

    footer.innerHTML = `
      <button class="btn btn-secondary" id="inf-wiz-back4"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="inf-wiz-save"><i class="fa-solid fa-check"></i> Save Rule</button>`;

    document.getElementById('inf-test-btn').addEventListener('click', async () => {
      const resultsDiv = document.getElementById('inf-test-results');
      resultsDiv.innerHTML = loadingHTML('Testing rule...');
      try {
        const result = await engram.loadRules({ rules: [ruleString], dry_run: true });
        const matches = result.matches || result.match_count || 0;
        const creates = result.creates || result.create_count || 0;
        if (matches > 0 || creates > 0) {
          resultsDiv.innerHTML = `<div class="test-results success" style="padding:0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--confidence-high);font-size:0.85rem"><i class="fa-solid fa-circle-check" style="color:var(--confidence-high)"></i> This rule would match <strong>${matches}</strong> patterns and create <strong>${creates}</strong> edges.</div>`;
        } else {
          resultsDiv.innerHTML = `<div class="test-results empty" style="padding:0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border);font-size:0.85rem"><i class="fa-solid fa-circle-info" style="color:var(--text-secondary)"></i> No matches found with current graph data. The rule is valid but has nothing to match yet.</div>`;
        }
      } catch (err) {
        resultsDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      }
    });

    footer.querySelector('#inf-wiz-back4').addEventListener('click', () => {
      inferenceWizardStep = 3;
      renderInferenceWizardStep();
    });
    footer.querySelector('#inf-wiz-save').addEventListener('click', async () => {
      try {
        await engram.loadRules({ rules: [ruleString] });
        document.getElementById('inference-rule-modal').classList.remove('visible');
        showToast('Inference rule saved', 'success');
        await loadInferenceRulesList();
      } catch (err) {
        showToast('Failed to save rule: ' + err.message, 'error');
      }
    });
  }
}

function saveInferenceConditions() {
  inferenceWizardData.conditions.forEach((cond, i) => {
    if (cond.type === 'edge') {
      const from = document.querySelector(`.inf-cond-from[data-idx="${i}"]`);
      const rel = document.querySelector(`.inf-cond-rel[data-idx="${i}"]`);
      const to = document.querySelector(`.inf-cond-to[data-idx="${i}"]`);
      if (from) cond.from_var = from.value.trim();
      if (rel) cond.rel = rel.value.trim();
      if (to) cond.to_var = to.value.trim();
    } else if (cond.type === 'property') {
      const node = document.querySelector(`.inf-cond-node[data-idx="${i}"]`);
      const key = document.querySelector(`.inf-cond-key[data-idx="${i}"]`);
      const val = document.querySelector(`.inf-cond-val[data-idx="${i}"]`);
      if (node) cond.node_var = node.value.trim();
      if (key) cond.key = key.value.trim();
      if (val) cond.val = val.value.trim();
    } else if (cond.type === 'confidence') {
      const v = document.querySelector(`.inf-cond-var[data-idx="${i}"]`);
      const op = document.querySelector(`.inf-cond-op[data-idx="${i}"]`);
      const thresh = document.querySelector(`.inf-cond-thresh[data-idx="${i}"]`);
      if (v) cond.var = v.value.trim();
      if (op) cond.op = op.value;
      if (thresh) cond.threshold = thresh.value;
    }
  });
}

function saveInferenceActions() {
  inferenceWizardData.actions.forEach((act, i) => {
    if (act.type === 'create_edge') {
      const from = document.querySelector(`.inf-act-from[data-idx="${i}"]`);
      const rel = document.querySelector(`.inf-act-rel[data-idx="${i}"]`);
      const to = document.querySelector(`.inf-act-to[data-idx="${i}"]`);
      const mode = document.querySelector(`.inf-act-conf-mode[data-idx="${i}"]`);
      const val = document.querySelector(`.inf-act-conf-val[data-idx="${i}"]`);
      if (from) act.from_var = from.value.trim();
      if (rel) act.rel = rel.value.trim();
      if (to) act.to_var = to.value.trim();
      if (mode) act.confidence_mode = mode.value;
      if (val) act.confidence_val = val.value;
    } else if (act.type === 'set_property') {
      const node = document.querySelector(`.inf-act-node[data-idx="${i}"]`);
      const key = document.querySelector(`.inf-act-key[data-idx="${i}"]`);
      const val = document.querySelector(`.inf-act-propval[data-idx="${i}"]`);
      if (node) act.node_var = node.value.trim();
      if (key) act.key = key.value.trim();
      if (val) act.val = val.value.trim();
    } else if (act.type === 'flag_review') {
      const flagvar = document.querySelector(`.inf-act-flagvar[data-idx="${i}"]`);
      const reason = document.querySelector(`.inf-act-reason[data-idx="${i}"]`);
      if (flagvar) act.node_var = flagvar.value.trim();
      if (reason) act.reason = reason.value.trim();
    }
  });
}

function buildInferenceRuleString() {
  let lines = [`rule ${inferenceWizardData.name}`];

  for (const cond of inferenceWizardData.conditions) {
    if (cond.type === 'edge') {
      lines.push(`  when edge(${cond.from_var || '?'}, "${cond.rel || ''}", ${cond.to_var || '?'})`);
    } else if (cond.type === 'property') {
      lines.push(`  when prop(${cond.node_var || '?'}, "${cond.key || ''}", "${cond.val || ''}")`);
    } else if (cond.type === 'confidence') {
      lines.push(`  when confidence(${cond.var || '?'}) ${cond.op || '>'} ${cond.threshold || '0.5'}`);
    }
  }

  for (const act of inferenceWizardData.actions) {
    if (act.type === 'create_edge') {
      let confExpr;
      if (act.confidence_mode === 'literal') {
        confExpr = act.confidence_val || '0.5';
      } else if (act.confidence_mode === 'product') {
        confExpr = `product(${inferenceWizardData.conditions.filter(c => c.type === 'edge').map(c => c.from_var).join(',')})`;
      } else {
        confExpr = `min(${inferenceWizardData.conditions.filter(c => c.type === 'edge').map(c => c.from_var).join(',')})`;
      }
      lines.push(`  then edge(${act.from_var || '?'}, "${act.rel || ''}", ${act.to_var || '?'}, confidence=${confExpr})`);
    } else if (act.type === 'set_property') {
      lines.push(`  then prop(${act.node_var || '?'}, "${act.key || ''}", "${act.val || ''}")`);
    } else if (act.type === 'flag_review') {
      lines.push(`  then flag(${act.node_var || '?'}, "${act.reason || 'needs review'}")`);
    }
  }

  return lines.join('\n');
}


// ============================================================
//  ACTION RULES - LIST + WIZARD
// ============================================================

async function loadActionRulesList() {
  const container = document.getElementById('action-rules-list');
  if (!container) return;

  try {
    const data = await engram.listActionRules();
    const rules = data.rules || [];

    if (rules.length === 0) {
      container.innerHTML = '<div class="feature-status" style="padding:0.75rem;font-size:0.85rem;color:var(--text-secondary)"><i class="fa-solid fa-circle" style="color:var(--accent-bright);font-size:0.5rem;vertical-align:middle;margin-right:0.4rem"></i>No action rules defined. Add a rule to trigger automated actions based on graph events.</div>';
      return;
    }

    let html = '<div style="display:flex;flex-direction:column;gap:0.5rem">';
    for (let i = 0; i < rules.length; i++) {
      const rule = rules[i];
      const ruleName = extractActionRuleName(rule);
      html += `
        <div style="display:flex;align-items:center;gap:0.75rem;padding:0.6rem 0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border)">
          <i class="fa-solid fa-bolt" style="color:var(--accent-bright);flex-shrink:0"></i>
          <div style="flex:1;min-width:0">
            <div style="font-weight:600;font-size:0.9rem">${escapeHtml(ruleName)}</div>
            <div style="font-size:0.75rem;color:var(--text-muted);font-family:monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">${escapeHtml(typeof rule === 'string' ? rule : JSON.stringify(rule))}</div>
          </div>
          <button class="btn btn-ghost btn-sm remove-btn action-rule-delete" data-idx="${i}" title="Delete rule">
            <i class="fa-solid fa-trash" style="color:var(--error)"></i>
          </button>
        </div>`;
    }
    html += '</div>';
    container.innerHTML = html;

    // Delete handlers
    container.querySelectorAll('.action-rule-delete').forEach(btn => {
      btn.addEventListener('click', async () => {
        if (!confirm('Delete this action rule?')) return;
        try {
          const current = await engram.listActionRules();
          const remaining = (current.rules || []).filter((_, idx) => idx !== parseInt(btn.dataset.idx));
          // Re-save remaining rules (clear + reload pattern)
          await engram.loadActionRules({ rules: remaining, replace: true });
          showToast('Action rule deleted', 'success');
          await loadActionRulesList();
        } catch (err) {
          showToast('Failed to delete rule: ' + err.message, 'error');
        }
      });
    });
  } catch (err) {
    container.innerHTML = '<div class="feature-status" style="padding:0.75rem;font-size:0.85rem;color:var(--text-secondary)"><i class="fa-solid fa-circle" style="color:var(--accent-bright);font-size:0.5rem;vertical-align:middle;margin-right:0.4rem"></i>Action rules available. Add a rule to trigger automated actions based on graph events.</div>';
  }
}

function extractActionRuleName(rule) {
  if (typeof rule === 'string') {
    const match = rule.match(/^action\s+(\S+)/);
    if (match) return match[1];
    return rule.substring(0, 40);
  }
  if (rule && rule.name) return rule.name;
  return 'Unnamed action rule';
}

function setupActionModalClose() {
  const closeBtn = document.getElementById('action-modal-close');
  if (closeBtn) closeBtn.addEventListener('click', () => {
    document.getElementById('action-rule-modal').classList.remove('visible');
  });
  const overlay = document.getElementById('action-rule-modal');
  if (overlay) overlay.addEventListener('click', (e) => {
    if (e.target === overlay) overlay.classList.remove('visible');
  });
}

function openActionRuleWizard() {
  actionWizardStep = 1;
  actionWizardData = {
    name: '',
    trigger: 'node_created',
    conditions: [{ type: 'property', node_var: 'node', key: '', val: '' }],
    actions: [{ type: 'notify', message: '' }],
  };
  renderActionWizardStep();
  document.getElementById('action-rule-modal').classList.add('visible');
}

function renderActionWizardStep() {
  const body = document.getElementById('action-wizard-body');
  const footer = document.getElementById('action-wizard-footer');

  if (actionWizardStep === 1) {
    // Step 1: Name + Trigger
    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="active">1</span> <span>2</span> <span>3</span> <span>4</span></div>
      <label>Rule Name</label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">A short identifier for this action rule (no spaces).</p>
      <input type="text" id="act-wiz-name" class="input" placeholder="e.g. alert_high_confidence" value="${escapeHtml(actionWizardData.name)}">
      <label style="margin-top:1rem">Trigger Event</label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">What graph event should activate this rule?</p>
      <select id="act-wiz-trigger" class="input">
        <option value="node_created" ${actionWizardData.trigger === 'node_created' ? 'selected' : ''}>Node created</option>
        <option value="edge_created" ${actionWizardData.trigger === 'edge_created' ? 'selected' : ''}>Edge created</option>
        <option value="confidence_changed" ${actionWizardData.trigger === 'confidence_changed' ? 'selected' : ''}>Confidence changed</option>
        <option value="node_updated" ${actionWizardData.trigger === 'node_updated' ? 'selected' : ''}>Node updated</option>
        <option value="conflict_detected" ${actionWizardData.trigger === 'conflict_detected' ? 'selected' : ''}>Conflict detected</option>
      </select>`;

    footer.innerHTML = `
      <button class="btn btn-secondary" id="act-wiz-cancel">Cancel</button>
      <button class="btn btn-primary" id="act-wiz-next1">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    footer.querySelector('#act-wiz-cancel').addEventListener('click', () => {
      document.getElementById('action-rule-modal').classList.remove('visible');
    });
    footer.querySelector('#act-wiz-next1').addEventListener('click', () => {
      actionWizardData.name = document.getElementById('act-wiz-name').value.trim().replace(/\s+/g, '_');
      actionWizardData.trigger = document.getElementById('act-wiz-trigger').value;
      if (!actionWizardData.name) { showToast('Rule name is required', 'error'); return; }
      actionWizardStep = 2;
      renderActionWizardStep();
    });

  } else if (actionWizardStep === 2) {
    // Step 2: Conditions (filters)
    let condHtml = '';
    actionWizardData.conditions.forEach((cond, i) => {
      condHtml += `<div class="condition-row" style="display:flex;gap:0.5rem;align-items:center;margin-bottom:0.5rem;flex-wrap:wrap">`;
      condHtml += `<select class="input-sm act-cond-type" data-idx="${i}" style="min-width:140px;font-size:0.8rem;padding:0.25rem 0.4rem">
        <option value="property" ${cond.type === 'property' ? 'selected' : ''}>Property equals</option>
        <option value="type_is" ${cond.type === 'type_is' ? 'selected' : ''}>Type is</option>
        <option value="confidence" ${cond.type === 'confidence' ? 'selected' : ''}>Confidence threshold</option>
        <option value="label_match" ${cond.type === 'label_match' ? 'selected' : ''}>Label contains</option>
      </select>`;

      if (cond.type === 'property') {
        condHtml += `
          <input type="text" class="input-sm act-cond-key" data-idx="${i}" placeholder="key" value="${escapeHtml(cond.key || '')}" style="width:100px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm act-cond-val" data-idx="${i}" placeholder="value" value="${escapeHtml(cond.val || '')}" style="width:120px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (cond.type === 'type_is') {
        condHtml += `
          <input type="text" class="input-sm act-cond-typeval" data-idx="${i}" placeholder="e.g. Person" value="${escapeHtml(cond.type_val || '')}" style="width:140px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (cond.type === 'confidence') {
        condHtml += `
          <select class="input-sm act-cond-op" data-idx="${i}" style="width:60px;font-size:0.8rem;padding:0.25rem 0.4rem">
            <option value=">" ${cond.op === '>' ? 'selected' : ''}>&gt;</option>
            <option value=">=" ${cond.op === '>=' ? 'selected' : ''}>&gt;=</option>
            <option value="<" ${cond.op === '<' ? 'selected' : ''}>&lt;</option>
            <option value="<=" ${cond.op === '<=' ? 'selected' : ''}>&lt;=</option>
          </select>
          <input type="number" class="input-sm act-cond-thresh" data-idx="${i}" placeholder="0.5" value="${cond.threshold || ''}" step="0.05" min="0" max="1" style="width:70px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (cond.type === 'label_match') {
        condHtml += `
          <input type="text" class="input-sm act-cond-pattern" data-idx="${i}" placeholder="substring" value="${escapeHtml(cond.pattern || '')}" style="width:160px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      }

      condHtml += `<button class="btn btn-ghost btn-sm remove-btn act-cond-remove" data-idx="${i}" title="Remove condition"><i class="fa-solid fa-xmark" style="color:var(--error)"></i></button>`;
      condHtml += `</div>`;
    });

    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="active">2</span> <span>3</span> <span>4</span></div>
      <label>Conditions <span class="text-secondary">(filter)</span></label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">Narrow which events trigger the action. Leave empty to match all events of the trigger type.</p>
      <div id="act-conditions-list">${condHtml}</div>
      <button class="btn btn-secondary btn-sm" id="act-add-condition" style="margin-top:0.5rem">
        <i class="fa-solid fa-plus"></i> Add Condition
      </button>`;

    footer.innerHTML = `
      <button class="btn btn-secondary" id="act-wiz-back2"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="act-wiz-next2">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    body.querySelectorAll('.act-cond-type').forEach(sel => {
      sel.addEventListener('change', () => {
        saveActionConditions();
        const idx = parseInt(sel.dataset.idx);
        actionWizardData.conditions[idx].type = sel.value;
        renderActionWizardStep();
      });
    });

    body.querySelectorAll('.act-cond-remove').forEach(btn => {
      btn.addEventListener('click', () => {
        saveActionConditions();
        actionWizardData.conditions.splice(parseInt(btn.dataset.idx), 1);
        renderActionWizardStep();
      });
    });

    document.getElementById('act-add-condition').addEventListener('click', () => {
      saveActionConditions();
      actionWizardData.conditions.push({ type: 'property', key: '', val: '' });
      renderActionWizardStep();
    });

    footer.querySelector('#act-wiz-back2').addEventListener('click', () => {
      saveActionConditions();
      actionWizardStep = 1;
      renderActionWizardStep();
    });
    footer.querySelector('#act-wiz-next2').addEventListener('click', () => {
      saveActionConditions();
      actionWizardStep = 3;
      renderActionWizardStep();
    });

  } else if (actionWizardStep === 3) {
    // Step 3: Actions
    let actHtml = '';
    actionWizardData.actions.forEach((act, i) => {
      actHtml += `<div class="action-row" style="display:flex;gap:0.5rem;align-items:center;margin-bottom:0.5rem;flex-wrap:wrap">`;
      actHtml += `<select class="input-sm act-act-type" data-idx="${i}" style="min-width:140px;font-size:0.8rem;padding:0.25rem 0.4rem">
        <option value="notify" ${act.type === 'notify' ? 'selected' : ''}>Send notification</option>
        <option value="create_assessment" ${act.type === 'create_assessment' ? 'selected' : ''}>Create assessment</option>
        <option value="tag_node" ${act.type === 'tag_node' ? 'selected' : ''}>Tag node</option>
        <option value="webhook" ${act.type === 'webhook' ? 'selected' : ''}>Call webhook</option>
        <option value="ingest" ${act.type === 'ingest' ? 'selected' : ''}>Trigger ingest</option>
      </select>`;

      if (act.type === 'notify') {
        actHtml += `<input type="text" class="input-sm act-act-msg" data-idx="${i}" placeholder="Notification message" value="${escapeHtml(act.message || '')}" style="flex:1;min-width:160px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (act.type === 'create_assessment') {
        actHtml += `<input type="text" class="input-sm act-act-title" data-idx="${i}" placeholder="Assessment title template" value="${escapeHtml(act.title || '')}" style="flex:1;min-width:160px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (act.type === 'tag_node') {
        actHtml += `
          <input type="text" class="input-sm act-act-tagkey" data-idx="${i}" placeholder="tag key" value="${escapeHtml(act.tag_key || '')}" style="width:100px;font-size:0.8rem;padding:0.25rem 0.4rem">
          <input type="text" class="input-sm act-act-tagval" data-idx="${i}" placeholder="tag value" value="${escapeHtml(act.tag_val || '')}" style="width:120px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (act.type === 'webhook') {
        actHtml += `<input type="text" class="input-sm act-act-url" data-idx="${i}" placeholder="https://..." value="${escapeHtml(act.url || '')}" style="flex:1;min-width:200px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      } else if (act.type === 'ingest') {
        actHtml += `<input type="text" class="input-sm act-act-source" data-idx="${i}" placeholder="Source name" value="${escapeHtml(act.source || '')}" style="width:160px;font-size:0.8rem;padding:0.25rem 0.4rem">`;
      }

      actHtml += `<button class="btn btn-ghost btn-sm remove-btn act-act-remove" data-idx="${i}" title="Remove action"><i class="fa-solid fa-xmark" style="color:var(--error)"></i></button>`;
      actHtml += `</div>`;
    });

    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="complete">2</span> <span class="active">3</span> <span>4</span></div>
      <label>Actions <span class="text-secondary">(do)</span></label>
      <p class="text-secondary" style="margin-top:0;font-size:0.85rem">What should happen when conditions match.</p>
      <div id="act-actions-list">${actHtml}</div>
      <button class="btn btn-secondary btn-sm" id="act-add-action" style="margin-top:0.5rem">
        <i class="fa-solid fa-plus"></i> Add Action
      </button>`;

    footer.innerHTML = `
      <button class="btn btn-secondary" id="act-wiz-back3"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="act-wiz-next3">Next <i class="fa-solid fa-arrow-right"></i></button>`;

    body.querySelectorAll('.act-act-type').forEach(sel => {
      sel.addEventListener('change', () => {
        saveActionActions();
        const idx = parseInt(sel.dataset.idx);
        actionWizardData.actions[idx] = { type: sel.value };
        renderActionWizardStep();
      });
    });

    body.querySelectorAll('.act-act-remove').forEach(btn => {
      btn.addEventListener('click', () => {
        saveActionActions();
        actionWizardData.actions.splice(parseInt(btn.dataset.idx), 1);
        renderActionWizardStep();
      });
    });

    document.getElementById('act-add-action').addEventListener('click', () => {
      saveActionActions();
      actionWizardData.actions.push({ type: 'notify', message: '' });
      renderActionWizardStep();
    });

    footer.querySelector('#act-wiz-back3').addEventListener('click', () => {
      saveActionActions();
      actionWizardStep = 2;
      renderActionWizardStep();
    });
    footer.querySelector('#act-wiz-next3').addEventListener('click', () => {
      saveActionActions();
      if (actionWizardData.actions.length === 0) { showToast('Add at least one action', 'error'); return; }
      actionWizardStep = 4;
      renderActionWizardStep();
    });

  } else if (actionWizardStep === 4) {
    // Step 4: Test & Save
    const ruleObj = buildActionRuleObject();
    const ruleJson = JSON.stringify(ruleObj, null, 2);

    body.innerHTML = `
      <div class="wizard-step-indicator"><span class="complete">1</span> <span class="complete">2</span> <span class="complete">3</span> <span class="active">4</span></div>
      <label>Generated Action Rule</label>
      <pre style="background:var(--bg-tertiary);padding:0.75rem;border-radius:var(--radius-sm);border:1px solid var(--border);font-size:0.8rem;white-space:pre-wrap;overflow-x:auto">${escapeHtml(ruleJson)}</pre>
      <div style="margin-top:1rem">
        <button class="btn btn-secondary" id="act-test-btn">
          <i class="fa-solid fa-flask"></i> Dry Run
        </button>
      </div>
      <div id="act-test-results" class="mt-1"></div>`;

    footer.innerHTML = `
      <button class="btn btn-secondary" id="act-wiz-back4"><i class="fa-solid fa-arrow-left"></i> Back</button>
      <button class="btn btn-primary" id="act-wiz-save"><i class="fa-solid fa-check"></i> Save Rule</button>`;

    document.getElementById('act-test-btn').addEventListener('click', async () => {
      const resultsDiv = document.getElementById('act-test-results');
      resultsDiv.innerHTML = loadingHTML('Running dry run...');
      try {
        const result = await engram.dryRunAction(ruleObj);
        const matches = result.matches || result.match_count || 0;
        if (matches > 0) {
          resultsDiv.innerHTML = `<div class="test-results success" style="padding:0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--confidence-high);font-size:0.85rem"><i class="fa-solid fa-circle-check" style="color:var(--confidence-high)"></i> This rule would trigger on <strong>${matches}</strong> existing events.</div>`;
        } else {
          resultsDiv.innerHTML = `<div class="test-results empty" style="padding:0.75rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border);font-size:0.85rem"><i class="fa-solid fa-circle-info" style="color:var(--text-secondary)"></i> No matching events found. The rule will activate on future events matching the trigger.</div>`;
        }
      } catch (err) {
        resultsDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      }
    });

    footer.querySelector('#act-wiz-back4').addEventListener('click', () => {
      actionWizardStep = 3;
      renderActionWizardStep();
    });
    footer.querySelector('#act-wiz-save').addEventListener('click', async () => {
      try {
        await engram.loadActionRules({ rules: [ruleObj] });
        document.getElementById('action-rule-modal').classList.remove('visible');
        showToast('Action rule saved', 'success');
        await loadActionRulesList();
      } catch (err) {
        showToast('Failed to save rule: ' + err.message, 'error');
      }
    });
  }
}

function saveActionConditions() {
  actionWizardData.conditions.forEach((cond, i) => {
    if (cond.type === 'property') {
      const key = document.querySelector(`.act-cond-key[data-idx="${i}"]`);
      const val = document.querySelector(`.act-cond-val[data-idx="${i}"]`);
      if (key) cond.key = key.value.trim();
      if (val) cond.val = val.value.trim();
    } else if (cond.type === 'type_is') {
      const tv = document.querySelector(`.act-cond-typeval[data-idx="${i}"]`);
      if (tv) cond.type_val = tv.value.trim();
    } else if (cond.type === 'confidence') {
      const op = document.querySelector(`.act-cond-op[data-idx="${i}"]`);
      const thresh = document.querySelector(`.act-cond-thresh[data-idx="${i}"]`);
      if (op) cond.op = op.value;
      if (thresh) cond.threshold = thresh.value;
    } else if (cond.type === 'label_match') {
      const pat = document.querySelector(`.act-cond-pattern[data-idx="${i}"]`);
      if (pat) cond.pattern = pat.value.trim();
    }
  });
}

function saveActionActions() {
  actionWizardData.actions.forEach((act, i) => {
    if (act.type === 'notify') {
      const msg = document.querySelector(`.act-act-msg[data-idx="${i}"]`);
      if (msg) act.message = msg.value.trim();
    } else if (act.type === 'create_assessment') {
      const title = document.querySelector(`.act-act-title[data-idx="${i}"]`);
      if (title) act.title = title.value.trim();
    } else if (act.type === 'tag_node') {
      const tk = document.querySelector(`.act-act-tagkey[data-idx="${i}"]`);
      const tv = document.querySelector(`.act-act-tagval[data-idx="${i}"]`);
      if (tk) act.tag_key = tk.value.trim();
      if (tv) act.tag_val = tv.value.trim();
    } else if (act.type === 'webhook') {
      const url = document.querySelector(`.act-act-url[data-idx="${i}"]`);
      if (url) act.url = url.value.trim();
    } else if (act.type === 'ingest') {
      const src = document.querySelector(`.act-act-source[data-idx="${i}"]`);
      if (src) act.source = src.value.trim();
    }
  });
}

function buildActionRuleObject() {
  const obj = {
    name: actionWizardData.name,
    trigger: actionWizardData.trigger,
    conditions: actionWizardData.conditions.map(c => {
      if (c.type === 'property') return { type: 'property', key: c.key, value: c.val };
      if (c.type === 'type_is') return { type: 'type_is', value: c.type_val };
      if (c.type === 'confidence') return { type: 'confidence', op: c.op || '>', threshold: parseFloat(c.threshold) || 0.5 };
      if (c.type === 'label_match') return { type: 'label_match', pattern: c.pattern };
      return c;
    }),
    actions: actionWizardData.actions.map(a => {
      if (a.type === 'notify') return { type: 'notify', message: a.message };
      if (a.type === 'create_assessment') return { type: 'create_assessment', title: a.title };
      if (a.type === 'tag_node') return { type: 'tag_node', key: a.tag_key, value: a.tag_val };
      if (a.type === 'webhook') return { type: 'webhook', url: a.url };
      if (a.type === 'ingest') return { type: 'ingest', source: a.source };
      return a;
    }),
  };
  return obj;
}
