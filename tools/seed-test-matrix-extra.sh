#!/bin/bash
# Extra Seed Enrichment Tests: outside-world-knowledge, codestacks, edge cases, niche domains

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

  printf "| %-38s | %5s | %5s | %6s | %6s | %7s |\n" "$name" "$facts" "$rels" "$new_nodes" "$new_edges" "${ms}ms"
}

echo ""
echo "## Extended Test Matrix: Edge Cases & Niche Domains"
echo ""
printf "| %-38s | %5s | %5s | %6s | %6s | %7s |\n" "Topic" "Facts" "Rels" "+Nodes" "+Edges" "Time"
printf "| %-38s | %5s | %5s | %6s | %6s | %7s |\n" "--------------------------------------" "-----" "-----" "------" "------" "-------"

echo "### Outside World Knowledge (private/internal)"

run_seed_test "Internal: Project Team" \
  "Our project team consists of Alice Mueller as lead developer, Bob Schmidt as DevOps engineer, and Clara Weber as product owner. We build the Phoenix platform using Rust and React. Our sprint velocity is 34 points. We report to CTO Marcus Braun. The project deadline is Q3 2026."

run_seed_test "Internal: Office Network" \
  "Our office network has three VLANs: VLAN10 for engineering on 10.10.10.0/24, VLAN20 for management on 10.10.20.0/24, VLAN30 for guest WiFi on 10.10.30.0/24. Core switch is a Cisco Catalyst 9300. Firewall is Palo Alto PA-850. Domain controller is dc01.acme.local running Windows Server 2022. DNS servers: 10.10.10.5 and 10.10.10.6."

run_seed_test "Internal: Meeting Notes" \
  "In the Q1 review meeting, CFO Sarah Klein reported 12% revenue growth. CTO Marcus Braun announced migration from Azure to AWS by September. VP Sales Thomas Fischer closed the Daimler deal worth 2.3M EUR. HR director Lisa Hoffman mentioned hiring 15 engineers in Munich. CEO Wolfgang Berger approved the Singapore expansion."

run_seed_test "Personal: Family Tree" \
  "My grandfather Hans Mueller was born in Hamburg in 1935 and married Ingrid Petersen from Kiel. They had three children: my father Peter born 1960, uncle Thomas born 1962, and aunt Monika born 1965. Peter married my mother Sabine Krause from Munich. I have one sister named Julia, born 1992. Uncle Thomas lives in Vienna with his wife Eva."

run_seed_test "Personal: Travel Planning" \
  "Planning a Japan trip in April 2026. Tokyo: stay at Park Hyatt Shinjuku, visit Senso-ji, Meiji Shrine, Tsukiji Market. Take Shinkansen to Kyoto for Fushimi Inari, Kinkaku-ji, Arashiyama bamboo grove. Day trip to Nara for deer park and Todai-ji. Osaka: Dotonbori street food, Osaka Castle. Budget: 5000 EUR for 14 days. Rail pass: JR Pass 21 days."

run_seed_test "Personal: Health Tracking" \
  "I track my health metrics. Resting heart rate: 58 BPM measured with Apple Watch Ultra 2. Blood pressure: 125/82 monitored with Withings BPM Connect. Weight: 78kg tracked on Withings Body+. Supplements: Vitamin D3, Omega-3, Magnesium glycinate. Sleep: average 7.2 hours tracked by AutoSleep app. Weekly exercise: 3x running, 2x strength training."

echo ""
echo "### Codestack Understanding"

run_seed_test "Stack: React + Next.js" \
  "Our frontend stack: Next.js 14 with App Router, React 18 with Server Components, TypeScript 5.3. State: Zustand for client, TanStack Query for server state. UI: shadcn/ui components, Radix primitives, Tailwind CSS 3.4. Forms: React Hook Form with Zod validation. Auth: NextAuth.js v5 with OAuth providers. Testing: Jest, React Testing Library, Cypress E2E."

run_seed_test "Stack: Spring Boot Microservices" \
  "Our Java microservice architecture: Spring Boot 3.2 with Spring Cloud Gateway. Service discovery: Consul. Messaging: Apache Kafka with Spring Cloud Stream. Database: PostgreSQL with Flyway migrations, Redis for caching. Observability: Micrometer metrics, Zipkin tracing, ELK stack logging. Security: Spring Security with Keycloak OIDC. Build: Maven, JUnit 5, Testcontainers."

