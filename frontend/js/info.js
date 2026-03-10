/* ============================================
   engram - System Info View
   Capabilities, embedder status, tips
   ============================================ */

router.register('/info', async () => {
  renderTo(`
    <div class="view-header">
      <div>
        <h1><i class="fa-solid fa-circle-info"></i> System Info</h1>
        <p class="text-secondary" style="margin-top:0.25rem">Capabilities and configuration at a glance</p>
      </div>
    </div>
    <div id="info-content">${loadingHTML('Gathering system information...')}</div>
  `);

  await loadInfoView();
});

async function loadInfoView() {
  const container = document.getElementById('info-content');

  // Fetch data in parallel
  let compute = null, stats = null;
  let sourcesEnabled = false, actionsEnabled = false, reasonEnabled = false;
  let nerAvailable = false;

  const promises = [
    engram.compute().then(r => { compute = r; }).catch(() => {}),
    engram.stats().then(r => { stats = r; }).catch(() => {}),
    engram._fetch('/sources').then(() => { sourcesEnabled = true; }).catch(() => {}),
    engram._fetch('/actions/rules').then(() => { actionsEnabled = true; }).catch(() => {}),
    engram._fetch('/reason/gaps').then(() => { reasonEnabled = true; }).catch(() => {}),
    engram._post('/ingest/configure', {}).then(() => { nerAvailable = true; }).catch(() => {}),
  ];
  await Promise.allSettled(promises);

  const hasEmbedder = compute && (compute.embedder_model || compute.embedder_endpoint);
  const embedderIsOnnx = hasEmbedder && compute.embedder_model &&
    /onnx/i.test(compute.embedder_model);
  const embedderIsOllama = hasEmbedder && compute.embedder_endpoint &&
    /11434|ollama/i.test(compute.embedder_endpoint);
  const embedderIsOpenAI = hasEmbedder && compute.embedder_endpoint &&
    /openai/i.test(compute.embedder_endpoint);

  let html = '';

  // --- Embedder Status Card (always shown, prominent) ---
  html += buildEmbedderCard(compute, hasEmbedder, embedderIsOnnx, embedderIsOllama, embedderIsOpenAI);

  // --- Capability Cards in 2-column grid ---
  html += '<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(380px,1fr));gap:1rem;margin-bottom:1.5rem">';

  html += capabilityCard({
    icon: 'fa-magnifying-glass',
    title: 'Search',
    levels: [
      {
        label: 'Full-text search (BM25)',
        quality: hasEmbedder ? null : 'current',
        qualityLevel: 'basic',
        tech: 'Built-in',
        license: 'Proprietary',
        licenseType: 'proprietary',
        note: 'Always available',
        active: true,
      },
      {
        label: 'Semantic search via embeddings',
        quality: hasEmbedder && !embedderIsOnnx ? 'current' : (hasEmbedder ? null : 'upgrade'),
        qualityLevel: 'good',
        tech: hasEmbedder ? techFromEmbedder(compute) : 'Ollama / OpenAI API',
        license: hasEmbedder ? licenseFromEmbedder(compute) : 'Varies',
        licenseType: hasEmbedder ? licenseTypeFromEmbedder(compute) : 'info',
        note: hasEmbedder && !embedderIsOnnx
          ? 'Semantic search active via ' + escapeHtml(compute.embedder_model || 'API')
          : null,
        upgradeLink: !hasEmbedder,
        active: hasEmbedder && !embedderIsOnnx,
      },
      {
        label: 'ONNX local model',
        quality: embedderIsOnnx ? 'current' : 'best',
        qualityLevel: 'excellent',
        tech: 'ONNX Runtime',
        license: 'MIT',
        licenseType: 'mit',
        note: embedderIsOnnx
          ? 'Local ONNX model active'
          : null,
        upgradeLink: !embedderIsOnnx,
        active: embedderIsOnnx,
      },
    ],
  });

  html += capabilityCard({
    icon: 'fa-tag',
    title: 'Entity Recognition (NER)',
    levels: [
      {
        label: 'Built-in rule-based',
        quality: !nerAvailable ? 'current' : null,
        qualityLevel: 'basic',
        tech: 'Built-in',
        license: 'Proprietary',
        licenseType: 'proprietary',
        note: 'Always available',
        active: true,
      },
      {
        label: 'Ingest pipeline with NER',
        quality: nerAvailable ? 'current' : 'upgrade',
        qualityLevel: 'good',
        tech: 'engram-ingest',
        license: 'Proprietary',
        licenseType: 'proprietary',
        note: nerAvailable
          ? 'Ingest pipeline active with NER stages'
          : 'Not available in this build',
        active: nerAvailable,
      },
      {
        label: 'spaCy / Anno NER model',
        quality: 'best',
        qualityLevel: 'excellent',
        tech: 'spaCy / Anno',
        license: 'MIT',
        licenseType: 'mit',
        note: null,
        upgradeLink: true,
        active: false,
      },
    ],
  });

  html += capabilityCard({
    icon: 'fa-gears',
    title: 'Action Engine',
    levels: [
      {
        label: 'No automated actions',
        quality: !actionsEnabled ? 'current' : null,
        qualityLevel: 'none',
        tech: 'None',
        license: null,
        licenseType: null,
        note: 'Store and query manually',
        active: false,
        hidden: actionsEnabled,
      },
      {
        label: 'Rule-based triggers',
        quality: actionsEnabled ? 'current' : 'upgrade',
        qualityLevel: 'good',
        tech: 'engram-action',
        license: 'Proprietary',
        licenseType: 'proprietary',
        note: actionsEnabled
          ? 'Action engine active'
          : 'Not available in this build',
        active: actionsEnabled,
      },
    ],
  });

  html += capabilityCard({
    icon: 'fa-lightbulb',
    title: 'Intelligence / Reasoning',
    levels: [
      {
        label: 'Graph traversal + inference',
        quality: !reasonEnabled ? 'current' : null,
        qualityLevel: 'basic',
        tech: 'Built-in',
        license: 'Proprietary',
        licenseType: 'proprietary',
        note: 'Always available',
        active: true,
      },
      {
        label: 'Gap detection + confidence reasoning',
        quality: reasonEnabled ? 'current' : 'upgrade',
        qualityLevel: 'good',
        tech: 'engram-core',
        license: 'Proprietary',
        licenseType: 'proprietary',
        note: reasonEnabled
          ? 'Advanced reasoning active'
          : 'Automatically enabled with sufficient data',
        active: reasonEnabled,
      },
    ],
  });

  html += '</div>'; // close grid

  // --- Tips Card ---
  html += buildTipsCard(hasEmbedder, sourcesEnabled, stats);

  container.innerHTML = html;
}

