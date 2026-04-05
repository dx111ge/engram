/// Shared application state — wraps the Graph in an Arc<RwLock>.
///
/// Uses RwLock instead of Mutex so multiple readers can proceed concurrently.
/// Writes are deferred-checkpointed: mutations go to WAL + mmap immediately,
/// but the expensive disk flush happens on a background timer (every 5s) or
/// when explicitly requested.

use engram_core::Graph;
use engram_core::events::EventBus;
use engram_core::learning::rules::Rule;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::auth::{Session, UserStore};
use crate::secrets::SecretStore;
use std::collections::HashMap;

/// Cached hardware and embedder info for the /compute endpoint.
#[derive(Clone, serde::Serialize)]
pub struct ComputeInfo {
    pub cpu_cores: usize,
    pub has_avx2: bool,
    pub has_neon: bool,
    pub has_gpu: bool,
    pub gpu_name: String,
    pub gpu_backend: String,
    pub has_npu: bool,
    pub npu_name: String,
    pub dedicated_npu: Vec<String>,
    pub embedder_model: Option<String>,
    pub embedder_dim: Option<usize>,
    pub embedder_endpoint: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct KbEndpointConfig {
    pub name: String,
    pub url: String,
    #[serde(default = "default_auth_none")]
    pub auth_type: String,
    pub auth_secret_key: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub entity_link_template: Option<String>,
    pub relation_query_template: Option<String>,
    pub max_lookups_per_call: Option<u32>,
}

fn default_auth_none() -> String { "none".to_string() }
fn default_true() -> bool { true }

/// Runtime configuration for LLM, embedder, and pipeline settings.
/// Persisted to a `.brain.config` JSON sidecar file.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct EngineConfig {
    pub embed_endpoint: Option<String>,
    pub embed_model: Option<String>,
    pub llm_endpoint: Option<String>,
    pub llm_model: Option<String>,
    #[serde(skip_serializing)]
    pub llm_api_key: Option<String>,
    pub llm_temperature: Option<f32>,
    /// Whether the configured model is a thinking/reasoning model (e.g. DeepSeek-R1, o3-mini)
    pub llm_thinking: Option<bool>,
    /// Maximum context window of the configured LLM in tokens. Used to budget max_tokens
    /// for debate synthesis and other large prompts. Common values: 8192, 32768, 131072.
    pub llm_context_window: Option<u32>,
    pub pipeline_batch_size: Option<u32>,
    pub pipeline_workers: Option<u32>,
    pub pipeline_skip_stages: Option<Vec<String>>,
    /// NER provider: "builtin", "spacy", "gliner"
    pub ner_provider: Option<String>,
    /// NER model name (e.g. "en_core_web_sm" for spaCy)
    pub ner_model: Option<String>,
    /// NER endpoint URL (for external NER services)
    pub ner_endpoint: Option<String>,
    /// Relation extraction model name (e.g. "multilingual-MiniLMv2-L6-mnli-xnli")
    pub rel_model: Option<String>,
    /// Custom relation templates for NLI-based RE: { rel_type: hypothesis_template }
    pub relation_templates: Option<std::collections::HashMap<String, String>>,
    /// NLI relation extraction confidence threshold (0.0-1.0). Default: 0.9.
    pub rel_threshold: Option<f32>,
    /// Enable coreference resolution (pronoun -> canonical entity). Default: true.
    pub coreference_enabled: Option<bool>,
    /// Mesh enabled flag
    pub mesh_enabled: Option<bool>,
    /// Mesh topology: "star", "full", "ring"
    pub mesh_topology: Option<String>,
    /// Vector quantization enabled (int8). Defaults to true.
    pub quantization_enabled: Option<bool>,
    /// Knowledge base endpoints (SPARQL, etc.).
    pub kb_endpoints: Option<Vec<KbEndpointConfig>>,
    /// Web search provider: "duckduckgo", "brave", "searxng"
    pub web_search_provider: Option<String>,
    /// Web search API key (for Brave Search)
    pub web_search_api_key: Option<String>,
    /// Web search URL (for SearXNG self-hosted)
    pub web_search_url: Option<String>,
    /// Per-source-type initial trust overrides: { "web": 0.30, "x": 0.10, ... }
    /// Merged with built-in defaults. User can adjust via config API or UI.
    pub source_trust_defaults: Option<std::collections::HashMap<String, f32>>,
    /// Whether the onboarding wizard has been dismissed.
    #[serde(default)]
    pub wizard_dismissed: Option<bool>,
}

