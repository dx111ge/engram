/* ============================================
   engram - Dashboard View
   ============================================ */

router.register('/', async () => {
  renderTo(`
    <div class="dashboard-search">
      <i class="fa-solid fa-magnifying-glass search-icon"></i>
      <input type="text" id="dash-search" placeholder="Search the knowledge graph..." autofocus>
    </div>

    <div class="grid-3 mb-2" id="dash-stats">
      <div class="card stat-card">
        <div class="stat-value" id="stat-nodes">--</div>
        <div class="stat-label"><i class="fa-solid fa-circle-nodes"></i> Nodes</div>
      </div>
      <div class="card stat-card">
        <div class="stat-value" id="stat-edges">--</div>
        <div class="stat-label"><i class="fa-solid fa-arrow-right-arrow-left"></i> Edges</div>
      </div>
      <div class="card stat-card">
        <div class="stat-value" id="stat-health">--</div>
        <div class="stat-label"><i class="fa-solid fa-heart-pulse"></i> Status</div>
      </div>
    </div>

    <div class="card mb-2">
      <div class="card-header">
        <h3><i class="fa-solid fa-bolt"></i> Quick Actions</h3>
      </div>
      <div class="quick-actions">
        <button class="quick-action-btn" onclick="location.hash='#/import'">
          <i class="fa-solid fa-plus-circle"></i>
          <div><strong>Store Entity</strong><br><small class="text-muted">Add a new node to the graph</small></div>
        </button>
        <button class="quick-action-btn" onclick="location.hash='#/import'">
          <i class="fa-solid fa-link"></i>
          <div><strong>Create Relationship</strong><br><small class="text-muted">Connect two entities</small></div>
        </button>
        <button class="quick-action-btn" onclick="location.hash='#/nl'">
          <i class="fa-solid fa-circle-question"></i>
          <div><strong>Ask Question</strong><br><small class="text-muted">Query in natural language</small></div>
        </button>
        <button class="quick-action-btn" onclick="location.hash='#/nl'">
          <i class="fa-solid fa-comment-dots"></i>
          <div><strong>Tell Fact</strong><br><small class="text-muted">Teach the knowledge graph</small></div>
        </button>
      </div>
    </div>

    <div class="card">
      <div class="card-header">
        <h3><i class="fa-solid fa-diagram-project"></i> Explore Graph</h3>
      </div>
      <p class="text-secondary">Use the <a href="#/graph">Graph Explorer</a> to visualize and navigate the knowledge graph, or <a href="#/search">Search</a> for specific entities.</p>
    </div>
  `);

  // Search handler
  document.getElementById('dash-search').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      const q = e.target.value.trim();
      if (q) location.hash = `#/search?q=${encodeURIComponent(q)}`;
    }
  });

  // Load stats
  try {
    const [stats, health] = await Promise.all([
      engram.stats().catch(() => null),
      engram.health().catch(() => null),
    ]);
    if (stats) {
      document.getElementById('stat-nodes').textContent = stats.nodes ?? 0;
      document.getElementById('stat-edges').textContent = stats.edges ?? 0;
    } else {
      document.getElementById('stat-nodes').textContent = '?';
      document.getElementById('stat-edges').textContent = '?';
    }
    if (health) {
      document.getElementById('stat-health').textContent = health.status === 'ok' ? 'Online' : 'Error';
      document.getElementById('stat-health').style.color = health.status === 'ok' ? 'var(--confidence-high)' : 'var(--error)';
    } else {
      document.getElementById('stat-health').textContent = 'Offline';
      document.getElementById('stat-health').style.color = 'var(--error)';
    }
  } catch (_) {}
});