// ── Embedder Status Card ──

function buildEmbedderCard(compute, hasEmbedder, isOnnx, isOllama, isOpenAI) {
  if (hasEmbedder) {
    const model = compute.embedder_model || 'Unknown model';
    const dims = compute.embedder_dim || '?';
    const endpoint = compute.embedder_endpoint || 'Local';
    return `
      <div class="card" style="margin-bottom:1.5rem;border:1px solid rgba(0,184,148,0.3)">
        <div class="card-header" style="display:flex;align-items:center;justify-content:space-between">
          <h3><i class="fa-solid fa-vector-square"></i> Embedding Model</h3>
          <span style="font-size:0.75rem;font-weight:600;color:var(--success);background:rgba(0,184,148,0.1);border:1px solid rgba(0,184,148,0.2);padding:0.2rem 0.6rem;border-radius:3px;display:inline-flex;align-items:center;gap:0.4rem">
            <i class="fa-solid fa-lock"></i> Committed
          </span>
        </div>
        <div style="display:flex;flex-direction:column;gap:0.25rem">
          ${infoRow('fa-cube', 'Model', escapeHtml(model))}
          ${infoRow('fa-ruler', 'Dimensions', dims)}
          ${infoRow('fa-link', 'Endpoint', escapeHtml(endpoint))}
        </div>
        <p style="font-size:0.8rem;color:var(--warning);margin-top:0.75rem;margin-bottom:0">
          <i class="fa-solid fa-triangle-exclamation"></i>
          Changing your embedding model requires a full reindex of all data.
        </p>
      </div>`;
  }

  return `
    <div class="card" style="margin-bottom:1.5rem;border:1px solid rgba(253,203,110,0.3)">
      <div class="card-header" style="display:flex;align-items:center;justify-content:space-between">
        <h3><i class="fa-solid fa-vector-square"></i> Embedding Model</h3>
        <span style="font-size:0.75rem;font-weight:600;color:var(--warning);background:rgba(253,203,110,0.1);border:1px solid rgba(253,203,110,0.2);padding:0.2rem 0.6rem;border-radius:3px;display:inline-flex;align-items:center;gap:0.4rem">
          <i class="fa-solid fa-circle-exclamation"></i> Not Configured
        </span>
      </div>
      <p style="font-size:0.9rem;color:var(--text-secondary);margin:0 0 0.5rem 0">
        No embedding model is set. Semantic search and similarity features are unavailable.
      </p>
      <p style="font-size:0.85rem;margin:0">
        <a href="#/settings" style="color:var(--accent-bright);text-decoration:none">
          <i class="fa-solid fa-arrow-right"></i> Configure in Settings
        </a>
      </p>
    </div>`;
}

