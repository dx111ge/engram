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
**Debate and Chat fail, but everything else is fine.** Storing knowledge works,
search works, the graph opens — and the connection test under *System > LLM*
even passes. Only the parts that need a language model come back with an error
or nothing at all.

## What to do

Ollama is running and answering; it just does not have the model Engram is
asking it for. Download it:

    ollama pull gemma4:e4b

Then check that the model name in Engram matches exactly what `ollama list`
prints. `gemma4` and `gemma4:e4b` are different names to Ollama, and asking for
the wrong one fails in exactly the same way.

**It is a few gigabytes over your connection**, which is why this agent does not
start the download for you.

## Why the connection test passes anyway

The test asks whether Ollama is *there*. It is — that is why it passes. The
model is a separate thing that Ollama holds or does not, and it is only asked
for when something actually needs to think.

When that happens, Ollama answers with a 404 and the message
`model '<name>' not found, try pulling it first`. Depending on where you see it,
that can look like a broken configuration rather than a missing download, which
is why this is worth saying out loud.

Nothing is wrong with your setup, and nothing needs reconfiguring.
