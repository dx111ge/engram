#!/bin/bash
# Extended Seed Enrichment Test Matrix
# Covers: professional, personal, academic, hobby, coding, finance domains

TOKEN=$(curl -s -X POST http://127.0.0.1:3030/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"12Buchstaben!!"}' | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
AUTH="Authorization: Bearer $TOKEN"

run_seed_test() {
  local name="$1"
  local text="$2"

  local before=$(curl -s http://127.0.0.1:3030/stats -H "$AUTH")
  local nodes_before=$(echo "$before" | grep -o '"nodes":[0-9]*' | cut -d: -f2)
  local edges_before=$(echo "$before" | grep -o '"edges":[0-9]*' | cut -d: -f2)

  local result=$(curl -s -X POST http://127.0.0.1:3030/ingest \
    -H "$AUTH" \
    -H "Content-Type: application/json" \
    -d "{\"items\":[\"$(echo "$text" | sed 's/"/\\"/g')\"],\"source\":\"seed-test\"}" \
    --max-time 120)

  local facts=$(echo "$result" | grep -o '"facts_stored":[0-9]*' | cut -d: -f2)
  local rels=$(echo "$result" | grep -o '"relations_created":[0-9]*' | cut -d: -f2)
  local ms=$(echo "$result" | grep -o '"duration_ms":[0-9]*' | cut -d: -f2)

  local after=$(curl -s http://127.0.0.1:3030/stats -H "$AUTH")
  local nodes_after=$(echo "$after" | grep -o '"nodes":[0-9]*' | cut -d: -f2)
  local edges_after=$(echo "$after" | grep -o '"edges":[0-9]*' | cut -d: -f2)

  local new_nodes=$((nodes_after - nodes_before))
  local new_edges=$((edges_after - edges_before))

  printf "| %-35s | %5s | %5s | %6s | %6s | %7s |\n" "$name" "$facts" "$rels" "$new_nodes" "$new_edges" "${ms}ms"
}

echo ""
echo "## Extended Seed Enrichment Test Matrix"
echo ""
printf "| %-35s | %5s | %5s | %6s | %6s | %7s |\n" "Topic" "Facts" "Rels" "+Nodes" "+Edges" "Time"
printf "| %-35s | %5s | %5s | %6s | %6s | %7s |\n" "-----------------------------------" "-----" "-----" "------" "------" "-------"

echo "### Professional / Intelligence"

run_seed_test "Geopolitics: Russia-Ukraine" \
  "I track the Russia-Ukraine war. Key figures include Putin, Zelensky, Macron, Stoltenberg, and Scholz. Military equipment: HIMARS, Leopard 2, F-16, Bayraktar drones. Organizations: NATO, EU, UN, Wagner Group. Locations: Kyiv, Moscow, Crimea, Donbas."

run_seed_test "Cybersecurity: EU CISO" \
  "I'm a European CISO focused on cybersecurity. Key agencies: BSI, ANSSI, CCDCOE Tallinn. Threat actors: APT28, APT29, Lazarus Group. Regulations: NIS2, Cyber Resilience Act. Priority: energy sector, Nord Stream infrastructure."

run_seed_test "AI/Tech: AI Companies" \
  "I research AI companies. OpenAI with GPT-4, Google DeepMind with Gemini, Anthropic with Claude, Meta AI with LLaMA. Key people: Ilya Sutskever, Demis Hassabis, Yann LeCun. VCs: Sequoia, Andreessen Horowitz."

run_seed_test "Crime: Drug Trafficking" \
  "I investigate drug trafficking in Latin America. Cartels: Sinaloa Cartel, CJNG, Gulf Cartel. Figures: El Mayo Zambada, El Chapo legacy. DEA operations, cryptocurrency laundering, fentanyl crisis. Corridors: Mexico, Colombia, Guatemala."

echo ""
echo "### Finance & Business"

run_seed_test "Finance: ServiceNow Stock" \
  "I'm a stock analyst tracking ServiceNow. Competitors: Salesforce, BMC Software, Atlassian, Freshworks. CEO Bill McDermott, headquartered Santa Clara. Products: ITSM, CMDB, Now Platform. I monitor earnings, cloud revenue, enterprise AI."

run_seed_test "Finance: ETF Portfolio" \
  "I manage a personal ETF portfolio. Core holdings: Vanguard FTSE All-World UCITS ETF, iShares Core MSCI World, Xtrackers MSCI Emerging Markets. I track TER ratios, tracking difference, and rebalancing. Brokers: Trade Republic, Scalable Capital, Interactive Brokers. Key indices: MSCI World, S&P 500, FTSE 100."

run_seed_test "Finance: Crypto DeFi" \
  "I follow DeFi and cryptocurrency markets. Key protocols: Uniswap, Aave, MakerDAO, Lido, Curve Finance. Blockchains: Ethereum, Solana, Avalanche, Arbitrum. I track TVL, yield farming, liquid staking. Key people: Vitalik Buterin, Anatoly Yakovenko. Regulatory bodies: SEC, CFTC, MiCA regulation."

run_seed_test "Business: Startup Ecosystem" \
  "I track the Berlin startup ecosystem. Notable companies: N26, Delivery Hero, Zalando, Wefox, Personio. Key investors: Project A, HV Capital, Cherry Ventures. Accelerators: Techstars, Entrepreneur First. Ecosystem players: Berlin Partner, German Startups Association. Key people: Oliver Samwer, Valentin Stalf."

echo ""
echo "### Personal / Hobbies"

run_seed_test "Hobby: Photography" \
  "I'm an enthusiast photographer. I shoot with Sony Alpha A7 IV and Fujifilm X-T5. Favorite lenses: Sony 24-70mm GM II, Fuji 56mm f/1.2, Sigma 35mm Art. I use Lightroom and Capture One for editing. Genres: street photography, landscape, astrophotography. Influences: Henri Cartier-Bresson, Ansel Adams, Sebastiao Salgado."

run_seed_test "Hobby: Board Games" \
  "I collect and play modern board games. Favorites: Gloomhaven, Terraforming Mars, Wingspan, Spirit Island, Ark Nova. Publishers: Stonemaier Games, Cephalofair, Czech Games Edition. I follow reviewers: Shut Up & Sit Down, Dice Tower, No Pun Included. I play on Board Game Arena and Tabletop Simulator."

run_seed_test "Hobby: Coffee" \
  "I'm a specialty coffee enthusiast. Equipment: Lelit Bianca espresso machine, Niche Zero grinder, Hario V60. I buy from roasters: Square Mile, The Barn, Friedhats, April Coffee. Origins I track: Ethiopia Yirgacheffe, Colombia Huila, Guatemala Antigua, Kenya Nyeri. SCA scoring, light roast profiles, water chemistry with Third Wave Water."

run_seed_test "Hobby: Cycling" \
  "I'm a road cycling enthusiast. My bike: Canyon Aeroad CF SL with Shimano Ultegra Di2. I train with Wahoo Kickr and Zwift. Key races I follow: Tour de France, Giro d'Italia, Paris-Roubaix. Riders: Tadej Pogacar, Jonas Vingegaard, Remco Evenepoel, Mathieu van der Poel. Teams: UAE Team Emirates, Visma-Lease a Bike."

run_seed_test "Hobby: Gardening" \
  "I maintain a permaculture garden. Key plants: tomatoes, basil, lavender, rosemary, blueberries, raised bed potatoes. Companion planting with marigolds and nasturtiums. Composting with bokashi and worm bin. Seed suppliers: Bingenheimer Saatgut, Dreschflegel, ReinSaat. Tools: Felco pruners, Gardena irrigation, Hochbeet from Garantia."

run_seed_test "Sport: Trail Running" \
  "I do trail running and ultramarathons. Races: UTMB, Western States 100, Lavaredo Ultra Trail, Eiger Ultra Trail. Shoes: Salomon S/Lab Ultra 3, Hoka Speedgoat 5, La Sportiva Bushido. Nutrition: Maurten, Tailwind, SiS gels. Training with Coros Pace 3 watch. Athletes: Kilian Jornet, Courtney Dauwalter, Jim Walmsley."

run_seed_test "Sport: Chess" \
  "I study chess competitively. I play on Lichess and Chess.com, rated around 1800 Elo. Openings I play: Sicilian Najdorf, Queen's Gambit Declined, Caro-Kann. I study games by Magnus Carlsen, Bobby Fischer, Mikhail Tal, and Garry Kasparov. Training tools: ChessBase, Stockfish engine, Chessable courses. Key tournaments: World Championship, Candidates, Tata Steel."

echo ""
echo "### Academic & Research"

run_seed_test "Academic: Transformers" \
  "I study transformer architecture in ML. Key paper: Attention Is All You Need by Vaswani et al. at Google Brain. Concepts: self-attention, multi-head attention, positional encoding, layer normalization. Models: BERT, GPT, T5, Vision Transformer. Labs: Google Brain, OpenAI, Meta FAIR."

run_seed_test "Academic: Quantum Computing" \
  "I research quantum computing. Key companies: IBM with Condor processor, Google with Sycamore, IonQ with trapped ions, Rigetti. Concepts: qubits, quantum entanglement, error correction, quantum supremacy. Programming: Qiskit, Cirq, PennyLane. Researchers: John Preskill, Peter Shor, Scott Aaronson."

run_seed_test "Academic: Neuroscience" \
  "I study computational neuroscience. Key topics: neural coding, spike timing dependent plasticity, hippocampal place cells, grid cells. Tools: NEURON simulator, Brian2, Allen Brain Atlas. Researchers: Karl Friston, Gyorgy Buzsaki, Eve Marder. Theories: free energy principle, predictive coding, Bayesian brain hypothesis."

run_seed_test "Academic: Climate Science" \
  "I research climate science and modeling. Key models: CMIP6, ERA5 reanalysis, IPCC AR6. Organizations: IPCC, WMO, NOAA, Copernicus Climate Service. Concepts: radiative forcing, climate sensitivity, tipping points, AMOC slowdown. Tools: Python xarray, CDO, CESM. Key scientists: Michael Mann, James Hansen, Katharine Hayhoe."

echo ""
echo "### Software Development"

run_seed_test "Code: Rust Ecosystem" \
  "I develop in Rust. Key crates: tokio for async, serde for serialization, axum for web, sqlx for databases, tracing for observability. Tools: cargo, clippy, rustfmt, miri. I follow the Rust blog, This Week in Rust. Key people: Steve Klabnik, Jon Gjengset. Concepts: ownership, borrowing, lifetimes, async/await, traits."

run_seed_test "Code: DevOps/Platform" \
  "I work on DevOps and platform engineering. Tools: Kubernetes, ArgoCD, Terraform, Pulumi, GitHub Actions. Monitoring: Prometheus, Grafana, OpenTelemetry, Jaeger. Cloud: AWS EKS, GCP GKE, Azure AKS. Concepts: GitOps, service mesh with Istio, SRE practices. Key projects: CNCF, Linux Foundation."

run_seed_test "Code: Full-Stack TypeScript" \
  "I build full-stack TypeScript applications. Frontend: Next.js, React, TailwindCSS, shadcn/ui. Backend: tRPC, Prisma, Drizzle ORM. Database: PostgreSQL, Redis, Supabase. Deployment: Vercel, Cloudflare Workers. Testing: Vitest, Playwright. Package management: pnpm, Turborepo monorepos."

run_seed_test "Code: Data Engineering" \
  "I do data engineering with Python. Tools: Apache Spark, dbt, Airflow, Dagster. Warehouses: Snowflake, BigQuery, DuckDB. Formats: Parquet, Delta Lake, Apache Iceberg. Python stack: polars, pandas, SQLAlchemy. Orchestration with Prefect and Temporal. Data quality: Great Expectations, Soda."

echo ""
echo "### ITSM / ESM / Enterprise"

run_seed_test "ITSM: Incident Management" \
  "I manage IT incidents for a large enterprise. We use ServiceNow ITSM with CMDB, Change Management, and Problem Management. Integration with PagerDuty, Splunk, and Datadog for alerting. ITIL v4 framework guides our processes. Key metrics: MTTR, MTBF, SLA compliance. Teams use Slack and Microsoft Teams for war rooms. Infrastructure: VMware, AWS, Azure hybrid cloud."

run_seed_test "ESM: HR Service Delivery" \
  "I lead HR service delivery transformation using Enterprise Service Management. Platform: ServiceNow HRSD with Employee Center. Processes: onboarding workflows, benefits administration, case management. Integration with Workday, SAP SuccessFactors, ADP. Self-service portal with virtual agent chatbot. Compliance: GDPR employee data, SOX controls. Analytics with Performance Analytics dashboards."

run_seed_test "ITSM: CMDB & Asset Mgmt" \
  "I maintain the CMDB and IT asset management. Discovery tools: ServiceNow Discovery, Qualys, Flexera. CI types: servers, applications, databases, network devices, cloud resources. Relationships: runs on, depends on, hosted on. Asset lifecycle: procurement, deployment, maintenance, retirement. Integration with Jira Service Management and BMC Helix."

run_seed_test "ESM: IT Financial Mgmt" \
  "I handle IT financial management and FinOps. Tools: ServiceNow ITFM, Apptio, CloudHealth by VMware, AWS Cost Explorer. Concepts: cost allocation, showback, chargeback, unit economics. Cloud optimization: reserved instances, spot instances, rightsizing. Budget tracking across AWS, Azure, GCP. TBM framework for technology business management."

echo ""
echo "### Summary"
FINAL=$(curl -s http://127.0.0.1:3030/stats -H "$AUTH")
echo "Total graph: $FINAL"
echo ""
echo "Test complete."