// ── Capability Card Builder ──

function capabilityCard(cfg) {
  let html = `
    <div class="card" style="margin-bottom:0">
      <div class="card-header" style="padding-bottom:0.4rem">
        <h3 style="font-size:1rem"><i class="fa-solid ${cfg.icon}"></i> ${escapeHtml(cfg.title)}</h3>
      </div>
      <div style="display:flex;flex-direction:column;gap:0">`;

  const levels = cfg.levels.filter(l => !l.hidden);

  levels.forEach((level, idx) => {
    const isLast = idx === levels.length - 1;
    const isCurrent = level.quality === 'current';
    const isActive = level.active;

    // Connector line
    const connector = !isLast
      ? `<div style="position:absolute;left:11px;top:22px;bottom:-1px;width:2px;background:${isCurrent ? 'var(--accent-bright)' : 'var(--border)'}"></div>`
      : '';

    // Node dot
    const dotColor = isCurrent ? 'var(--accent-bright)' : isActive ? 'var(--success)' : 'var(--border-light)';
    const dotStyle = isCurrent
      ? `width:10px;height:10px;border-radius:50%;background:${dotColor};border:2px solid var(--accent-bright);box-shadow:0 0 6px rgba(74,158,255,0.4)`
      : `width:8px;height:8px;border-radius:50%;background:${dotColor};border:2px solid ${isActive ? 'var(--success)' : 'var(--border-light)'}`;

    // Quality badge
    const badge = qualityBadge(level.qualityLevel, isCurrent);

    // License badge
    const licenseBadge = level.license ? inlineLicenseBadge(level.license, level.licenseType) : '';

    // Role label
    let roleLabel = '';
    if (isCurrent) {
      roleLabel = `<span style="font-size:0.65rem;font-weight:600;text-transform:uppercase;letter-spacing:0.05em;color:var(--accent-bright);background:rgba(74,158,255,0.1);padding:0.1rem 0.35rem;border-radius:3px;margin-left:0.3rem">Current</span>`;
    } else if (level.quality === 'upgrade') {
      roleLabel = `<span style="font-size:0.65rem;font-weight:600;text-transform:uppercase;letter-spacing:0.05em;color:var(--text-muted);margin-left:0.3rem">Upgrade</span>`;
    } else if (level.quality === 'best') {
      roleLabel = `<span style="font-size:0.65rem;font-weight:600;text-transform:uppercase;letter-spacing:0.05em;color:var(--warning);margin-left:0.3rem">Best</span>`;
    }

    // Row background
    const rowBg = isCurrent ? 'background:rgba(74,158,255,0.05);border:1px solid rgba(74,158,255,0.15);border-radius:var(--radius-sm)' : '';

    // Upgrade link
    const upgradeAction = level.upgradeLink && !isActive
      ? `<a href="#/settings" style="font-size:0.75rem;color:var(--accent-bright);text-decoration:none;margin-left:0.5rem;white-space:nowrap"><i class="fa-solid fa-arrow-right" style="font-size:0.65rem"></i> Configure in Settings</a>`
      : '';

    html += `
      <div style="display:flex;align-items:flex-start;gap:0.6rem;padding:0.45rem 0.4rem;position:relative;${rowBg}">
        ${connector}
        <div style="flex-shrink:0;margin-top:0.3rem;position:relative;z-index:1">
          <div style="${dotStyle}"></div>
        </div>
        <div style="flex:1;min-width:0">
          <div style="display:flex;align-items:center;flex-wrap:wrap;gap:0.2rem">
            <span style="font-size:0.85rem;font-weight:${isCurrent ? '600' : '400'};color:${isCurrent ? 'var(--text-primary)' : 'var(--text-secondary)'}">${escapeHtml(level.label)}</span>
            ${roleLabel}
            ${upgradeAction}
          </div>
          <div style="display:flex;align-items:center;gap:0.4rem;margin-top:0.15rem;flex-wrap:wrap">
            ${level.tech ? `<span style="font-size:0.7rem;color:var(--text-muted);background:var(--bg-input);padding:0.05rem 0.35rem;border-radius:3px;border:1px solid var(--border)">${escapeHtml(level.tech)}</span>` : ''}
            ${badge}
            ${licenseBadge}
          </div>
          ${level.note ? `<div style="font-size:0.75rem;color:var(--text-muted);margin-top:0.1rem">${escapeHtml(level.note)}</div>` : ''}
        </div>
      </div>`;
  });

  html += '</div></div>';
  return html;
}

