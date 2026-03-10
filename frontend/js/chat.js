/* ============================================
   engram - Chat View
   Agent-style conversational interface with
   knowledge graph grounding and active learning
   ============================================ */

let chatHistory = [];
let chatProcessing = false;

const CHAT_SYSTEM_PROMPT = `You are engram's knowledge analyst. You answer questions STRICTLY based on the knowledge graph data provided to you. You have access to tools to search and query the knowledge graph.

RULES:
- ONLY state facts that are supported by the tool results. Never invent or hallucinate information.
- When data is insufficient, say so explicitly. Suggest what additional data could be ingested.
- For "what-if" scenarios: analyze based on existing relationships and connections in the graph. State your confidence level.
- For impact assessments: map the news/event to existing entities and relationships. Show which connections are affected.
- For probability analysis: use confidence scores from the graph. Facts with high confidence (>0.7) are strong, moderate (0.4-0.7) are uncertain, low (<0.4) are weak.
- Always cite which entities and relationships you base your analysis on.
- When you use a tool, explain briefly what you found.
- Keep responses focused and structured. Use bullet points for clarity.`;

const CHAT_TOOLS = [
  {
    type: 'function',
    function: {
      name: 'search_knowledge',
      description: 'Search the engram knowledge graph for facts matching a query. Returns entities with confidence scores and types.',
      parameters: {
        type: 'object',
        properties: {
          query: { type: 'string', description: 'Search query (natural language or keywords)' }
        },
        required: ['query']
      }
    }
  },
  {
    type: 'function',
    function: {
      name: 'ask_engram',
      description: 'Ask engram a natural language question. Returns a direct answer based on the knowledge graph.',
      parameters: {
        type: 'object',
        properties: {
          question: { type: 'string', description: 'Natural language question' }
        },
        required: ['question']
      }
    }
  },
  {
    type: 'function',
    function: {
      name: 'get_entity',
      description: 'Get detailed information about a specific entity including its properties, connections, and confidence score.',
      parameters: {
        type: 'object',
        properties: {
          label: { type: 'string', description: 'Entity label/name' }
        },
        required: ['label']
      }
    }
  },
  {
    type: 'function',
    function: {
      name: 'find_similar',
      description: 'Find entities semantically similar to a given text. Requires an embedding model to be configured.',
      parameters: {
        type: 'object',
        properties: {
          text: { type: 'string', description: 'Text to find similar entities for' },
          limit: { type: 'number', description: 'Max results (default 10)' }
        },
        required: ['text']
      }
    }
  },
  {
    type: 'function',
    function: {
      name: 'search_news',
      description: 'Search for recent news articles on a topic. Use this to gather fresh information about current events.',
      parameters: {
        type: 'object',
        properties: {
          query: { type: 'string', description: 'News search query' }
        },
        required: ['query']
      }
    }
  },
  {
    type: 'function',
    function: {
      name: 'web_search',
      description: 'Search the web for information. Use when knowledge graph lacks context and user needs fresh data.',
      parameters: {
        type: 'object',
        properties: {
          query: { type: 'string', description: 'Web search query' }
        },
        required: ['query']
      }
    }
  },
  {
    type: 'function',
    function: {
      name: 'ingest_fact',
      description: 'Add a new fact to the knowledge graph. Use this to store relevant findings from searches or user-provided information.',
      parameters: {
        type: 'object',
        properties: {
          statement: { type: 'string', description: 'Fact to store in natural language' },
          source: { type: 'string', description: 'Source attribution (e.g. "news", "web-search", "user")' }
        },
        required: ['statement']
      }
    }
  }
];