impl EngineConfig {
    /// Load config from a JSON file. Returns Default if file does not exist.
    pub fn load(path: &PathBuf) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to a JSON file.
    pub fn save(&self, path: &PathBuf) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Merge another config into this one (only overwrite fields that are Some in `other`).
    pub fn merge(&mut self, other: &EngineConfig) {
        if other.embed_endpoint.is_some() {
            self.embed_endpoint = other.embed_endpoint.clone();
        }
        if other.embed_model.is_some() {
            self.embed_model = other.embed_model.clone();
        }
        if other.llm_endpoint.is_some() {
            self.llm_endpoint = other.llm_endpoint.clone();
        }
        if other.llm_model.is_some() {
            self.llm_model = other.llm_model.clone();
        }
        if other.llm_api_key.is_some() {
            self.llm_api_key = other.llm_api_key.clone();
        }
        if other.llm_temperature.is_some() {
            self.llm_temperature = other.llm_temperature;
        }
        if other.llm_thinking.is_some() {
            self.llm_thinking = other.llm_thinking;
        }
        if other.llm_context_window.is_some() {
            self.llm_context_window = other.llm_context_window;
        }
        if other.pipeline_batch_size.is_some() {
            self.pipeline_batch_size = other.pipeline_batch_size;
        }
        if other.pipeline_workers.is_some() {
            self.pipeline_workers = other.pipeline_workers;
        }
        if other.pipeline_skip_stages.is_some() {
            self.pipeline_skip_stages = other.pipeline_skip_stages.clone();
        }
        if other.ner_provider.is_some() {
            self.ner_provider = other.ner_provider.clone();
        }
        if other.ner_model.is_some() {
            self.ner_model = other.ner_model.clone();
        }
        if other.ner_endpoint.is_some() {
            self.ner_endpoint = other.ner_endpoint.clone();
        }
        if other.rel_model.is_some() {
            self.rel_model = other.rel_model.clone();
        }
        if other.relation_templates.is_some() {
            self.relation_templates = other.relation_templates.clone();
        }
        if other.rel_threshold.is_some() {
            self.rel_threshold = other.rel_threshold;
        }
        if other.coreference_enabled.is_some() {
            self.coreference_enabled = other.coreference_enabled;
        }
        if other.mesh_enabled.is_some() {
            self.mesh_enabled = other.mesh_enabled;
        }
        if other.mesh_topology.is_some() {
            self.mesh_topology = other.mesh_topology.clone();
        }
        if other.quantization_enabled.is_some() {
            self.quantization_enabled = other.quantization_enabled;
        }
        if other.kb_endpoints.is_some() {
            self.kb_endpoints = other.kb_endpoints.clone();
        }
        if other.web_search_provider.is_some() {
            self.web_search_provider = other.web_search_provider.clone();
        }
        if other.web_search_api_key.is_some() {
            self.web_search_api_key = other.web_search_api_key.clone();
        }
        if other.web_search_url.is_some() {
            self.web_search_url = other.web_search_url.clone();
        }
        if other.source_trust_defaults.is_some() {
            self.source_trust_defaults = other.source_trust_defaults.clone();
        }
        if other.wizard_dismissed.is_some() {
            self.wizard_dismissed = other.wizard_dismissed;
        }
    }
}

/// Mesh state for knowledge mesh networking (optional, requires `mesh` feature).
#[cfg(feature = "mesh")]
#[derive(Clone)]
pub struct MeshState {
    /// Local node identity (ed25519 keypair)
    pub identity: Arc<engram_mesh::identity::Keypair>,
    /// Registered peers
    pub peers: Arc<RwLock<engram_mesh::peer::PeerRegistry>>,
    /// Audit trail for sync operations
    pub audit: Arc<RwLock<engram_mesh::audit::AuditLog>>,
    /// Path to `.brain.peers` sidecar file.
    pub peers_path: Option<PathBuf>,
    /// Path to `.brain.audit` sidecar file.
    pub audit_path: Option<PathBuf>,
}