// ── Quality Badge ──

function qualityBadge(level, isCurrent) {
  const styles = {
    none:      { color: '#888', bg: 'rgba(136,136,136,0.1)', border: 'rgba(136,136,136,0.2)', icon: 'fa-circle-minus',  text: 'Not Active' },
    basic:     { color: '#6e8eaa', bg: 'rgba(110,142,170,0.1)', border: 'rgba(110,142,170,0.2)', icon: 'fa-signal',  text: 'Basic' },
    good:      { color: '#4a9eff', bg: 'rgba(74,158,255,0.1)', border: 'rgba(74,158,255,0.2)', icon: 'fa-signal',  text: 'Good' },
    excellent: { color: '#00b894', bg: 'rgba(0,184,148,0.1)', border: 'rgba(0,184,148,0.2)', icon: 'fa-signal',  text: 'Excellent' },
  };
  if (!level || !styles[level]) return '';
  const s = styles[level];
  return `<span style="font-size:0.65rem;font-weight:600;color:${s.color};background:${s.bg};border:1px solid ${s.border};padding:0.05rem 0.35rem;border-radius:3px;display:inline-flex;align-items:center;gap:0.25rem"><i class="fa-solid ${s.icon}" style="font-size:0.55rem"></i> ${s.text}</span>`;
}

// ── Inline License Badge ──

function inlineLicenseBadge(license, licenseType) {
  const colors = {
    mit:         { color: '#00b894', bg: 'rgba(0,184,148,0.08)', border: 'rgba(0,184,148,0.2)' },
    apache:      { color: '#4a9eff', bg: 'rgba(74,158,255,0.08)', border: 'rgba(74,158,255,0.2)' },
    commercial:  { color: '#fdcb6e', bg: 'rgba(253,203,110,0.08)', border: 'rgba(253,203,110,0.2)' },
    proprietary: { color: '#a29bfe', bg: 'rgba(162,155,254,0.08)', border: 'rgba(162,155,254,0.2)' },
    info:        { color: '#888', bg: 'rgba(136,136,136,0.08)', border: 'rgba(136,136,136,0.2)' },
  };
  const c = colors[licenseType] || colors.info;
  return `<span style="font-size:0.6rem;font-weight:500;color:${c.color};background:${c.bg};border:1px solid ${c.border};padding:0.05rem 0.3rem;border-radius:3px"><i class="fa-solid fa-scale-balanced" style="font-size:0.5rem;margin-right:0.2rem"></i>${escapeHtml(license)}</span>`;
}