// Execute a tool call against engram APIs
async function executeTool(name, args) {
  try {
    switch (name) {
      case 'search_knowledge': {
        const results = await engram.search({ query: args.query });
        const items = results.results || results.entities || results || [];
        if (!Array.isArray(items) || items.length === 0) return { found: 0, results: [] };
        return {
          found: items.length,
          results: items.slice(0, 15).map(i => ({
            label: i.label || i.entity || i,
            type: i.node_type || i.type || null,
            confidence: i.confidence != null ? Math.round(i.confidence * 100) / 100 : null,
            properties: i.properties || null,
          }))
        };
      }
      case 'ask_engram': {
        const result = await engram.ask({ question: args.question });
        return result;
      }
      case 'get_entity': {
        const node = await engram.getNode(args.label);
        return node;
      }
      case 'find_similar': {
        const results = await engram.similar({ text: args.text, limit: args.limit || 10 });
        const items = results.results || results || [];
        return { results: Array.isArray(items) ? items.slice(0, 10) : [] };
      }
      case 'search_news': {
        const resp = await engram._fetch(`/proxy/rss?q=${encodeURIComponent(args.query)}`);
        const items = resp.items || [];
        return { articles: items.slice(0, 8).map(a => ({ title: a.title, source: a.source, date: a.pubDate })) };
      }
      case 'web_search': {
        const resp = await engram._fetch(`/proxy/search?q=${encodeURIComponent(args.query)}`);
        const results = resp.results || resp || [];
        return { results: Array.isArray(results) ? results.slice(0, 8) : [] };
      }
      case 'ingest_fact': {
        const result = await engram.tell({ statement: args.statement, source: args.source || 'chat-agent' });
        return { ingested: true, label: result.label || result.entity || args.statement };
      }
      default:
        return { error: 'Unknown tool: ' + name };
    }
  } catch (err) {
    return { error: err.message };
  }
}

// Run the agent loop: send message, handle tool calls, repeat
async function runAgentLoop(userMessage) {
  // Add user message to history
  chatHistory.push({ role: 'user', content: userMessage });

  const messages = [
    { role: 'system', content: CHAT_SYSTEM_PROMPT },
    ...chatHistory
  ];

  const maxIterations = 6;
  let toolLog = [];

  for (let i = 0; i < maxIterations; i++) {
    // Get config for LLM settings
    let cfg = {};
    try { cfg = await engram.getConfig(); } catch (_) {}

    const endpoint = cfg.llm_endpoint || 'http://localhost:11434/v1';
    const model = cfg.llm_model || 'llama3.2';
    const apiKey = cfg.llm_api_key || '';

    const requestBody = {
      model: model,
      messages: messages,
      tools: CHAT_TOOLS,
      temperature: 0.2,
      max_tokens: 2048,
    };

    // Call LLM via proxy
    let response;
    try {
      response = await engram._post('/proxy/llm', requestBody);
    } catch (err) {
      return { content: 'LLM request failed: ' + err.message, tools: toolLog };
    }

    // Parse response
    const choice = response.choices?.[0];
    if (!choice) {
      return { content: 'No response from LLM.', tools: toolLog };
    }

    const msg = choice.message;

    // If the model wants to call tools
    if (msg.tool_calls && msg.tool_calls.length > 0) {
      // Add assistant message with tool calls to history
      messages.push(msg);

      for (const toolCall of msg.tool_calls) {
        const fn = toolCall.function;
        let args = {};
        try { args = typeof fn.arguments === 'string' ? JSON.parse(fn.arguments) : fn.arguments; } catch (_) {}

        // Update UI with tool execution status
        updateToolStatus(fn.name, args);

        const result = await executeTool(fn.name, args);
        toolLog.push({ tool: fn.name, args, result });

        // Add tool result to history
        messages.push({
          role: 'tool',
          tool_call_id: toolCall.id,
          content: JSON.stringify(result),
        });
      }
      continue; // Loop back for next LLM call with tool results
    }

    // Final response (no more tool calls)
    const content = msg.content || '';
    chatHistory.push({ role: 'assistant', content });
    return { content, tools: toolLog };
  }

  return { content: 'Agent reached maximum iterations. The analysis may be incomplete.', tools: toolLog };
}

function updateToolStatus(toolName, args) {
  const statusEl = document.getElementById('chat-tool-status');
  if (!statusEl) return;

  const icons = {
    search_knowledge: 'fa-magnifying-glass',
    ask_engram: 'fa-circle-question',
    get_entity: 'fa-cube',
    find_similar: 'fa-arrows-to-dot',
    search_news: 'fa-newspaper',
    web_search: 'fa-globe',
    ingest_fact: 'fa-plus-circle',
  };

  const labels = {
    search_knowledge: 'Searching knowledge graph',
    ask_engram: 'Asking engram',
    get_entity: 'Looking up entity',
    find_similar: 'Finding similar facts',
    search_news: 'Searching news',
    web_search: 'Searching the web',
    ingest_fact: 'Ingesting new fact',
  };

  const icon = icons[toolName] || 'fa-gear';
  const label = labels[toolName] || toolName;
  const detail = args.query || args.question || args.label || args.text || args.statement || '';

  statusEl.innerHTML = `
    <div style="display:flex;align-items:center;gap:0.5rem;padding:0.5rem 0.75rem;font-size:0.85rem;color:var(--text-secondary)">
      <span class="spinner" style="width:14px;height:14px"></span>
      <i class="fa-solid ${icon}"></i>
      <span>${escapeHtml(label)}</span>
      ${detail ? '<span class="text-muted" style="font-size:0.8rem">: ' + escapeHtml(detail.substring(0, 60)) + '</span>' : ''}
    </div>`;
}

