## Engram v1.1.0

AI Memory Engine -- knowledge graph + semantic search + reasoning + learning in a single binary.

### Highlights

- **Multi-agent debate** -- 7 analysis modes (Analyze, Red Team, Devil's Advocate, Scenario Planning, Delphi, SAT, War Game) with War Room live dashboard
- **Chat system** -- 47 tools across 8 clusters (analysis, investigation, reporting, temporal, assessment, action, reasoning)
- **Assessment engine** -- Bayesian confidence calculation with living assessments and evidence boards
- **Document pipeline** -- full PDF/HTML ingest, table extraction, source CRUD, folder watch, document provenance chain
- **Temporal facts** -- valid_from / valid_to on edges with 3-layer automatic extraction
- **Contradiction detection** -- ConflictDetector with resolution workflows
- **NER category learning** -- three-tier self-improving label system
- **GLiNER2 in-process ONNX** -- all sidecars removed, GPU acceleration (DirectML/CUDA/CoreML)
- **Onboarding wizard** -- 11-step guided setup (works via UI or API)
- **UX restructure** -- 4-section nav: Knowledge | Insights | Debate | System
- **230+ REST endpoints** across 15 groups
- **24 MCP tools** for Claude, Cursor, Windsurf
- **Tiered web search** -- SearXNG -> Serper -> Brave -> DuckDuckGo

### Downloads

| Platform | File | GPU |
|----------|------|-----|
| Windows x86_64 | `engram.exe` | DirectML |
| Linux x86_64 | `engram-linux-x86_64` | CUDA |
| Linux aarch64 | `engram-linux-aarch64` | CUDA |
| macOS x86_64 | `engram-macos-x86_64` | CoreML |
| macOS aarch64 | `engram-macos-aarch64` | CoreML |

### Quick Start

```bash
engram create my.brain
engram serve my.brain
# Open http://localhost:3030
```

### Recommended LLM

Gemma 4 via Ollama (thinking mode, large context window):
```bash
ollama pull gemma4:e4b
```

### Documentation

Full documentation: [Wiki](https://github.com/dx111ge/engram/wiki)

See [CHANGELOG.md](https://github.com/dx111ge/engram/blob/main/CHANGELOG.md) for the complete list of changes.