// ── Tech label from embedder info ──

function techFromEmbedder(compute) {
  if (!compute) return 'Unknown';
  const ep = (compute.embedder_endpoint || '').toLowerCase();
  if (/ollama|11434/.test(ep)) return 'Ollama';
  if (/openai/.test(ep)) return 'OpenAI API';
  if (/vlllm|vllm/.test(ep)) return 'vLLM';
  return 'API Embedder';
}

function licenseFromEmbedder(compute) {
  if (!compute) return 'Varies';
  const ep = (compute.embedder_endpoint || '').toLowerCase();
  if (/ollama|11434/.test(ep)) return 'Apache 2.0';
  if (/openai/.test(ep)) return 'Commercial';
  return 'Varies';
}

function licenseTypeFromEmbedder(compute) {
  if (!compute) return 'info';
  const ep = (compute.embedder_endpoint || '').toLowerCase();
  if (/ollama|11434/.test(ep)) return 'apache';
  if (/openai/.test(ep)) return 'commercial';
  return 'info';
}

// ── Tips Card ──

function buildTipsCard(hasEmbedder, sourcesEnabled, stats) {
  const tips = [];

  if (!hasEmbedder) {
    tips.push({
      icon: 'fa-vector-square',
      html: 'Add an embedding model for semantic search. <a href="#/settings" style="color:var(--accent-bright);text-decoration:none"><i class="fa-solid fa-arrow-right" style="font-size:0.8rem"></i> Configure in Settings</a>',
    });
  }

  if (stats) {
    const nodeCount = stats.nodes || stats.node_count || 0;
    if (nodeCount < 10) {
      tips.push({
        icon: 'fa-plus-circle',
        html: 'Add more facts to build a useful knowledge base. Try the Add page to start remembering important information.',
      });
    }
    const typeCount = stats.types || stats.type_count || 0;
    if (typeCount === 0 || typeCount === '--') {
      tips.push({
        icon: 'fa-tags',
        html: 'Add types to your entities for better organization. For example: person, concept, tool, event, location.',
      });
    }
  }

  if (!sourcesEnabled) {
    tips.push({
      icon: 'fa-database',
      html: 'The ingest feature enables automatic text analysis and entity extraction. Not available in this build.',
    });
  }

  if (tips.length === 0) return '';

  let html = `
    <div class="card">
      <div class="card-header">
        <h3><i class="fa-solid fa-lightbulb"></i> Tips</h3>
      </div>
      <div style="display:flex;flex-direction:column;gap:0.5rem">`;

  for (const tip of tips) {
    html += `
      <div style="display:flex;gap:0.6rem;align-items:flex-start;padding:0.6rem;background:var(--bg-secondary);border-radius:var(--radius-sm);border:1px solid var(--border)">
        <i class="fa-solid ${tip.icon}" style="color:var(--accent-bright);font-size:1rem;margin-top:0.1rem;flex-shrink:0"></i>
        <div style="font-size:0.85rem;color:var(--text-secondary)">${tip.html}</div>
      </div>`;
  }

  html += '</div></div>';
  return html;
}

// ── Shared helpers ──

function infoRow(icon, label, value) {
  return `
    <div style="display:flex;align-items:center;gap:0.75rem;padding:0.5rem 0;border-bottom:1px solid var(--border)">
      <i class="fa-solid ${icon}" style="color:var(--accent-bright);width:20px;text-align:center;flex-shrink:0"></i>
      <span style="font-size:0.85rem;color:var(--text-secondary);min-width:100px">${escapeHtml(label)}</span>
      <span style="font-size:0.9rem">${value}</span>
    </div>`;
}