// Render a message in the chat
function renderMessage(role, content, tools) {
  const container = document.getElementById('chat-messages');
  if (!container) return;

  const msgDiv = document.createElement('div');
  msgDiv.style.cssText = `display:flex;gap:0.75rem;padding:0.75rem 0;${role === 'user' ? 'flex-direction:row-reverse;' : ''}`;

  const avatar = role === 'user'
    ? '<div style="width:32px;height:32px;border-radius:50%;background:var(--accent-bright);display:flex;align-items:center;justify-content:center;flex-shrink:0"><i class="fa-solid fa-user" style="color:#fff;font-size:0.8rem"></i></div>'
    : '<div style="width:32px;height:32px;border-radius:50%;background:var(--bg-secondary);border:1px solid var(--border);display:flex;align-items:center;justify-content:center;flex-shrink:0"><i class="fa-solid fa-brain" style="color:var(--accent-bright);font-size:0.8rem"></i></div>';

  // Format content with basic markdown-like rendering
  let formattedContent = escapeHtml(content)
    .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
    .replace(/\n- /g, '\n<span style="margin-left:0.5rem">- </span>')
    .replace(/\n/g, '<br>');

  let toolsHtml = '';
  if (tools && tools.length > 0) {
    toolsHtml = `
      <details style="margin-top:0.5rem;font-size:0.8rem">
        <summary style="cursor:pointer;color:var(--text-muted)">
          <i class="fa-solid fa-wrench"></i> ${tools.length} tool${tools.length !== 1 ? 's' : ''} used
        </summary>
        <div style="margin-top:0.4rem;display:flex;flex-direction:column;gap:0.3rem">
          ${tools.map(t => `
            <div style="padding:0.3rem 0.5rem;background:var(--bg-input);border-radius:var(--radius-sm);display:flex;align-items:center;gap:0.4rem">
              <i class="fa-solid fa-gear" style="color:var(--text-muted);font-size:0.7rem"></i>
              <span style="font-weight:500">${escapeHtml(t.tool)}</span>
              <span class="text-muted">${escapeHtml(JSON.stringify(t.args).substring(0, 80))}</span>
              ${t.result?.error ? '<span style="color:var(--error)">failed</span>' : '<span style="color:var(--success)"><i class="fa-solid fa-check"></i></span>'}
            </div>
          `).join('')}
        </div>
      </details>`;
  }

  const bubbleAlign = role === 'user' ? 'margin-left:auto;background:var(--accent-bright);color:#fff;' : 'background:var(--bg-secondary);border:1px solid var(--border);';

  msgDiv.innerHTML = `
    ${avatar}
    <div style="max-width:75%;min-width:200px;padding:0.75rem 1rem;border-radius:var(--radius-sm);font-size:0.9rem;line-height:1.5;${bubbleAlign}">
      <div>${formattedContent}</div>
      ${toolsHtml}
    </div>`;

  container.appendChild(msgDiv);
  container.scrollTop = container.scrollHeight;
}

// Quick action buttons
const CHAT_QUICK_ACTIONS = [
  { icon: 'fa-circle-question', label: 'What do I know about...', prompt: 'What do I know about ' },
  { icon: 'fa-scale-balanced', label: 'What-if analysis', prompt: 'What if ' },
  { icon: 'fa-newspaper', label: 'Assess news impact', prompt: 'Assess the impact of this news: ' },
  { icon: 'fa-chart-line', label: 'Probability analysis', prompt: 'What is the probability that ' },
  { icon: 'fa-magnifying-glass-chart', label: 'Find gaps', prompt: 'What are the biggest knowledge gaps about ' },
];