/// A seed enrichment session: tracks state across multi-phase interactive seed flow.
#[derive(Clone, Debug)]
pub struct SeedSession {
    pub session_id: String,
    pub seed_text: String,
    pub area_of_interest: Option<String>,
    /// Entities extracted by NER (label, type, confidence).
    pub entities: Vec<SeedEntity>,
    /// Entity links to Wikidata (label, canonical, description, qid).
    pub entity_links: Vec<SeedEntityLink>,
    /// All items for human review: triples (node-edge-node) and standalone nodes.
    pub review_items: Vec<SeedReviewItem>,
    pub confirmed: bool,
    /// Enrichment status: "pending", "enriching", "complete", "error"
    pub status: String,
    /// Error message if status is "error"
    pub status_error: Option<String>,
}

/// An entity discovered during SPARQL property expansion (not in original NER).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SeedExpansionEntity {
    pub label: String,
    pub node_type: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SeedEntity {
    pub label: String,
    pub entity_type: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SeedEntityLink {
    pub label: String,
    pub canonical: String,
    pub description: String,
    pub qid: String,
}

/// Review tier for relation triage.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionTier {
    /// SPARQL >= 0.70 OR GLiNER2 >= 0.70 -- auto-accepted, pre-checked
    Confirmed,
    /// GLiNER2 0.50-0.70 -- quick confirm/reject
    Likely,
    /// GLiNER2 < 0.50 -- careful review needed
    Uncertain,
    /// GLiNER2 NO_RELATION -- human assigns type or skips
    NoRelation,
}

