/* ============================================
   engram - Search View
   ============================================ */

router.register('/search', () => {
  renderTo(`
    <div class="view-header">
      <h1><i class="fa-solid fa-magnifying-glass"></i> Search</h1>
    </div>
    <div class="search-layout">
      <div class="search-filters">
        <div class="card">
          <div class="card-header"><h3><i class="fa-solid fa-filter"></i> Filters</h3></div>

          <div class="filter-section">
            <h4>Confidence Range</h4>
            <div class="slider-group">
              <label>Min <span id="search-conf-min-val">0.0</span></label>
              <input type="range" id="search-conf-min" min="0" max="100" value="0">
            </div>
            <div class="slider-group">
              <label>Max <span id="search-conf-max-val">1.0</span></label>
              <input type="range" id="search-conf-max" min="0" max="100" value="100">
            </div>
          </div>

          <div class="filter-section">
            <h4>Type Filter</h4>
            <input type="text" id="search-type-filter" placeholder="e.g. person, concept...">
          </div>

          <div class="filter-section">
            <h4>Memory Tier</h4>
            <div class="checkbox-group">
              <label><input type="checkbox" id="tier-core" checked> Core (0.8+)</label>
              <label><input type="checkbox" id="tier-active" checked> Active (0.4-0.8)</label>
              <label><input type="checkbox" id="tier-archival" checked> Archival (&lt;0.4)</label>
            </div>
          </div>

          <div class="filter-section">
            <h4>Property Filter</h4>
            <div class="form-row mb-1">
              <div class="form-group" style="margin-bottom:0">
                <input type="text" id="prop-filter-key" placeholder="key">
              </div>
              <div class="form-group" style="margin-bottom:0">
                <input type="text" id="prop-filter-value" placeholder="value">
              </div>
            </div>
          </div>

          <button class="btn btn-primary w-100" id="search-apply">
            <i class="fa-solid fa-magnifying-glass"></i> Apply Filters
          </button>
        </div>
      </div>

      <div class="search-results">
        <div class="search-bar">
          <i class="fa-solid fa-magnifying-glass search-icon"></i>
          <input type="text" id="search-input" placeholder="Search entities..." autofocus>
        </div>
        <div class="card">
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  <th class="clickable" data-sort="label">Label <i class="fa-solid fa-sort"></i></th>
                  <th class="clickable" data-sort="type">Type <i class="fa-solid fa-sort"></i></th>
                  <th class="clickable" data-sort="confidence">Confidence <i class="fa-solid fa-sort"></i></th>
                  <th class="clickable" data-sort="score">Score <i class="fa-solid fa-sort"></i></th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody id="search-results-body">
                <tr><td colspan="5">${emptyStateHTML('fa-magnifying-glass', 'Enter a search query to find entities')}</td></tr>
              </tbody>
            </table>
          </div>
          <div id="search-status" class="text-muted mt-1" style="font-size:0.85rem"></div>
        </div>
      </div>
    </div>
  `);

  setupSearchEvents();

  // Check for query param
  const params = new URLSearchParams(location.hash.split('?')[1] || '');
  const q = params.get('q');
  if (q) {
    document.getElementById('search-input').value = q;
    doSearch(q);
  }
});

let searchResults = [];
let searchSortField = 'score';
let searchSortDir = -1;

function setupSearchEvents() {
  const input = document.getElementById('search-input');
  let debounce = null;

  input.addEventListener('input', () => {
    clearTimeout(debounce);
    debounce = setTimeout(() => {
      const q = input.value.trim();
      if (q.length >= 2) doSearch(q);
    }, 400);
  });

  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      const q = input.value.trim();
      if (q) doSearch(q);
    }
  });

  document.getElementById('search-apply').addEventListener('click', () => {
    const q = input.value.trim();
    if (q) doSearch(q);
  });

  // Slider labels
  document.getElementById('search-conf-min').addEventListener('input', (e) => {
    document.getElementById('search-conf-min-val').textContent = (e.target.value / 100).toFixed(2);
  });
  document.getElementById('search-conf-max').addEventListener('input', (e) => {
    document.getElementById('search-conf-max-val').textContent = (e.target.value / 100).toFixed(2);
  });

  // Sortable headers
  document.querySelectorAll('th[data-sort]').forEach(th => {
    th.addEventListener('click', () => {
      const field = th.getAttribute('data-sort');
      if (searchSortField === field) {
        searchSortDir *= -1;
      } else {
        searchSortField = field;
        searchSortDir = -1;
      }
      renderSearchResults();
    });
  });
}

