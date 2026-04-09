# Engram Integrations

Engram provides multiple integration points for AI tools, agent frameworks, and external services.

---

## MCP (Model Context Protocol)

Native integration with Claude, Cursor, Windsurf, and any MCP-compatible AI tool.

- **Transport**: JSON-RPC 2.0 over stdio
- **Tools**: 24 tools covering graph operations, search, inference, intelligence, mesh, assessments, documents, and KGE
- **Resources**: graph stats and health status

See [mcp-server.md](mcp-server.md) for the full reference.

```bash
engram mcp /path/to/my.brain
```

---

## A2A (Agent-to-Agent Protocol)

Google's Agent-to-Agent protocol for inter-agent communication and skill routing.

**Available skills:**

| Skill | Description |
|-------|-------------|
| `query-knowledge` | Query the knowledge graph with traversal |
| `search-knowledge` | Full-text and semantic search |
| `store-knowledge` | Store new facts and relationships |
| `analyze-gaps` | Detect knowledge gaps and blind spots |
| `federated-search` | Search across mesh peers |
| `suggest-investigations` | Generate investigation suggestions for gaps |

**Streaming support**: Long-running operations use SSE streaming via `POST /tasks/sendSubscribe`. Events follow the pattern: `status-update` (working) -> `artifact-chunk` (partial results) -> `task-complete` or `task-failed`.

---

## gRPC

High-performance RPC for service-to-service communication.

- **Port**: 50051 (default)
- **Feature flag**: `--features grpc`

**Services:**

| Service | RPCs |
|---------|------|
| `EngramService` | Store, Relate, Query, Search, Similar, Reinforce, Correct, Decay, Derive, GetNode, DeleteNode, BatchStore, ExportJsonLd |
| `EngramStreamService` | EventStream, IngestProgress, EnrichStream (server-streaming), BulkIngest (client-streaming) |

```bash
engram serve my.brain --grpc
```

---

## LLM Tool-Calling

OpenAI-compatible function/tool definitions for direct LLM integration.

```bash
curl http://localhost:3030/tools
```

Returns tool definitions in OpenAI format. Compatible with any LLM that supports function calling (GPT-4, Claude, Gemini, Llama, etc.).

---

## SSE (Server-Sent Events)

Real-time event streaming for live dashboards and monitoring.

```bash
curl -N http://localhost:3030/events/stream
```

Events include:
- Node and edge changes
- Confidence updates
- Ingest pipeline progress
- Enrichment results

---

## Webhooks

Receive data from external services into the ingest pipeline.

```bash
curl -X POST http://localhost:3030/ingest/webhook \
  -H 'Content-Type: application/json' \
  -d '{"source": "external-service", "items": [...]}'
```

The action engine can also send webhooks as effects when rules trigger.

---

## WebSocket Ingest

Real-time streaming ingestion over WebSocket connections.

```
ws://localhost:3030/ingest/ws/{pipeline_id}
```

Send JSON items over the WebSocket connection for continuous ingestion. Each message is processed through the NER pipeline and stored in real-time.

---

## Natural Language Interface

Query and assert knowledge using natural language:

```bash
# Ask a question
curl -X POST http://localhost:3030/ask \
  -H 'Content-Type: application/json' \
  -d '{"text": "What databases are used in production?"}'

# Assert a fact
curl -X POST http://localhost:3030/tell \
  -H 'Content-Type: application/json' \
  -d '{"text": "PostgreSQL is the primary database"}'
```

---

## JSON-LD (Linked Data)

Import and export knowledge as JSON-LD for interoperability with RDF tools, Wikidata, DBpedia, and schema.org.

```bash
# Export
curl http://localhost:3030/export/jsonld
```

Import is handled via the ingest pipeline (`POST /ingest`) or individual `POST /store` and `POST /relate` calls.

---

## Python Integration

See [python-examples.md](python-examples.md) for:
- EngramClient wrapper class
- Bulk import from CSV/JSON
- LangChain tool integration
- Subprocess scripting

---

## LLM Configuration

Engram requires an LLM for seed enrichment, fact extraction, debate analysis, and gap-closing research. Any OpenAI-compatible chat API works.

### Supported Providers