run_seed_test "Stack: Python ML Pipeline" \
  "Our ML pipeline: PyTorch 2.1 for model training, Hugging Face Transformers for NLP. Experiment tracking: MLflow and Weights & Biases. Feature store: Feast on Redis. Model serving: Triton Inference Server behind FastAPI gateway. Data: DVC for versioning, Delta Lake on S3. Infrastructure: Kubernetes with KubeFlow, NVIDIA A100 GPUs. CI: GitHub Actions with DVC pipelines."

run_seed_test "Stack: Mobile Cross-Platform" \
  "Our mobile stack: Flutter 3.16 with Dart, BLoC pattern for state management. Backend: Firebase with Firestore, Cloud Functions, Authentication. CI/CD: Codemagic for builds, Firebase App Distribution for beta. Testing: widget tests, integration tests with patrol. Analytics: Firebase Analytics, Crashlytics. Push: Firebase Cloud Messaging. Storage: Hive for local, Cloud Storage for media."

run_seed_test "Stack: Rust Systems" \
  "Our systems stack: Rust with tokio async runtime, axum for HTTP, tonic for gRPC. Storage: custom mmap with WAL, sled for metadata. Networking: quinn for QUIC, rustls for TLS. Serialization: serde with bincode and JSON. Observability: tracing with jaeger exporter, metrics with prometheus. Build: cargo workspace, cross-compilation for linux-musl. CI: GitHub Actions, cargo-deny for audits."

run_seed_test "Stack: Infrastructure as Code" \
  "Our IaC stack: Terraform 1.6 with OpenTofu as alternative. Cloud: AWS with multi-account Organization. Modules: custom VPC, EKS, RDS, S3 modules. State: S3 backend with DynamoDB locking. Secrets: AWS Secrets Manager, external-secrets operator in K8s. Policy: OPA Gatekeeper, Checkov for scanning. Drift: Spacelift for drift detection. DNS: Route53 with external-dns controller."

echo ""
echo "### Niche & Specialized Domains"

run_seed_test "Wine: Sommelier" \
  "I study wines from Burgundy and Bordeaux. Key appellations: Gevrey-Chambertin, Vosne-Romanee, Pommard in Burgundy. Bordeaux: Pauillac, Saint-Emilion, Margaux. Producers: Domaine de la Romanee-Conti, Domaine Leroy, Chateau Latour, Chateau Margaux. Grape varieties: Pinot Noir, Chardonnay, Cabernet Sauvignon, Merlot. Vintage ratings from Robert Parker and Jancis Robinson."

run_seed_test "Music: Jazz Collection" \
  "I collect jazz vinyl records. Essential albums: Kind of Blue by Miles Davis, A Love Supreme by John Coltrane, Maiden Voyage by Herbie Hancock, The Shape of Jazz to Come by Ornette Coleman. Labels: Blue Note, Prestige, Impulse, ECM Records. Players: Thelonious Monk, Bill Evans, Wayne Shorter, Pat Metheny. I buy from Discogs and local shops."

run_seed_test "Architecture: Modernism" \
  "I study modernist architecture. Key architects: Le Corbusier, Mies van der Rohe, Frank Lloyd Wright, Alvar Aalto, Oscar Niemeyer. Buildings: Villa Savoye, Farnsworth House, Fallingwater, Brasilia Cathedral. Movements: Bauhaus, International Style, Brutalism. Concepts: pilotis, free plan, ribbon windows, roof garden. Schools: Bauhaus Dessau, IIT Chicago."

run_seed_test "Cooking: Japanese Cuisine" \
  "I study Japanese cooking techniques. Styles: kaiseki, izakaya, washoku. Key ingredients: dashi from kombu and katsuobushi, mirin, sake, miso. Techniques: sous vide for onsen tamago, charcoal yakitori, fermentation for nukazuke. Knife types: yanagiba for sashimi, deba for fish, usuba for vegetables. Rice varieties: Koshihikari, Akitakomachi. Tea ceremony: matcha, sencha."

run_seed_test "Aviation: Private Pilot" \
  "I'm training for a private pilot license PPL(A). Aircraft: Cessna 172 Skyhawk for training, Piper PA-28 Cherokee. Instruments: altimeter, attitude indicator, heading indicator, VOR navigation. Procedures: preflight checklist, crosswind landing, go-around. Authorities: EASA, LBA in Germany. Airfields: EDFE Egelsbach, EDDF Frankfurt. Weather: METAR, TAF, GAFOR."

echo ""
echo "### Summary"
FINAL=$(curl -s http://127.0.0.1:3030/stats -H "$AUTH")
echo "Total graph after all tests: $FINAL"
echo ""
echo "Test complete."
