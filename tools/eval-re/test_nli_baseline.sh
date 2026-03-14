#!/bin/bash
# Test NLI baseline relation extraction with German text
# Uses the existing engram-rel sidecar with multilingual-MiniLMv2-L6-mnli-xnli
#
# Usage: bash test_nli_baseline.sh

set -e

MODEL_DIR="$HOME/.engram/models/rel/multilingual-MiniLMv2-L6-mnli-xnli"
ENGRAM_REL="$(dirname "$0")/../engram-rel/target/release/engram-rel"

if [ ! -f "$ENGRAM_REL" ]; then
    ENGRAM_REL="$(dirname "$0")/../../target/debug/engram-rel"
fi

if [ ! -f "$ENGRAM_REL" ]; then
    echo "Building engram-rel..."
    cd "$(dirname "$0")/../../"
    cargo build -p engram-rel 2>&1
    ENGRAM_REL="$(dirname "$0")/../../target/debug/engram-rel"
fi

if [ ! -d "$MODEL_DIR" ]; then
    echo "ERROR: NLI model not found at $MODEL_DIR"
    exit 1
fi

echo "=== NLI Baseline RE Evaluation ==="
echo "Model: multilingual-MiniLMv2-L6-mnli-xnli"
echo "Binary: $ENGRAM_REL"
echo ""

# Templates (matching engram-rel defaults)
TEMPLATES='"works_at":"{head} works at {tail}","headquartered_in":"{head} headquarters are in {tail}","located_in":"{head} is located in {tail}","leads":"{head} leads {tail}","founded":"{head} founded {tail}","supports":"{head} supports {tail}","born_in":"{head} was born in {tail}","member_of":"{head} is a member of {tail}"'

# Build all test requests
REQUESTS=$(cat <<'JSONL'
{"text":"Bill Gates is an American businessman who co-founded Microsoft.","entities":[{"text":"Bill Gates","label":"person","start":0,"end":10},{"text":"Microsoft","label":"organization","start":53,"end":62}],"relation_templates":{"works_at":"{head} works at {tail}","headquartered_in":"{head} headquarters are in {tail}","located_in":"{head} is located in {tail}","leads":"{head} leads {tail}","founded":"{head} founded {tail}","supports":"{head} supports {tail}","born_in":"{head} was born in {tail}","member_of":"{head} is a member of {tail}"},"threshold":0.5}
{"text":"Tim Cook ist der CEO von Apple. Apple hat seinen Hauptsitz in Cupertino.","entities":[{"text":"Tim Cook","label":"person","start":0,"end":8},{"text":"Apple","label":"organization","start":27,"end":32},{"text":"Cupertino","label":"location","start":62,"end":71}],"relation_templates":{"works_at":"{head} works at {tail}","headquartered_in":"{head} headquarters are in {tail}","located_in":"{head} is located in {tail}","leads":"{head} leads {tail}","founded":"{head} founded {tail}","supports":"{head} supports {tail}","born_in":"{head} was born in {tail}","member_of":"{head} is a member of {tail}"},"threshold":0.5}
{"text":"Max arbeitet bei Siemens in Muenchen.","entities":[{"text":"Max","label":"person","start":0,"end":3},{"text":"Siemens","label":"organization","start":17,"end":24},{"text":"Muenchen","label":"location","start":28,"end":36}],"relation_templates":{"works_at":"{head} works at {tail}","headquartered_in":"{head} headquarters are in {tail}","located_in":"{head} is located in {tail}","leads":"{head} leads {tail}","founded":"{head} founded {tail}","supports":"{head} supports {tail}","born_in":"{head} was born in {tail}","member_of":"{head} is a member of {tail}"},"threshold":0.5}
{"text":"Angela Merkel war Bundeskanzlerin von Deutschland.","entities":[{"text":"Angela Merkel","label":"person","start":0,"end":13},{"text":"Deutschland","label":"location","start":38,"end":49}],"relation_templates":{"works_at":"{head} works at {tail}","headquartered_in":"{head} headquarters are in {tail}","located_in":"{head} is located in {tail}","leads":"{head} leads {tail}","founded":"{head} founded {tail}","supports":"{head} supports {tail}","born_in":"{head} was born in {tail}","member_of":"{head} is a member of {tail}"},"threshold":0.5}
{"text":"Putin und Zelensky verhandeln ueber den Konflikt in der Ukraine. NATO unterstuetzt die Ukraine mit HIMARS.","entities":[{"text":"Putin","label":"person","start":0,"end":5},{"text":"Zelensky","label":"person","start":10,"end":18},{"text":"Ukraine","label":"location","start":56,"end":63},{"text":"NATO","label":"organization","start":65,"end":69},{"text":"HIMARS","label":"product","start":99,"end":105}],"relation_templates":{"works_at":"{head} works at {tail}","headquartered_in":"{head} headquarters are in {tail}","located_in":"{head} is located in {tail}","leads":"{head} leads {tail}","founded":"{head} founded {tail}","supports":"{head} supports {tail}","born_in":"{head} was born in {tail}","member_of":"{head} is a member of {tail}"},"threshold":0.5}
JSONL
)

# Pipe all requests to engram-rel, measure time per response
echo "$REQUESTS" | timeout 120 "$ENGRAM_REL" "$MODEL_DIR" 2>/dev/null | while IFS= read -r line; do
    echo "$line" | python3 -c "
import sys, json
data = json.load(sys.stdin)
if 'status' in data:
    print(f'Ready: {data[\"status\"]}')
elif data.get('ok') and 'relations' in data:
    rels = data['relations']
    if not rels:
        print('  Relations: (none)')
    else:
        for r in rels:
            print(f'  {r[\"head\"]:20} | {r[\"label\"]:20} | {r[\"tail\"]:20} | {r[\"score\"]:.1%}')
elif 'error' in data:
    print(f'  ERROR: {data[\"error\"]}')
"
done

echo ""
echo "=== NLI Baseline complete ==="