impl ConnectionTier {
    pub fn from_confidence(confidence: f32, is_sparql: bool) -> Self {
        if is_sparql && confidence >= 0.70 {
            Self::Confirmed
        } else if confidence >= 0.70 {
            Self::Confirmed
        } else if confidence >= 0.50 {
            Self::Likely
        } else if confidence > 0.0 {
            Self::Uncertain
        } else {
            Self::NoRelation
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SeedConnection {
    pub from: String,
    pub to: String,
    pub rel_type: String,
    pub source: String,
    pub confidence: f32,
    pub tier: ConnectionTier,
}

/// A single item in the merged seed review screen.
/// Either a triple (from--rel-->to) or a standalone node (from only).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SeedReviewItem {
    pub from: String,
    pub to: Option<String>,
    pub rel_type: Option<String>,
    pub source: String,
    pub confidence: f32,
    pub tier: ConnectionTier,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

/// An ingest review session: stores pipeline results for human review before committing.
#[derive(Clone, Debug)]
pub struct IngestSession {
    pub session_id: String,
    pub entities: Vec<IngestPreviewEntity>,
    pub relations: Vec<IngestPreviewRelation>,
    pub created_at: std::time::Instant,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IngestPreviewEntity {
    pub label: String,
    pub entity_type: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct IngestPreviewRelation {
    pub from: String,
    pub to: String,
    pub rel_type: String,
    pub confidence: f32,
    pub method: String,
    pub tier: ConnectionTier,
}

/// Thread-safe shared graph state for the HTTP server.
#[derive(Clone)]
pub struct AppState {
    /// Shared HTTP client -- reuse everywhere instead of creating new clients.
    /// reqwest::Client is Clone (Arc internally), supports connection pooling and TLS session reuse.
    pub http_client: reqwest::Client,
    pub graph: Arc<RwLock<Graph>>,
    pub compute: Arc<RwLock<ComputeInfo>>,
    /// Set to true when a write happens; cleared after checkpoint.
    pub dirty: Arc<AtomicBool>,
    /// Optional rule set for push-based inference triggers.
    /// When non-empty, rules are evaluated after store/relate/tell mutations.
    pub rules: Arc<RwLock<Vec<Rule>>>,
    /// Event bus for broadcasting graph change events to SSE subscribers.
    pub event_bus: Arc<EventBus>,
    /// Optional action engine for event-driven rules.
    #[cfg(feature = "actions")]
    pub action_engine: Arc<RwLock<engram_action::ActionEngine>>,
    /// Optional mesh networking state.
    #[cfg(feature = "mesh")]
    pub mesh: Option<MeshState>,
    /// Runtime configuration (LLM, embedder, pipeline settings).
    pub config: Arc<RwLock<EngineConfig>>,
    /// Path to the `.brain.config` sidecar file (None if not persisting).
    pub config_path: Option<PathBuf>,
    /// Assessment store (optional, requires `assess` feature).
    #[cfg(feature = "assess")]
    pub assessments: Arc<RwLock<engram_assess::AssessmentStore>>,
    /// Path to the `.brain.rules` sidecar file for action rules persistence.
    #[cfg(feature = "actions")]
    pub action_rules_path: Option<PathBuf>,
    /// Encrypted secrets store (API keys, auth tokens).
    /// Arc<RwLock<Option>> so it can be unlocked at runtime when admin logs in.
    pub secrets: Arc<RwLock<Option<SecretStore>>>,
    /// Path to the `.brain.secrets` sidecar file.
    pub secrets_path: Option<PathBuf>,
    /// User store (`.brain.users` sidecar).
    pub user_store: Arc<RwLock<UserStore>>,
    /// Active sessions (in-memory, lost on restart).
    pub sessions: Arc<RwLock<HashMap<String, Session>>>,
    /// Source registry for ingest pipeline (optional, requires `ingest` feature).
    #[cfg(feature = "ingest")]
    pub source_registry: Arc<engram_ingest::SourceRegistry>,
    /// Adaptive scheduler for source fetch intervals (optional, requires `ingest` feature).
    #[cfg(feature = "ingest")]
    pub scheduler: Arc<RwLock<engram_ingest::AdaptiveScheduler>>,
    /// Path to `.brain.schedules` sidecar file.
    #[cfg(feature = "ingest")]
    pub schedules_path: Option<PathBuf>,
    /// Cached NER backend (loaded once, invalidated on config change).
    #[cfg(feature = "ingest")]
    pub cached_ner: Arc<RwLock<Option<Arc<dyn engram_ingest::Extractor>>>>,
    /// Cached REL backend (loaded once, invalidated on config change).
    #[cfg(feature = "ingest")]
    pub cached_rel: Arc<RwLock<Option<Arc<dyn engram_ingest::RelationExtractor>>>>,
    /// Document content store for provenance tracking.
    pub doc_store: Arc<RwLock<engram_core::storage::doc_store::DocStore>>,
    /// Active seed enrichment sessions (interactive multi-phase flow).
    pub seed_sessions: Arc<RwLock<HashMap<String, SeedSession>>>,
    /// Active ingest review sessions (review=true mode).
    pub ingest_sessions: Arc<RwLock<HashMap<String, IngestSession>>>,
    /// Active multi-agent debate sessions (in-memory, 2h TTL).
    pub debate_sessions: Arc<RwLock<HashMap<String, crate::handlers::debate::DebateSession>>>,
}

impl AppState {
    pub fn new(graph: Graph) -> Self {
        let hw = engram_compute::planner::HardwareInfo::detect();
        let graph = Arc::new(RwLock::new(graph));
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(8)
            .build()
            .expect("failed to build shared HTTP client");
        AppState {
            http_client,
            #[cfg(feature = "actions")]
            action_engine: Arc::new(RwLock::new(engram_action::ActionEngine::new(graph.clone()))),
            graph,
            compute: Arc::new(RwLock::new(ComputeInfo {
                cpu_cores: hw.cpu_cores,
                has_avx2: hw.has_avx2,
                has_neon: hw.has_neon,
                has_gpu: hw.has_gpu,
                gpu_name: hw.gpu_name,
                gpu_backend: hw.gpu_backend,
                has_npu: hw.has_npu,
                npu_name: hw.npu_name,
                dedicated_npu: hw.dedicated_npu,
                embedder_model: None,
                embedder_dim: None,
                embedder_endpoint: None,
            })),
            dirty: Arc::new(AtomicBool::new(false)),
            rules: Arc::new(RwLock::new(Vec::new())),
            event_bus: Arc::new(EventBus::default()),
            #[cfg(feature = "mesh")]
            mesh: None,
            config: Arc::new(RwLock::new(EngineConfig::default())),
            config_path: None,
            #[cfg(feature = "actions")]
            action_rules_path: None,
            #[cfg(feature = "assess")]
            assessments: Arc::new(RwLock::new(engram_assess::AssessmentStore::new(PathBuf::new()))),
            secrets: Arc::new(RwLock::new(None)),
            secrets_path: None,
            user_store: Arc::new(RwLock::new(UserStore::empty())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "ingest")]
            source_registry: Arc::new(engram_ingest::SourceRegistry::new()),
            #[cfg(feature = "ingest")]
            scheduler: Arc::new(RwLock::new(engram_ingest::AdaptiveScheduler::default())),
            #[cfg(feature = "ingest")]
            schedules_path: None,
            #[cfg(feature = "ingest")]
            cached_ner: Arc::new(RwLock::new(None)),
            #[cfg(feature = "ingest")]
            cached_rel: Arc::new(RwLock::new(None)),
            doc_store: Arc::new(RwLock::new(
                engram_core::storage::doc_store::DocStore::empty()
            )),
            seed_sessions: Arc::new(RwLock::new(HashMap::new())),
            ingest_sessions: Arc::new(RwLock::new(HashMap::new())),
            debate_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Open the document content store for the given brain file path.
    pub fn open_doc_store(&mut self, brain_path: &std::path::Path) {
        match engram_core::storage::doc_store::DocStore::open(brain_path) {
            Ok(store) => {
                let count = store.entry_count();
                self.doc_store = Arc::new(RwLock::new(store));
                if count > 0 {
                    println!("DocStore: {} cached documents", count);
                }
            }
            Err(e) => {
                eprintln!("WARNING: DocStore failed to open: {e}");
            }
        }
    }

    /// Load config from a sidecar file and store the path for later saves.
    pub fn load_config(&mut self, path: PathBuf) {
        let cfg = EngineConfig::load(&path);
        self.config = Arc::new(RwLock::new(cfg));
        self.config_path = Some(path);
    }

    /// Persist current config to the sidecar file (if path is set).
    pub fn save_config(&self) -> std::io::Result<()> {
        if let Some(ref path) = self.config_path {
            let cfg = self.config.read().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "config lock poisoned")
            })?;
            cfg.save(path)
        } else {
            Ok(())
        }
    }

    /// Save scheduler state to sidecar file.
    #[cfg(feature = "ingest")]
    pub fn save_schedules(&self) {
        if let Some(ref path) = self.schedules_path {
            if let Ok(sched) = self.scheduler.read() {
                if let Err(e) = sched.save(path) {
                    tracing::warn!("failed to save schedules: {}", e);
                }
            }
        }
    }

    /// Load scheduler from sidecar file.
    #[cfg(feature = "ingest")]
    pub fn load_schedules(&mut self, path: PathBuf) {
        let sched = engram_ingest::AdaptiveScheduler::load(&path);
        self.scheduler = Arc::new(RwLock::new(sched));
        self.schedules_path = Some(path);
    }

    /// Enable mesh networking with a keypair and optional peer/audit paths.
    /// If sidecar files exist at the given paths, they are loaded on startup.
    #[cfg(feature = "mesh")]
    pub fn enable_mesh(&mut self, keypair: engram_mesh::identity::Keypair, peers_path: Option<PathBuf>, audit_path: Option<PathBuf>) {
        // Load existing peers if file exists
        let peers = if let Some(ref path) = peers_path {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(contents) => {
                        let reg: engram_mesh::peer::PeerRegistry = serde_json::from_str(&contents)
                            .unwrap_or_else(|_| engram_mesh::peer::PeerRegistry::new());
                        tracing::info!("loaded {} peers from {}", reg.peers.len(), path.display());
                        reg
                    }
                    Err(_) => engram_mesh::peer::PeerRegistry::new(),
                }
            } else {
                engram_mesh::peer::PeerRegistry::new()
            }
        } else {
            engram_mesh::peer::PeerRegistry::new()
        };

        // Load existing audit if file exists
        let audit = if let Some(ref path) = audit_path {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(contents) => {
                        let log: engram_mesh::audit::AuditLog = serde_json::from_str(&contents)
                            .unwrap_or_else(|_| engram_mesh::audit::AuditLog::new(10000));
                        tracing::info!("loaded {} audit entries from {}", log.len(), path.display());
                        log
                    }
                    Err(_) => engram_mesh::audit::AuditLog::new(10000),
                }
            } else {
                engram_mesh::audit::AuditLog::new(10000)
            }
        } else {
            engram_mesh::audit::AuditLog::new(10000)
        };