router.register('/chat', async () => {
  // Check if LLM is configured
  let llmConfigured = false;
  try {
    const cfg = await engram.getConfig();
    llmConfigured = !!(cfg.llm_endpoint && cfg.llm_model);
  } catch (_) {}

  renderTo(`
    <div style="display:flex;flex-direction:column;height:calc(100vh - 60px);max-height:calc(100vh - 60px)">

      <!-- Header -->
      <div style="padding:0.75rem 1rem;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;flex-shrink:0">
        <div>
          <h2 style="font-size:1.1rem;margin:0"><i class="fa-solid fa-comments" style="color:var(--accent-bright)"></i> Knowledge Chat</h2>
          <p class="text-muted" style="font-size:0.8rem;margin:0">Ask questions, run scenarios, assess impacts -- grounded in your knowledge graph</p>
        </div>
        <div style="display:flex;gap:0.5rem;align-items:center">
          <span id="chat-llm-badge" style="font-size:0.75rem;padding:0.2rem 0.5rem;border-radius:999px;${llmConfigured ? 'background:rgba(46,160,67,0.15);color:var(--success);border:1px solid rgba(46,160,67,0.3)' : 'background:rgba(227,160,8,0.1);color:var(--confidence-mid);border:1px solid rgba(227,160,8,0.3)'}">
            <i class="fa-solid ${llmConfigured ? 'fa-circle-check' : 'fa-triangle-exclamation'}"></i>
            ${llmConfigured ? 'LLM Connected' : 'LLM Not Configured'}
          </span>
          <button class="btn btn-sm btn-secondary" id="btn-chat-clear" title="Clear chat">
            <i class="fa-solid fa-trash-can"></i>
          </button>
        </div>
      </div>

      <!-- Messages area -->
      <div id="chat-messages" style="flex:1;overflow-y:auto;padding:1rem;display:flex;flex-direction:column;gap:0.25rem">
        ${!llmConfigured ? `
          <div style="text-align:center;padding:2rem;color:var(--text-muted)">
            <i class="fa-solid fa-robot" style="font-size:2rem;display:block;margin-bottom:0.75rem;color:var(--text-muted)"></i>
            <p style="font-size:1rem;margin-bottom:0.5rem">Configure an LLM to start chatting</p>
            <p style="font-size:0.85rem">Go to <a href="#/settings">Settings</a> to set up your language model (Ollama, OpenAI, Anthropic, or vLLM).</p>
          </div>
        ` : `
          <div style="text-align:center;padding:1.5rem;color:var(--text-muted)">
            <i class="fa-solid fa-brain" style="font-size:1.5rem;display:block;margin-bottom:0.5rem;color:var(--accent-bright)"></i>
            <p style="font-size:0.95rem;margin-bottom:1rem">Ask me anything about your knowledge base</p>
            <div style="display:flex;flex-wrap:wrap;gap:0.5rem;justify-content:center" id="chat-quick-actions">
              ${CHAT_QUICK_ACTIONS.map(a => `
                <button class="btn btn-sm btn-secondary chat-quick-action" data-prompt="${escapeHtml(a.prompt)}" style="font-size:0.8rem">
                  <i class="fa-solid ${a.icon}"></i> ${escapeHtml(a.label)}
                </button>
              `).join('')}
            </div>
          </div>
        `}
      </div>

      <!-- Tool status -->
      <div id="chat-tool-status" style="flex-shrink:0"></div>

      <!-- Input area -->
      <div style="padding:0.75rem 1rem;border-top:1px solid var(--border);flex-shrink:0;background:var(--bg-card)">
        <div style="display:flex;gap:0.5rem;align-items:flex-end">
          <textarea id="chat-input" rows="2" placeholder="${llmConfigured ? 'Ask a question, describe a scenario, or paste news to assess...' : 'Configure LLM in Settings first...'}"
            style="flex:1;resize:none;padding:0.6rem 0.75rem;border:1px solid var(--border);border-radius:var(--radius-sm);background:var(--bg-input);color:var(--text-primary);font-size:0.9rem;font-family:inherit;line-height:1.4"
            ${!llmConfigured ? 'disabled' : ''}></textarea>
          <button class="btn btn-primary" id="btn-chat-send" style="height:fit-content;padding:0.6rem 1rem" ${!llmConfigured ? 'disabled' : ''}>
            <i class="fa-solid fa-paper-plane"></i>
          </button>
        </div>
        <div style="display:flex;justify-content:space-between;align-items:center;margin-top:0.4rem">
          <span class="text-muted" style="font-size:0.75rem">
            <i class="fa-solid fa-lock"></i> Grounded in knowledge graph data. Temperature: 0.2
          </span>
          <span class="text-muted" style="font-size:0.75rem">
            Ctrl+Enter to send
          </span>
        </div>
      </div>

    </div>
  `);

  setupChatEvents();
});

