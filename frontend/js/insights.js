/* ============================================
   engram - Insights View
   Knowledge intelligence and gap analysis
   ============================================ */

// Pagination state for gaps
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

  // --- Knowledge Health summary (always shown) ---
  let stats = null;
  try {
    stats = await engram.stats();
  } catch (_) {}

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

  // --- Knowledge Gaps (with timeout to avoid hanging page) ---
  let gapsAvailable = false;
  let gapsData = null;
  try {
    const gapsPromise = engram._fetch('/reason/gaps');
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000));
    gapsData = await Promise.race([gapsPromise, timeout]);
    gapsAvailable = true;
  } catch (_) {}

  // --- Frontier ---
  let frontierData = null;
  try {
    const frontierPromise = engram._fetch('/reason/frontier');
    const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000));
    frontierData = await Promise.race([frontierPromise, timeout]);
  } catch (_) {}

  if (gapsAvailable) {
    insightsGapsAll = Array.isArray(gapsData) ? gapsData : (gapsData && gapsData.gaps ? gapsData.gaps : []);
    insightsGapPage = 0;

    // Gaps card
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

    // Frontier
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

    // Full Analysis button
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

    // Recommended Actions
    html += `
      <div class="card">
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

  } else {
    // Reason feature not available
    html += `
      <div class="card" style="margin-bottom:1.5rem">
        <div class="card-header">
          <h3><i class="fa-solid fa-circle-info"></i> Intelligence Features</h3>
        </div>
        <p style="font-size:0.95rem;color:var(--text-secondary);margin-bottom:1rem">
          Intelligence features help identify what your knowledge base is missing.
          Enable the reasoning module for automatic gap detection, conflict alerts, and investigation suggestions.
        </p>
        <div class="grid-2" style="gap:0.75rem">
          ${insightCapabilityCard('fa-triangle-exclamation', 'Gap Detection', 'Find missing relationships and incomplete knowledge areas.')}
          ${insightCapabilityCard('fa-bolt', 'Conflict Alerts', 'Detect contradictions between facts in your knowledge base.')}
          ${insightCapabilityCard('fa-lightbulb', 'Investigation Suggestions', 'Get recommended next steps to strengthen your knowledge.')}
          ${insightCapabilityCard('fa-border-none', 'Frontier Analysis', 'Identify facts that need more connections or context.')}
        </div>
        <p class="text-muted mt-2" style="font-size:0.85rem">
          <i class="fa-solid fa-info-circle"></i> Reasoning features are not available in this build.
        </p>
      </div>`;
  }

  container.innerHTML = html;

  // Render paginated gaps if available
  if (gapsAvailable) {
    renderGapsPage();
    setupResolveButton();
  }

  // Setup event handlers
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
}

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

    // Show description if it differs from the title
    if (description && description !== title) {
      html += `<div style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.3rem">${escapeHtml(description)}</div>`;
    }

    // Affected entities
    if (gap.entities && gap.entities.length > 0) {
      html += `
        <div style="font-size:0.85rem;color:var(--text-secondary);margin-bottom:0.3rem">
          <i class="fa-solid fa-diagram-project"></i> Affected: ${gap.entities.map(e => '<a href="#/node/' + encodeURIComponent(e) + '">' + escapeHtml(e) + '</a>').join(', ')}
        </div>`;
    }

    // Single suggestion field (legacy)
    if (gap.suggestion) {
      html += `<div style="font-size:0.85rem;color:var(--accent-bright)"><i class="fa-solid fa-lightbulb"></i> ${escapeHtml(gap.suggestion)}</div>`;
    }

    // Suggested queries
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
      const totalPages = Math.ceil(insightsGapsAll.length / GAPS_PER_PAGE);
      if (insightsGapPage < totalPages - 1) { insightsGapPage++; renderGapsPage(); setupResolveButton(); }
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

  // Individual checkbox change -> update resolve button
  const boxes = area.querySelectorAll('.gap-checkbox');
  boxes.forEach(cb => {
    cb.addEventListener('change', () => {
      updateResolveButtonVisibility();
      // Sync select-all state
      if (selectAll) {
        const allChecked = Array.from(boxes).every(b => b.checked);
        selectAll.checked = allChecked;
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

  // Remove old listener by replacing node
  const newBtn = resolveBtn.cloneNode(true);
  resolveBtn.parentNode.replaceChild(newBtn, resolveBtn);
  newBtn.style.display = 'none';
  updateResolveButtonVisibility();

  newBtn.addEventListener('click', () => {
    const checked = document.querySelectorAll('.gap-checkbox:checked');
    const indices = Array.from(checked).map(cb => parseInt(cb.dataset.idx, 10));

    // Remove selected gaps (highest index first to preserve ordering)
    indices.sort((a, b) => b - a);
    for (const idx of indices) {
      insightsGapsAll.splice(idx, 1);
    }

    // Adjust page if current page is now out of range
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