        self.mesh = Some(MeshState {
            identity: Arc::new(keypair),
            peers: Arc::new(RwLock::new(peers)),
            audit: Arc::new(RwLock::new(audit)),
            peers_path,
            audit_path,
        });
    }

    /// Persist mesh peer registry to its sidecar file.
    #[cfg(feature = "mesh")]
    pub fn save_mesh_peers(&self) {
        if let Some(ref mesh) = self.mesh {
            if let Some(ref path) = mesh.peers_path {
                if let Ok(peers) = mesh.peers.read() {
                    if let Ok(json) = serde_json::to_string_pretty(&*peers) {
                        if let Err(e) = std::fs::write(path, json) {
                            tracing::warn!("failed to save mesh peers: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Persist mesh audit log to its sidecar file.
    #[cfg(feature = "mesh")]
    pub fn save_mesh_audit(&self) {
        if let Some(ref mesh) = self.mesh {
            if let Some(ref path) = mesh.audit_path {
                if let Ok(audit) = mesh.audit.read() {
                    if let Ok(json) = serde_json::to_string_pretty(&*audit) {
                        if let Err(e) = std::fs::write(path, json) {
                            tracing::warn!("failed to save mesh audit: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Set embedder info for the /compute endpoint.
    pub fn set_embedder_info(&self, model: String, dim: usize, endpoint: String) {
        if let Ok(mut compute) = self.compute.write() {
            compute.embedder_model = Some(model);
            compute.embedder_dim = Some(dim);
            compute.embedder_endpoint = Some(endpoint);
        }
    }

    /// Mark the graph as dirty (needs checkpoint).
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// If dirty, acquire write lock and checkpoint. Returns true if flushed.
    pub fn checkpoint_if_dirty(&self) -> bool {
        let mut flushed = false;
        if self.dirty.swap(false, Ordering::AcqRel) {
            if let Ok(mut g) = self.graph.write() {
                let _ = g.checkpoint();
                flushed = true;
            }
        }
        // Also checkpoint assessment sidecar
        #[cfg(feature = "assess")]
        {
            if let Ok(store) = self.assessments.read() {
                if store.checkpoint_if_dirty() {
                    tracing::debug!("assessment checkpoint complete");
                    flushed = true;
                }
            }
        }
        // Also checkpoint secrets sidecar
        if let Ok(guard) = self.secrets.read() {
            if let Some(ref s) = *guard {
                if s.checkpoint_if_dirty() {
                    tracing::debug!("secrets checkpoint complete");
                    flushed = true;
                }
            }
        }
        // Also checkpoint mesh sidecars (peers + audit)
        #[cfg(feature = "mesh")]
        {
            self.save_mesh_peers();
            self.save_mesh_audit();
        }
        // Also checkpoint schedules sidecar
        #[cfg(feature = "ingest")]
        {
            self.save_schedules();
        }
        flushed
    }

    /// Load assessment store from a sidecar file.
    #[cfg(feature = "assess")]
    pub fn load_assessments(&mut self, path: PathBuf) {
        let store = engram_assess::AssessmentStore::load(path);
        self.assessments = Arc::new(RwLock::new(store));
    }

    /// Save action rules to sidecar file.
    #[cfg(feature = "actions")]
    pub fn save_action_rules(&self) {
        if let Some(ref path) = self.action_rules_path {
            if let Ok(engine) = self.action_engine.read() {
                let rules: Vec<&engram_action::ActionRule> = engine.list_rules()
                    .iter()
                    .filter_map(|id| engine.get_rule(id))
                    .collect();
                if let Ok(json) = serde_json::to_string_pretty(&rules) {
                    if let Err(e) = std::fs::write(path, json) {
                        tracing::warn!("failed to save action rules: {}", e);
                    }
                }
            }
        }
    }

    /// Load action rules from sidecar file.
    #[cfg(feature = "actions")]
    pub fn load_action_rules_from_file(&self) {
        if let Some(ref path) = self.action_rules_path {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(contents) => {
                        match serde_json::from_str::<Vec<engram_action::ActionRule>>(&contents) {
                            Ok(rules) => {
                                let count = rules.len();
                                if let Ok(mut engine) = self.action_engine.write() {
                                    engine.load_rules(rules);
                                }
                                if count > 0 {
                                    tracing::info!("loaded {} action rules from {}", count, path.display());
                                }
                            }
                            Err(e) => tracing::warn!("failed to parse action rules: {}", e),
                        }
                    }
                    Err(e) => tracing::warn!("failed to read action rules: {}", e),
                }
            }
        }
    }

    /// Fire push-based rules asynchronously if any are loaded.
    /// Called after store/relate/tell mutations.
    pub fn fire_rules_async(&self) {
        let rules = self.rules.clone();
        let graph = self.graph.clone();
        let dirty = self.dirty.clone();

        tokio::spawn(async move {
            let rules_guard = match rules.read() {
                Ok(r) => r,
                Err(_) => return,
            };
            if rules_guard.is_empty() {
                return;
            }
            let rules_snapshot: Vec<Rule> = rules_guard.clone();
            drop(rules_guard);

            let mut g = match graph.write() {
                Ok(g) => g,
                Err(_) => return,
            };
            let prov = engram_core::graph::Provenance {
                source_type: engram_core::graph::SourceType::Derived,
                source_id: "rules-trigger".to_string(),
            };
            match g.forward_chain(&rules_snapshot, &prov) {
                Ok(result) => {
                    if result.edges_created > 0 || result.flags_raised > 0 {
                        dirty.store(true, Ordering::Release);
                        tracing::info!(
                            "push rules: {} fired, {} edges, {} flags",
                            result.rules_fired,
                            result.edges_created,
                            result.flags_raised,
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("push rule evaluation failed: {e}");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_config_serde_round_trip() {
        let mut config = EngineConfig::default();
        config.ner_provider = Some("gliner".into());
        config.ner_model = Some("knowledgator/gliner-x-small".into());
        config.rel_model = Some("multilingual-MiniLMv2-L6-mnli-xnli".into());
        config.rel_threshold = Some(0.85);
        config.coreference_enabled = Some(true);
        config.relation_templates = Some({
            let mut m = HashMap::new();
            m.insert("works_at".into(), "{head} works at {tail}".into());
            m.insert("born_in".into(), "{head} was born in {tail}".into());
            m
        });

        let json = serde_json::to_string(&config).unwrap();
        let parsed: EngineConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.ner_provider, Some("gliner".into()));
        assert_eq!(parsed.ner_model, Some("knowledgator/gliner-x-small".into()));
        assert_eq!(parsed.rel_model, Some("multilingual-MiniLMv2-L6-mnli-xnli".into()));
        assert_eq!(parsed.rel_threshold, Some(0.85));
        assert_eq!(parsed.coreference_enabled, Some(true));
        assert_eq!(parsed.relation_templates.as_ref().unwrap().len(), 2);
        assert_eq!(
            parsed.relation_templates.as_ref().unwrap().get("works_at").unwrap(),
            "{head} works at {tail}"
        );
    }

    #[test]
    fn engine_config_merge_new_fields() {
        let mut base = EngineConfig::default();
        let mut overlay = EngineConfig::default();
        overlay.relation_templates = Some({
            let mut m = HashMap::new();
            m.insert("test".into(), "{head} tests {tail}".into());
            m
        });
        overlay.coreference_enabled = Some(false);

        base.merge(&overlay);
        assert_eq!(base.relation_templates.as_ref().unwrap().len(), 1);
        assert_eq!(base.coreference_enabled, Some(false));
    }

    #[test]
    fn engine_config_merge_rel_threshold() {
        let mut base = EngineConfig::default();
        assert!(base.rel_threshold.is_none());

        let mut overlay = EngineConfig::default();
        overlay.rel_threshold = Some(0.85);
        base.merge(&overlay);
        assert_eq!(base.rel_threshold, Some(0.85));

        // Merge without rel_threshold should not overwrite
        let empty_overlay = EngineConfig::default();
        base.merge(&empty_overlay);
        assert_eq!(base.rel_threshold, Some(0.85));
    }

    #[test]
    fn engine_config_rel_threshold_default() {
        let config = EngineConfig::default();
        // Default is None; callers should use unwrap_or(0.9)
        assert!(config.rel_threshold.is_none());
        assert_eq!(config.rel_threshold.unwrap_or(0.9), 0.9);
    }
}
