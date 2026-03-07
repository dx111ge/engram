# Use Case 3: Working with Multiple Languages

### Overview

Engram stores labels as UTF-8 strings with no restrictions on character set. This makes it practical for multilingual knowledge graphs: you can store entities in German, French, Japanese, Chinese, or any other script and create explicit translation relationships between them.

This walkthrough builds a small multilingual vocabulary graph, demonstrates cross-language relationship traversal, and documents the known limitations of BM25 search with non-ASCII text.

**What this demonstrates today (v0.1.0):**

- UTF-8 labels work natively in both CLI and HTTP API
- Cross-language relationships using the `translation` edge type
- Case-insensitive matching for ASCII characters
- BM25 search with non-English text (works; limitations noted below)

**Known limitations (v0.1.0):**

- BM25 tokenizer splits on whitespace and punctuation only — no language-aware stemming
- Japanese and Chinese text is not word-boundary-tokenized (no spaces between words in these scripts), so BM25 treats an entire CJK phrase as a single token
- Case folding applies only to ASCII characters; accented characters (e, o, etc.) are not case-folded
- No synonym expansion, no transliteration

### Prerequisites

- `engram` binary on your PATH
- No additional tools required for this walkthrough

### Step-by-Step Implementation

#### Step 1: Create the brain file

```bash
engram create multilang.brain
```

Expected output:

```
Created: multilang.brain
```

#### Step 2: Store entities in multiple languages

Store the same concept — "dog" — in five languages:

```bash
# English
engram store "Dog" multilang.brain

# German
engram store "Hund" multilang.brain

# French
engram store "Chien" multilang.brain

# Japanese
engram store "犬" multilang.brain

# Mandarin Chinese
engram store "狗" multilang.brain
```

Expected output (one per command):

```
Stored node 'Dog' (id: 1)
Stored node 'Hund' (id: 2)
Stored node 'Chien' (id: 3)
Stored node '犬' (id: 4)
Stored node '狗' (id: 5)
```

#### Step 3: Add metadata properties

```bash
engram set "Dog"   language English  multilang.brain
engram set "Hund"  language German   multilang.brain
engram set "Chien" language French   multilang.brain
engram set "犬"    language Japanese multilang.brain
engram set "狗"    language Chinese  multilang.brain

# Add type
engram set "Dog"   type animal multilang.brain
engram set "Hund"  type animal multilang.brain
engram set "Chien" type animal multilang.brain
engram set "犬"    type animal multilang.brain
engram set "狗"    type animal multilang.brain
```

Expected output (one per `set` command):

```
Dog.language = English
Hund.language = German
...
```

#### Step 4: Create translation relationships

```bash
engram relate "Hund"  "translation" "Dog"   multilang.brain
engram relate "Chien" "translation" "Dog"   multilang.brain
engram relate "犬"    "translation" "Dog"   multilang.brain
engram relate "狗"    "translation" "Dog"   multilang.brain

# Also cross-reference the non-English translations to each other
engram relate "Hund"  "translation" "Chien" multilang.brain
```

Expected output:

```
Hund -[translation]-> Dog (edge id: 6)
Chien -[translation]-> Dog (edge id: 7)
犬 -[translation]-> Dog (edge id: 8)
狗 -[translation]-> Dog (edge id: 9)
Hund -[translation]-> Chien (edge id: 10)
```

#### Step 5: Add a broader vocabulary

Store a second concept set — "cat" — to demonstrate that the graph grows naturally:

```bash
engram store "Cat"    multilang.brain
engram store "Katze"  multilang.brain
engram store "Chat"   multilang.brain
engram store "猫"     multilang.brain

engram set "Cat"   language English  multilang.brain
engram set "Katze" language German   multilang.brain
engram set "Chat"  language French   multilang.brain
engram set "猫"    language Japanese multilang.brain

engram relate "Katze" "translation" "Cat" multilang.brain
engram relate "Chat"  "translation" "Cat" multilang.brain
engram relate "猫"    "translation" "Cat" multilang.brain

# Semantic relationship across the English anchors
engram relate "Dog" "is_a" "animal" multilang.brain
engram relate "Cat" "is_a" "animal" multilang.brain
engram store "animal" multilang.brain
```

