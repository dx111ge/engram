/* ============================================
   engram - Natural Language View
   ============================================ */

router.register('/nl', () => {
  renderTo(`
    <div class="view-header">
      <h1><i class="fa-solid fa-comments"></i> Natural Language</h1>
    </div>
    <div class="nl-layout">
      <div class="nl-panel">
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-circle-question"></i> Ask</h3></div>
          <div class="form-group">
            <label>Question</label>
            <input type="text" id="ask-input" placeholder="What is X? How are A and B related?">
          </div>
          <button class="btn btn-primary" id="btn-ask">
            <i class="fa-solid fa-paper-plane"></i> Ask
          </button>
          <div class="nl-history" id="ask-history">
            <div class="empty-state" style="padding:1.5rem">
              <i class="fa-solid fa-circle-question" style="font-size:1.5rem"></i>
              <p>Ask questions about the knowledge graph</p>
            </div>
          </div>
        </div>
      </div>
      <div class="nl-panel">
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-comment-dots"></i> Tell</h3></div>
          <div class="form-group">
            <label>Statement</label>
            <input type="text" id="tell-input" placeholder="X is a database. Y works at Z.">
          </div>
          <div class="form-group">
            <label>Source (optional)</label>
            <input type="text" id="tell-source" placeholder="Where did this fact come from?">
          </div>
          <button class="btn btn-primary" id="btn-tell">
            <i class="fa-solid fa-paper-plane"></i> Tell
          </button>
          <div class="nl-history" id="tell-history">
            <div class="empty-state" style="padding:1.5rem">
              <i class="fa-solid fa-comment-dots" style="font-size:1.5rem"></i>
              <p>Teach the knowledge graph new facts</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  `);

  setupNLEvents();
});

function setupNLEvents() {
  const askInput = document.getElementById('ask-input');
  const tellInput = document.getElementById('tell-input');
  const askBtn = document.getElementById('btn-ask');
  const tellBtn = document.getElementById('btn-tell');

  askBtn.addEventListener('click', () => doAsk());
  askInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') doAsk(); });

  tellBtn.addEventListener('click', () => doTell());
  tellInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') doTell(); });
}

async function doAsk() {
  const input = document.getElementById('ask-input');
  const question = input.value.trim();
  if (!question) return;

  const history = document.getElementById('ask-history');
  // Clear empty state
  if (history.querySelector('.empty-state')) history.innerHTML = '';

  const entry = document.createElement('div');
  entry.className = 'nl-entry';
  entry.innerHTML = `
    <div class="nl-query"><i class="fa-solid fa-circle-question"></i> ${escapeHtml(question)}</div>
    <div><span class="spinner"></span> Thinking...</div>
  `;
  history.prepend(entry);
  input.value = '';

  try {
    const result = await engram.ask({ question });
    let resultsHTML = '';

    if (result.results && result.results.length > 0) {
      resultsHTML = result.results.map(r => {
        const parts = [];
        if (r.label) parts.push(`<strong>${escapeHtml(r.label)}</strong>`);
        if (r.relationship) parts.push(`<span class="edge-rel">${escapeHtml(r.relationship)}</span>`);
        if (r.detail) parts.push(escapeHtml(r.detail));
        if (r.confidence != null) parts.push(`<span style="color:${confidenceColor(r.confidence)}">${(r.confidence * 100).toFixed(0)}%</span>`);
        return `<div style="padding:0.2rem 0">${parts.join(' ')}</div>`;
      }).join('');
    } else {
      resultsHTML = '<div class="text-muted">No results found.</div>';
    }

    entry.innerHTML = `
      <div class="nl-query"><i class="fa-solid fa-circle-question"></i> ${escapeHtml(question)}</div>
      ${result.interpretation ? `<div class="nl-interpretation"><i class="fa-solid fa-lightbulb"></i> ${escapeHtml(result.interpretation)}</div>` : ''}
      <div>${resultsHTML}</div>
    `;
  } catch (err) {
    entry.innerHTML = `
      <div class="nl-query"><i class="fa-solid fa-circle-question"></i> ${escapeHtml(question)}</div>
      <div style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>
    `;
  }
}

async function doTell() {
  const input = document.getElementById('tell-input');
  const sourceInput = document.getElementById('tell-source');
  const statement = input.value.trim();
  if (!statement) return;
  const source = sourceInput.value.trim() || undefined;

  const history = document.getElementById('tell-history');
  if (history.querySelector('.empty-state')) history.innerHTML = '';

  const entry = document.createElement('div');
  entry.className = 'nl-entry';
  entry.innerHTML = `
    <div class="nl-query"><i class="fa-solid fa-comment-dots"></i> ${escapeHtml(statement)}</div>
    <div><span class="spinner"></span> Processing...</div>
  `;
  history.prepend(entry);
  input.value = '';

  try {
    const result = await engram.tell({ statement, source });
    let actionsHTML = '';

    if (result.actions && result.actions.length > 0) {
      actionsHTML = result.actions.map(a =>
        `<div style="padding:0.15rem 0"><i class="fa-solid fa-check" style="color:var(--success);margin-right:0.3rem"></i>${escapeHtml(a)}</div>`
      ).join('');
    } else {
      actionsHTML = '<div class="text-muted">No actions taken.</div>';
    }

    entry.innerHTML = `
      <div class="nl-query"><i class="fa-solid fa-comment-dots"></i> ${escapeHtml(statement)}</div>
      ${result.interpretation ? `<div class="nl-interpretation"><i class="fa-solid fa-lightbulb"></i> ${escapeHtml(result.interpretation)}</div>` : ''}
      <div>${actionsHTML}</div>
    `;
    showToast('Fact recorded', 'success');
  } catch (err) {
    entry.innerHTML = `
      <div class="nl-query"><i class="fa-solid fa-comment-dots"></i> ${escapeHtml(statement)}</div>
      <div style="color:var(--error)"><i class="fa-solid fa-circle-exclamation"></i> ${escapeHtml(err.message)}</div>
    `;
  }
}
