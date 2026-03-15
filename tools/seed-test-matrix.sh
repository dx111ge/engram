#!/bin/bash
# Seed Enrichment Test Matrix
# Tests various domains to measure graph quality after seeding

TOKEN=$(curl -s -X POST http://127.0.0.1:3030/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"12Buchstaben!!"}' | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
AUTH="Authorization: Bearer $TOKEN"

run_seed_test() {
  local name="$1"
  local text="$2"

  # Get stats before
  local before=$(curl -s http://127.0.0.1:3030/stats -H "$AUTH")
  local nodes_before=$(echo "$before" | grep -o '"nodes":[0-9]*' | cut -d: -f2)

  # Ingest via the regular pipeline (which uses KB + web search enrichment)
  local result=$(curl -s -X POST http://127.0.0.1:3030/ingest \
    -H "$AUTH" \
    -H "Content-Type: application/json" \
    -d "{\"items\":[\"$text\"],\"source\":\"seed-test\"}" \
    --max-time 120)

  local facts=$(echo "$result" | grep -o '"facts_stored":[0-9]*' | cut -d: -f2)
  local rels=$(echo "$result" | grep -o '"relations_created":[0-9]*' | cut -d: -f2)
  local ms=$(echo "$result" | grep -o '"duration_ms":[0-9]*' | cut -d: -f2)

  # Get stats after
  local after=$(curl -s http://127.0.0.1:3030/stats -H "$AUTH")
  local nodes_after=$(echo "$after" | grep -o '"nodes":[0-9]*' | cut -d: -f2)
  local edges_after=$(echo "$after" | grep -o '"edges":[0-9]*' | cut -d: -f2)

  local new_nodes=$((nodes_after - nodes_before))

  printf "| %-30s | %5s | %5s | %6s | %7s |\n" "$name" "$facts" "$rels" "$new_nodes" "${ms}ms"
}

echo ""
echo "## Seed Enrichment Test Matrix"
echo ""
printf "| %-30s | %5s | %5s | %6s | %7s |\n" "Topic" "Facts" "Rels" "Nodes" "Time"
printf "| %-30s | %5s | %5s | %6s | %7s |\n" "------------------------------" "-----" "-----" "------" "-------"

run_seed_test "Geopolitics: Russia-Ukraine" \
  "I'm a security analyst tracking the Russia-Ukraine war. Key figures include Putin, Zelensky, Macron, Stoltenberg, and Scholz. I monitor military equipment like HIMARS, Leopard 2, F-16, and Bayraktar drones. Organizations of interest include NATO, EU, UN, and the Wagner Group. Key locations are Kyiv, Moscow, Crimea, and Donbas."

run_seed_test "Cybersecurity: EU CISO" \
  "I'm a European CISO focused on cybersecurity and critical infrastructure protection. Key agencies include Germany's BSI, France's ANSSI, and NATO's CCDCOE in Tallinn. I track threat actors like APT28, APT29, and Lazarus Group. Important regulations include NIS2 and the Cyber Resilience Act. The energy sector, particularly Nord Stream infrastructure, is a priority."

run_seed_test "AI/Tech: AI Companies" \
  "I'm researching artificial intelligence companies and their key products. Major players include OpenAI with GPT-4, Google DeepMind with Gemini and AlphaFold, Anthropic with Claude, and Meta AI with LLaMA. Key researchers include Ilya Sutskever, Demis Hassabis, and Yann LeCun. Venture capital firms like Sequoia and Andreessen Horowitz fund this space."

run_seed_test "Finance: ServiceNow" \
  "I'm a stock analyst tracking ServiceNow and its competitive landscape. ServiceNow competes with Salesforce, BMC Software, Atlassian, and Freshworks in the IT service management space. CEO Bill McDermott leads the company headquartered in Santa Clara. Key products include ITSM, CMDB, and the Now Platform. I monitor quarterly earnings, cloud revenue growth, and enterprise AI integration."

run_seed_test "History: Central Europe" \
  "I study the history and geography of Central Europe, particularly the Holy Roman Empire and its successor states. Key figures include Charlemagne, Frederick the Great, Maria Theresa, and Otto von Bismarck. Important cities include Vienna, Prague, Berlin, and Munich. Rivers like the Danube, Rhine, and Elbe shaped trade routes and political boundaries."

run_seed_test "Supply Chain: Semiconductors" \
  "I analyze the global semiconductor supply chain. Key companies include TSMC, Samsung, Intel, ASML, and NVIDIA. I track the CHIPS Act, export controls on China, and the role of rare earth materials. Important locations are Taiwan, South Korea, and Silicon Valley. The AI chip demand from companies like NVIDIA, AMD, and Qualcomm drives market dynamics."

run_seed_test "Health: mRNA Vaccines" \
  "I research mRNA vaccine technology and its developers. Key companies include BioNTech founded by Ugur Sahin and Ozlem Tureci, Moderna led by Stephane Bancel, and Pfizer. I track clinical trials, FDA approvals, WHO recommendations, and the role of lipid nanoparticles in drug delivery. Important institutions include the NIH, EMA, and the Paul Ehrlich Institute."

run_seed_test "Energy: EU Transition" \
  "I monitor the European energy transition and energy security. Key topics include Nord Stream pipelines, LNG terminals, offshore wind farms, and hydrogen strategy. Important players are Gazprom, Equinor, TotalEnergies, and Orsted. EU institutions like the European Commission and ACER regulate the market. Germany's Energiewende and France's nuclear policy are central."

run_seed_test "Crime: Drug Trafficking" \
  "I investigate drug trafficking networks in Latin America. Key cartels include the Sinaloa Cartel, CJNG (Jalisco New Generation), and the Gulf Cartel. Important figures are El Mayo Zambada and the legacy of El Chapo. I track DEA operations, money laundering through cryptocurrency, and the fentanyl crisis. Key corridors run through Mexico, Colombia, and Guatemala."

run_seed_test "Academic: Transformers" \
  "I study the transformer architecture in machine learning. Key papers include Attention Is All You Need by Vaswani et al. at Google Brain. Important concepts are self-attention, multi-head attention, positional encoding, and layer normalization. Influential models include BERT, GPT, T5, and Vision Transformer. Research labs at Google, OpenAI, and Meta FAIR drive innovation."

run_seed_test "Sports: Formula 1" \
  "I follow Formula 1 racing. Key teams include Red Bull Racing, Ferrari, Mercedes, and McLaren. Top drivers are Max Verstappen, Lewis Hamilton, Charles Leclerc, and Lando Norris. I track the FIA regulations, DRS zones, and the ground effect aerodynamics rules. Key circuits include Monaco, Silverstone, Spa, and Suzuka. Adrian Newey is the legendary aerodynamicist."

run_seed_test "Legal: GDPR" \
  "I monitor GDPR enforcement across Europe. Key data protection authorities include the Irish DPC, French CNIL, German BfDI, and the EDPB. Major fines have been issued to Meta, Google, Amazon, and TikTok. I track the EU AI Act, Data Governance Act, and the Schrems II ruling. Important topics include data transfers, consent management, and the right to be forgotten."

echo ""
echo "Test complete."
