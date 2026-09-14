---
id: search-stale-after-model-change
answers:
  problem_class: engram.search.stale-after-model-change
  when:
    engram.embedding_changed: "yes, and I did not run engram reindex"
severity: high
proposes:
  - action: report_only
    because: >-
      Re-embedding rewrites your knowledge base and can take a long time on a
      large brain. That is a decision with a cost, so it is yours to start
      rather than something that happens because a diagnosis ran
---
Semantic search compares the vector of your query against the vectors stored
with each node — and those were written when the node was stored, by whichever
model was configured at the time. Changing the model changes how queries are
embedded. It does not change what is already in the brain.

So every node stored before the change is being compared in one vector space
against a query in another, and the similarity scores that come back are
meaningless rather than merely worse. Full-text search is unaffected, which is
why results often look partly right: BM25 still finds what it always found, and
the semantic half contributes noise.

The CLI reference names the fix, and it is one command:

    engram reindex my.brain

It re-embeds every node with the model configured now. On a large brain this
takes a while and it is worth running when you do not need the brain for
something else. Nothing is lost either way — the nodes, edges and their
confidence are untouched; only the vectors are rewritten.

If you would rather go back, configuring the previous model again makes the
existing vectors correct once more, and then no reindex is needed.
