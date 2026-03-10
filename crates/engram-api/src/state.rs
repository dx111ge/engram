/// Shared application state — wraps the Graph in an Arc<RwLock>.
///
/// Uses RwLock instead of Mutex so multiple readers can proceed concurrently.
/// Writes are deferred-checkpointed: mutations go to WAL + mmap immediately,
/// but the expensive disk flush happens on a background timer (every 5s) or
/// when explicitly requested.

use engram_core::Graph;
use engram_core::learning::rules::Rule;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

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
}

/// Thread-safe shared graph state for the HTTP server.
#[derive(Clone)]
pub struct AppState {
    pub graph: Arc<RwLock<Graph>>,
    pub compute: ComputeInfo,
    /// Set to true when a write happens; cleared after checkpoint.
    pub dirty: Arc<AtomicBool>,
    /// Optional rule set for push-based inference triggers.
    /// When non-empty, rules are evaluated after store/relate/tell mutations.
    pub rules: Arc<RwLock<Vec<Rule>>>,
    /// Optional action engine for event-driven rules.
    #[cfg(feature = "actions")]
    pub action_engine: Arc<RwLock<engram_action::ActionEngine>>,
    /// Optional mesh networking state.
    #[cfg(feature = "mesh")]
    pub mesh: Option<MeshState>,
}

impl AppState {
    pub fn new(graph: Graph) -> Self {
        let hw = engram_compute::planner::HardwareInfo::detect();
        let graph = Arc::new(RwLock::new(graph));
        AppState {
            #[cfg(feature = "actions")]
            action_engine: Arc::new(RwLock::new(engram_action::ActionEngine::new(graph.clone()))),
            graph,
            compute: ComputeInfo {
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
            },
            dirty: Arc::new(AtomicBool::new(false)),
            rules: Arc::new(RwLock::new(Vec::new())),
            #[cfg(feature = "mesh")]
            mesh: None,
        }
    }

    /// Enable mesh networking with a keypair and optional peer/audit paths.
    #[cfg(feature = "mesh")]
    pub fn enable_mesh(&mut self, keypair: engram_mesh::identity::Keypair) {
        self.mesh = Some(MeshState {
            identity: Arc::new(keypair),
            peers: Arc::new(RwLock::new(engram_mesh::peer::PeerRegistry::new())),
            audit: Arc::new(RwLock::new(engram_mesh::audit::AuditLog::new(10000))),
        });
    }

    /// Set embedder info for the /compute endpoint.
    pub fn set_embedder_info(&mut self, model: String, dim: usize, endpoint: String) {
        let compute = &mut self.compute;
        compute.embedder_model = Some(model);
        compute.embedder_dim = Some(dim);
        compute.embedder_endpoint = Some(endpoint);
    }

    /// Mark the graph as dirty (needs checkpoint).
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// If dirty, acquire write lock and checkpoint. Returns true if flushed.
    pub fn checkpoint_if_dirty(&self) -> bool {
        if self.dirty.swap(false, Ordering::AcqRel) {
            if let Ok(mut g) = self.graph.write() {
                let _ = g.checkpoint();
                return true;
            }
        }
        false
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