function setupChatEvents() {
  const input = document.getElementById('chat-input');
  const sendBtn = document.getElementById('btn-chat-send');
  const clearBtn = document.getElementById('btn-chat-clear');

  if (!input || !sendBtn) return;

  // Send on button click
  sendBtn.addEventListener('click', () => sendChatMessage());

  // Send on Ctrl+Enter
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.ctrlKey) {
      e.preventDefault();
      sendChatMessage();
    }
  });

  // Auto-resize textarea
  input.addEventListener('input', () => {
    input.style.height = 'auto';
    input.style.height = Math.min(input.scrollHeight, 150) + 'px';
  });

  // Clear chat
  clearBtn.addEventListener('click', () => {
    chatHistory = [];
    const messages = document.getElementById('chat-messages');
    if (messages) {
      messages.innerHTML = `
        <div style="text-align:center;padding:1.5rem;color:var(--text-muted)">
          <i class="fa-solid fa-brain" style="font-size:1.5rem;display:block;margin-bottom:0.5rem;color:var(--accent-bright)"></i>
          <p style="font-size:0.95rem;margin-bottom:1rem">Ask me anything about your knowledge base</p>
          <div style="display:flex;flex-wrap:wrap;gap:0.5rem;justify-content:center" id="chat-quick-actions">
            ${CHAT_QUICK_ACTIONS.map(a => `
              <button class="btn btn-sm btn-secondary chat-quick-action" data-prompt="${escapeHtml(a.prompt)}" style="font-size:0.8rem">
                <i class="fa-solid ${a.icon}"></i> ${escapeHtml(a.label)}
              </button>
            `).join('')}
          </div>
        </div>`;
      attachQuickActionHandlers();
    }
    showToast('Chat cleared', 'info');
  });

  // Quick action buttons
  attachQuickActionHandlers();
}

function attachQuickActionHandlers() {
  document.querySelectorAll('.chat-quick-action').forEach(btn => {
    btn.addEventListener('click', () => {
      const input = document.getElementById('chat-input');
      if (input) {
        input.value = btn.dataset.prompt;
        input.focus();
        // Place cursor at end
        input.setSelectionRange(input.value.length, input.value.length);
      }
    });
  });
}

async function sendChatMessage() {
  if (chatProcessing) return;

  const input = document.getElementById('chat-input');
  const message = input.value.trim();
  if (!message) return;

  chatProcessing = true;
  input.value = '';
  input.style.height = 'auto';

  const sendBtn = document.getElementById('btn-chat-send');
  sendBtn.disabled = true;

  // Remove welcome message if present
  const quickActions = document.getElementById('chat-quick-actions');
  if (quickActions) quickActions.closest('div[style*="text-align:center"]')?.remove();

  // Render user message
  renderMessage('user', message);

  // Show typing indicator
  const typingDiv = document.createElement('div');
  typingDiv.id = 'chat-typing';
  typingDiv.style.cssText = 'display:flex;gap:0.75rem;padding:0.75rem 0';
  typingDiv.innerHTML = `
    <div style="width:32px;height:32px;border-radius:50%;background:var(--bg-secondary);border:1px solid var(--border);display:flex;align-items:center;justify-content:center;flex-shrink:0">
      <i class="fa-solid fa-brain" style="color:var(--accent-bright);font-size:0.8rem"></i>
    </div>
    <div style="padding:0.75rem 1rem;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);font-size:0.85rem;color:var(--text-muted)">
      <span class="spinner" style="width:14px;height:14px;margin-right:0.4rem"></span> Thinking...
    </div>`;
  document.getElementById('chat-messages').appendChild(typingDiv);
  typingDiv.scrollIntoView({ behavior: 'smooth' });

  try {
    const result = await runAgentLoop(message);

    // Remove typing indicator
    typingDiv.remove();
    document.getElementById('chat-tool-status').innerHTML = '';

    // Render response
    renderMessage('assistant', result.content, result.tools);
  } catch (err) {
    typingDiv.remove();
    document.getElementById('chat-tool-status').innerHTML = '';
    renderMessage('assistant', 'Error: ' + err.message);
  } finally {
    chatProcessing = false;
    sendBtn.disabled = false;
    input.focus();
  }
}