async function doSearch(query) {
  const tbody = document.getElementById('search-results-body');
  const status = document.getElementById('search-status');
  tbody.innerHTML = `<tr><td colspan="5">${loadingHTML('Searching...')}</td></tr>`;

  try {
    const data = await engram.search({ query, limit: 50 });
    searchResults = (data.results || []).map(r => ({
      ...r,
      type: r.type || '',
    }));

    // Apply client-side filters
    const confMin = parseInt(document.getElementById('search-conf-min').value) / 100;
    const confMax = parseInt(document.getElementById('search-conf-max').value) / 100;
    const typeFilter = document.getElementById('search-type-filter').value.trim().toLowerCase();
    const tierCore = document.getElementById('tier-core').checked;
    const tierActive = document.getElementById('tier-active').checked;
    const tierArchival = document.getElementById('tier-archival').checked;

    searchResults = searchResults.filter(r => {
      const c = r.confidence ?? 0;
      if (c < confMin || c > confMax) return false;
      if (typeFilter && !(r.type || '').toLowerCase().includes(typeFilter)) return false;
      // Tier filter
      if (c >= 0.8 && !tierCore) return false;
      if (c >= 0.4 && c < 0.8 && !tierActive) return false;
      if (c < 0.4 && !tierArchival) return false;
      return true;
    });

    status.textContent = `${searchResults.length} result${searchResults.length !== 1 ? 's' : ''} found`;
    renderSearchResults();
  } catch (err) {
    tbody.innerHTML = `<tr><td colspan="5">${emptyStateHTML('fa-circle-exclamation', 'Search failed: ' + escapeHtml(err.message))}</td></tr>`;
    status.textContent = '';
  }
}

function renderSearchResults() {
  const tbody = document.getElementById('search-results-body');

  if (searchResults.length === 0) {
    tbody.innerHTML = `<tr><td colspan="5">${emptyStateHTML('fa-magnifying-glass', 'No results match the current filters')}</td></tr>`;
    return;
  }

  const sorted = [...searchResults].sort((a, b) => {
    const av = a[searchSortField] ?? '';
    const bv = b[searchSortField] ?? '';
    if (typeof av === 'number' && typeof bv === 'number') return (av - bv) * searchSortDir;
    return String(av).localeCompare(String(bv)) * searchSortDir;
  });

  tbody.innerHTML = sorted.map(r => {
    const conf = r.confidence ?? 0;
    const color = confidenceColor(conf);
    return `
      <tr class="result-row" data-label="${escapeHtml(r.label)}">
        <td><strong>${escapeHtml(r.label)}</strong></td>
        <td class="text-secondary">${escapeHtml(r.type || '--')}</td>
        <td>
          <span style="color:${color};font-weight:600">${(conf * 100).toFixed(0)}%</span>
          ${tierBadge(conf)}
        </td>
        <td>${r.score != null ? r.score.toFixed(2) : '--'}</td>
        <td>
          <a href="#/node/${encodeURIComponent(r.label)}" class="btn btn-sm btn-secondary" title="View Details">
            <i class="fa-solid fa-eye"></i>
          </a>
          <a href="#/graph?node=${encodeURIComponent(r.label)}" class="btn btn-sm btn-primary" title="Explore in Graph">
            <i class="fa-solid fa-diagram-project"></i>
          </a>
        </td>
      </tr>`;
  }).join('');

  // Click row to navigate
  tbody.querySelectorAll('.result-row').forEach(row => {
    row.addEventListener('click', (e) => {
      if (e.target.closest('a, button')) return;
      const label = row.getAttribute('data-label');
      location.hash = `#/node/${encodeURIComponent(label)}`;
    });
  });
}
