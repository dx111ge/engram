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
**Search stopped finding things it used to find.** You ask for something you
know is in there, and you get either nothing or a list that looks almost right
— some sensible hits mixed with results that have nothing to do with the
question. Nothing crashed and nothing is missing; the search just got worse,
and it got worse the day you changed the embedding model.

## What to do

One command, and it is the one the CLI reference names for exactly this:

    engram reindex my.brain

Use the path to your own `.brain` file. Let it finish before you use the brain
for anything else.

**Nothing is lost either way.** Your nodes, your edges, the confidence you built
up — none of it is touched. The only thing rewritten is the part used for
similarity search. On a large brain this takes a while, which is the only reason
this agent does not simply do it for you.

**If you would rather not wait**, set the previous embedding model back under
*System > Embeddings*. That makes what is already stored correct again, and then
no reindex is needed at all.

## Why it happens, if you want it

Each node was given a set of numbers when it was stored — a fingerprint of its
meaning, produced by whichever embedding model was configured at that moment.
Searching produces the same kind of fingerprint for your question and looks for
the closest matches.

Change the model and you change how the fingerprints are made. The ones already
in the brain are not redone. So you are comparing today's fingerprints against
yesterday's, which is not a worse comparison — it is a meaningless one.

That is also why the results look *partly* right rather than completely broken:
full-text search still works exactly as it did, so the keyword half keeps
finding what it always found, and the meaning half adds noise on top.

Reindexing gives every node a fingerprint from the model you use now.
