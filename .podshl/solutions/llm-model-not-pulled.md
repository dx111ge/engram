---
id: llm-model-not-pulled
answers:
  problem_class: engram.llm.model-not-pulled
  when:
    engram.llm_endpoint: Ollama on this machine
severity: medium
proposes:
  - action: report_only
    because: >-
      Pulling a model downloads several gigabytes over your connection. That is
      not something to start on your behalf because a diagnosis was running
---
Engram reaches Ollama over its OpenAI-compatible endpoint and asks for a model
by name. Ollama answers a request for a model it does not hold with a 404 and
the message `model '<name>' not found, try pulling it first` — the endpoint is
reachable, the server is running, and the one thing missing is the model.

That is why this looks like a broken configuration rather than a missing
download: the connection test passes, and only Debate and Chat fail, because
they are the parts that need the model rather than the endpoint.

The README recommends Gemma 4:

    ollama pull gemma4:e4b

Then check the name in Engram matches exactly what `ollama list` prints. A tag
that differs — `gemma4` against `gemma4:e4b` — fails the same way and reads the
same way, because Ollama treats the whole string as the name.

Storing knowledge, search and the graph do not need an LLM at all and keep
working throughout.