#### Step 6: Demonstrate case-insensitive matching (ASCII)

```bash
engram query "dog" multilang.brain
```

Expected output — the label `Dog` is found despite querying `dog` in lowercase:

```
Node: Dog
  id: 1
  confidence: 0.80
  memory_tier: active
Properties:
  language: English
  type: animal
Edges out:
  Dog -[is_a]-> animal (confidence: 0.80)
Edges in:
  Hund -[translation]-> Dog (confidence: 0.80)
  Chien -[translation]-> Dog (confidence: 0.80)
  犬 -[translation]-> Dog (confidence: 0.80)
  狗 -[translation]-> Dog (confidence: 0.80)
Reachable (1-hop): 6 nodes
```

Case folding for ASCII means `dog`, `Dog`, and `DOG` all resolve to the same node.

#### Step 7: Search across languages

```bash
engram search "Hund" multilang.brain
```

Expected output:

```
Results (1):
  Hund
```

BM25 works: the label `Hund` is indexed as the token `hund` (lowercased ASCII). A search for `Hund` returns it.

```bash
engram search "animal" multilang.brain
```

Expected output:

```
Results (3):
  animal
  Dog
  Cat
```

The `type: animal` property on `Dog` and `Cat` puts them in the BM25 result for "animal".

#### Step 8: Query a Japanese entity

```bash
engram query "犬" multilang.brain
```

Expected output:

```
Node: 犬
  id: 4
  confidence: 0.80
  memory_tier: active
Properties:
  language: Japanese
  type: animal
Edges out:
  犬 -[translation]-> Dog (confidence: 0.80)
```

The CJK label is stored and retrieved correctly. Case-insensitive matching does not apply to CJK characters, but exact-match lookup works.

#### Step 9: BM25 search limitation with CJK

```bash
engram search "犬" multilang.brain
```

Expected output:

```
Results (1):
  犬
```

This works because the entire string `犬` is a single character and becomes a single token in the BM25 index. If the entity were a multi-character Japanese phrase without spaces, the entire phrase would be indexed as one token — sub-phrase search would not match it.

```bash
# This will NOT match "犬" because BM25 does not know "い" is a sub-token of "犬"
engram search "い" multilang.brain
```

Expected output:

```
No results
```

This is a known limitation. For CJK full-text search, either store individual characters as separate entities or use an external tokenizer to pre-segment phrases before storing them.

### Querying the Results

#### Find all German entities

```bash
engram search "prop:language=German" multilang.brain
```

Expected output:

```
Results (2):
  Hund
  Katze
```

#### Traverse translations of "Dog"

```bash
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "Dog", "depth": 1, "min_confidence": 0.0}'
```

Expected output shows Dog and all nodes one hop away (its translations and `animal`):

```json
{
  "nodes": [
    {"node_id": 1,  "label": "Dog",    "confidence": 0.8, "depth": 0},
    {"node_id": 11, "label": "animal", "confidence": 0.8, "depth": 1},
    {"node_id": 2,  "label": "Hund",   "confidence": 0.8, "depth": 1},
    {"node_id": 3,  "label": "Chien",  "confidence": 0.8, "depth": 1},
    {"node_id": 4,  "label": "犬",     "confidence": 0.8, "depth": 1},
    {"node_id": 5,  "label": "狗",     "confidence": 0.8, "depth": 1}
  ],
  "edges": [...]
}
```

### Key Takeaways

- UTF-8 labels work in the CLI and HTTP API without any configuration.
- Case-insensitive matching applies to ASCII characters only. Accented Latin, CJK, and other Unicode characters use exact byte comparison.
- BM25 tokenization is whitespace-and-punctuation based. It works well for space-delimited languages (English, German, French). For CJK languages without word boundaries, either pre-segment the text or store at the granularity where exact match is sufficient.
- Translation graphs are straightforward: store both forms, create a `translation` edge, traverse to find equivalents.
