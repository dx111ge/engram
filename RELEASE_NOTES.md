## Engram v1.1.2

AI Intelligence Platform -- knowledge graph + semantic search + reasoning + multi-agent debate in a single binary.

### v1.1.2 Bug Fixes

- **Chat search/topic_map LLM summary** -- search and topic_map tools now generate AI summaries after showing results
- **Conflicts pagination** -- proper page controls instead of hard limit
- **Mesh endpoints** -- return 200 with disabled status instead of 503 when mesh is off
- **Ingest page fix** -- correct JSON payload format (missing items field)
- **Onboarding wizard** -- added Serper.dev as search provider option
- **SearxNG setup guide** -- new documentation for self-hosted web search configuration

### v1.1.1

- Bundled frontend with binary in zip (no separate frontend/ folder needed)

### v1.1.0 Highlights

- **Multi-agent debate** -- 7 analysis modes with War Room live dashboard
- **Chat system** -- 47 tools across 8 clusters
- **Assessment engine** -- Bayesian confidence with evidence boards
- **Document pipeline** -- PDF/HTML ingest, table extraction, source CRUD, folder watch
- **Temporal facts** -- valid_from / valid_to with automatic extraction
- **Contradiction detection** -- ConflictDetector with resolution workflows
- **NER category learning** -- three-tier self-improving label system
- **GLiNER2 ONNX** -- GPU acceleration (DirectML/CUDA/CoreML)
- **Onboarding wizard** -- 11-step guided setup
- **230+ REST endpoints**, 24 MCP tools
- **Tiered web search** -- SearXNG -> Serper -> Brave -> DuckDuckGo

### Downloads

| Platform | File |
|----------|------|
| Windows x86_64 | engram-windows-x86_64.zip |
| Linux x86_64 | engram-linux-x86_64.zip |
| Linux aarch64 | engram-linux-aarch64.zip |
| macOS aarch64 | engram-macos-aarch64.zip |

### Quick Start

```bash
# Unzip and run
engram serve my.brain
# Open http://localhost:3030
```

### Recommended LLM

Gemma 4 via Ollama:
```bash
ollama pull gemma4:e4b
```

### Documentation

Full documentation: [Wiki](https://github.com/dx111ge/engram/wiki)