| Provider | Endpoint | Auth | Notes |
|----------|----------|------|-------|
| Ollama (recommended) | `http://localhost:11434/v1/chat/completions` | None | Free, local, private |
| LM Studio | `http://localhost:1234/v1/chat/completions` | None | Free, local |
| vLLM | `http://localhost:8000/v1/chat/completions` | None | GPU-optimized serving |
| OpenAI | `https://api.openai.com/v1/chat/completions` | API key | Cloud |
| DeepSeek | `https://api.deepseek.com/v1/chat/completions` | API key | Budget cloud option |
| OpenRouter | `https://openrouter.ai/api/v1/chat/completions` | API key | Multi-model gateway |

### Model Selection Tips

- **14B+ parameters recommended** for the debate panel and fact extraction. Smaller models (7-8B) work for basic operations but may produce unreliable JSON output.
- **Thinking/reasoning models** (deepseek-r1, qwq, qwen3, gemma4) give deeper analysis for debate turns but use more tokens. Engram automatically toggles thinking on/off per task -- on for deep analysis, off for fast structured extraction.
- **Context window** is auto-detected when you set or change a model. For Ollama, engram sends `num_ctx` with every request to use the full context. Larger context windows produce better debate synthesis. Verify your model's context with `ollama show <model>`.
- **Some models think by default** (gemma4, qwen3). Engram sends `think: false` for structured output tasks. If you see slow or garbled JSON responses, this is likely the cause -- check that your model is in the detected thinking models list.

### Ollama Quick Start

```bash
# Install a recommended model
ollama pull phi4          # 14B, excellent reasoning, good JSON
ollama pull gemma4:e4b    # 47B, high quality, needs 24GB+ VRAM
ollama pull qwen3:14b     # 14B, multilingual, thinking model

# Check installed models and VRAM usage
ollama list
ollama ps

# Check context window
ollama show phi4 | grep context
```

Configure via the onboarding wizard (recommended) or API:

```bash
curl -X POST http://localhost:3030/config -H "Content-Type: application/json" \
  -d '{"llm_endpoint": "http://localhost:11434/v1/chat/completions", "llm_model": "phi4"}'
```

---

## Web Search Providers

Engram uses a **tiered web search fallback** for gap-closing and seed enrichment: SearXNG -> Serper -> Brave -> DuckDuckGo. The first available provider is used.

| Provider | Auth | Cost | Privacy |
|----------|------|------|---------|
| SearXNG (recommended) | Self-hosted URL | Free | Full control, no rate limits |
| Serper.dev | API key | Free tier: 2,500 queries | Google results via API |
| Brave Search | API key | Free tier: 2,000/month | Independent index |
| DuckDuckGo | None | Free | No tracking, lower quality |

### SearXNG Setup (Important)

SearXNG blocks JSON API access by default. Engram requires JSON format to be enabled, otherwise the connection test will fail with `FAIL: No results from searxng`.

In your SearXNG `settings.yml`, ensure the `search.formats` list includes `json`:

```yaml
search:
  formats:
    - html
    - json    # <-- required for engram
```

After changing the config, restart SearXNG:

```bash
# Docker
docker restart searxng

# systemd
sudo systemctl restart searxng
```

Verify it works:

```bash
curl "http://localhost:8090/search?q=test&format=json"
# Should return JSON with results, NOT a 403 Forbidden
```

### Serper.dev Setup

Get a free API key at [serper.dev](https://serper.dev/) (2,500 free Google search queries). Configure:

```bash
curl -X POST http://localhost:3030/secrets/SERPER_API_KEY \
  -H "Content-Type: application/json" \
  -d '{"value": "your-api-key"}'
```

### Brave Search Setup

Get a free API key at [brave.com/search/api](https://brave.com/search/api/). Enter the key in the onboarding wizard when selecting Brave Search.

### DuckDuckGo

No configuration needed. Selected by default. Uses the DuckDuckGo Instant Answer API (no API key required).

---

## Embedding Providers

Engram supports any OpenAI-compatible embedding API:

| Provider | Endpoint |
|----------|----------|
| Ollama | `http://localhost:11434/v1` |
| OpenAI | `https://api.openai.com/v1` |
| vLLM | `http://localhost:8000/v1` |
| Local ONNX | No endpoint needed (sidecar files) |

Configure via environment variables:
```bash
ENGRAM_EMBED_ENDPOINT=http://localhost:11434/v1
ENGRAM_EMBED_MODEL=nomic-embed-text-v2-moe:latest
```
