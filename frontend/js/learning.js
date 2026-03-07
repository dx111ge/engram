/* ============================================
   engram - Learning View
   ============================================ */

router.register('/learning', () => {
  renderTo(`
    <div class="view-header">
      <h1><i class="fa-solid fa-graduation-cap"></i> Learning</h1>
    </div>
    <div class="learning-grid">
      <!-- Decay -->
      <div class="card">
        <div class="card-header"><h3><i class="fa-solid fa-clock-rotate-left"></i> Memory Decay</h3></div>
        <p class="text-secondary mb-2" style="font-size:0.9rem">
          Trigger a decay cycle to reduce confidence of unreinforced nodes. This simulates natural memory decay
          and helps keep the knowledge graph focused on frequently accessed information.
        </p>
        <button class="btn btn-danger" id="btn-decay">
          <i class="fa-solid fa-hourglass-half"></i> Trigger Decay Cycle
        </button>
        <div id="decay-result" class="mt-1"></div>
      </div>

      <!-- Inference Rules -->
      <div class="card">
        <div class="card-header"><h3><i class="fa-solid fa-wand-magic-sparkles"></i> Inference Rules</h3></div>
        <div class="form-group">
          <label>Rule Definitions</label>
          <textarea id="rules-input" rows="8" placeholder="Enter inference rules, one per line.

Example:
IF ?x is_a ?y AND ?y is_a ?z THEN ?x is_a ?z
IF ?x uses ?y AND ?y is_a database THEN ?x has_database ?y"></textarea>
        </div>
        <button class="btn btn-primary" id="btn-derive">
          <i class="fa-solid fa-play"></i> Derive
        </button>
        <div id="derive-result" class="mt-1"></div>
      </div>

      <!-- Explain -->
      <div class="card" style="grid-column: 1 / -1">
        <div class="card-header"><h3><i class="fa-solid fa-magnifying-glass-chart"></i> Explain Provenance</h3></div>
        <div class="form-row mb-1">
          <div class="form-group" style="margin-bottom:0">
            <input type="text" id="explain-label" placeholder="Enter node label to explain...">
          </div>
          <button class="btn btn-primary" id="btn-explain">
            <i class="fa-solid fa-search"></i> Explain
          </button>
        </div>
        <div id="explain-result"></div>
      </div>
    </div>
  `);

  setupLearningEvents();
});

function setupLearningEvents() {
  // Decay
  document.getElementById('btn-decay').addEventListener('click', async () => {
    if (!confirm('Are you sure you want to trigger a decay cycle? This will reduce confidence of unreinforced nodes.')) return;

    const btn = document.getElementById('btn-decay');
    const resultDiv = document.getElementById('decay-result');
    btn.disabled = true;
    resultDiv.innerHTML = `<span class="spinner"></span> Running decay cycle...`;

    try {
      const result = await engram.decay();
      resultDiv.innerHTML = `
        <div style="color:var(--success);font-size:0.85rem">
          <i class="fa-solid fa-check"></i> Decay cycle complete.
          ${result.decayed != null ? `${result.decayed} nodes decayed.` : ''}
          ${result.pruned != null ? `${result.pruned} nodes pruned.` : ''}
        </div>`;
      showToast('Decay cycle complete', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast(`Decay failed: ${err.message}`, 'error');
    } finally {
      btn.disabled = false;
    }
  });

  // Derive
  document.getElementById('btn-derive').addEventListener('click', async () => {
    const rulesText = document.getElementById('rules-input').value.trim();
    if (!rulesText) { showToast('Please enter at least one rule', 'error'); return; }

    const rules = rulesText.split('\n').map(r => r.trim()).filter(r => r.length > 0);
    const resultDiv = document.getElementById('derive-result');
    resultDiv.innerHTML = `<span class="spinner"></span> Running ${rules.length} rules...`;

    try {
      const result = await engram.derive({ rules });
      let html = '<div class="rule-results">';

      if (result.evaluated != null) html += `<div>Rules evaluated: <strong>${result.evaluated}</strong></div>`;
      if (result.fired != null) html += `<div>Rules fired: <strong>${result.fired}</strong></div>`;
      if (result.edges_created != null) html += `<div>Edges created: <strong>${result.edges_created}</strong></div>`;
      if (result.flags != null && result.flags.length > 0) {
        html += `<div style="margin-top:0.5rem;color:var(--warning)"><strong>Flags:</strong></div>`;
        result.flags.forEach(f => { html += `<div>  - ${escapeHtml(f)}</div>`; });
      }

      // Show raw result if specific fields not present
      if (result.evaluated == null && result.fired == null) {
        html += `<pre>${escapeHtml(JSON.stringify(result, null, 2))}</pre>`;
      }

      html += '</div>';
      resultDiv.innerHTML = html;
      showToast('Derivation complete', 'success');
    } catch (err) {
      resultDiv.innerHTML = `<div style="color:var(--error);font-size:0.85rem"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
      showToast(`Derive failed: ${err.message}`, 'error');
    }
  });

  // Explain
  const explainInput = document.getElementById('explain-label');
  document.getElementById('btn-explain').addEventListener('click', doExplain);
  explainInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') doExplain(); });
}

async function doExplain() {
  const label = document.getElementById('explain-label').value.trim();
  if (!label) { showToast('Please enter a node label', 'error'); return; }

  const resultDiv = document.getElementById('explain-result');
  resultDiv.innerHTML = loadingHTML('Loading provenance...');

  try {
    const data = await engram.explain(label);
    let html = '<div class="rule-results">';
    html += `<pre>${escapeHtml(JSON.stringify(data, null, 2))}</pre>`;
    html += '</div>';

    // Try to render a nicer view if we can detect structure
    if (data.label) {
      html = `
        <div class="card mt-1" style="background:var(--bg-input)">
          <h4 style="margin-bottom:0.5rem">${escapeHtml(data.label)}</h4>
          ${data.confidence != null ? confidenceBar(data.confidence) : ''}
          ${data.sources ? `<div class="mt-1"><strong>Sources:</strong> ${data.sources.map(s => escapeHtml(s)).join(', ')}</div>` : ''}
          ${data.co_occurrences && data.co_occurrences.length > 0
            ? `<div class="mt-1"><strong>Co-occurrences:</strong><ul class="edge-list">${data.co_occurrences.map(c =>
                `<li><i class="fa-solid fa-link"></i> <a href="#/node/${encodeURIComponent(c.label || c)}">${escapeHtml(c.label || c)}</a>
                ${c.count ? `<span class="text-muted">(${c.count}x)</span>` : ''}</li>`
              ).join('')}</ul></div>`
            : ''}
          <details class="mt-1" style="font-size:0.85rem">
            <summary class="text-muted" style="cursor:pointer">Raw JSON</summary>
            <pre style="margin-top:0.5rem;overflow-x:auto">${escapeHtml(JSON.stringify(data, null, 2))}</pre>
          </details>
        </div>`;
    }

    resultDiv.innerHTML = html;
  } catch (err) {
    resultDiv.innerHTML = `<div style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>`;
  }
}
