/// Mesh transport layer — HTTP endpoints for knowledge mesh sync.
///
/// Wires engram-mesh's sync engine, peer registry, and audit trail
/// to HTTP endpoints so peers can exchange knowledge over the network.
///
/// Endpoints:
/// - GET  /mesh/heartbeat   — return local heartbeat (bloom filter digest)
/// - POST /mesh/sync        — accept SyncRequest, return SyncResponse
/// - POST /mesh/receive     — accept SyncResponse, process and store facts
/// - GET  /mesh/peers       — list registered peers
/// - POST /mesh/peers       — register a new peer
/// - DELETE /mesh/peers/{key} — remove a peer
/// - GET  /mesh/audit       — recent audit entries

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use engram_mesh::conflict::Resolution;
use engram_mesh::gossip::{self, SyncFact, SyncEdge, SyncRequest, SyncResponse};
use engram_mesh::peer::{PeerConfig, SyncPolicy};
use engram_mesh::sync;

use crate::state::AppState;
use crate::types::ErrorResponse;

type MeshResult<T> = std::result::Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

fn mesh_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg.into() }))
}

// ── GET /mesh/heartbeat ──

pub async fn heartbeat(
    State(state): State<AppState>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;

    let g = state.graph.read()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "graph read lock poisoned"))?;

    // Collect all active node labels for the bloom filter
    let nodes = g.all_nodes()
        .map_err(|e| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let labels: Vec<String> = nodes.iter().map(|n| n.label.clone()).collect();
    let (node_count, _) = g.stats();

    let hb = gossip::build_heartbeat(
        &mesh.identity.public,
        &labels,
        &[], // topic subscriptions
        node_count,
        now_millis(),
    );

    Ok(Json(serde_json::to_value(&hb).unwrap_or_default()))
}

// ── POST /mesh/sync — Respond to a SyncRequest from a peer ──

pub async fn serve_sync(
    State(state): State<AppState>,
    Json(req): Json<SyncRequest>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;

    // Look up the requesting peer
    let registry = mesh.peers.read()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "peer registry lock poisoned"))?;
    let peer = registry.get(&req.sender)
        .ok_or_else(|| mesh_err(StatusCode::FORBIDDEN, format!("unknown peer: {}", req.sender.to_hex())))?;
    if !peer.approved {
        return Err(mesh_err(StatusCode::FORBIDDEN, "peer not approved"));
    }
    let share_policy = peer.share_policy.clone();
    drop(registry);

    // Build SyncFacts from our graph
    let g = state.graph.read()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "graph read lock poisoned"))?;

    let all_nodes = g.all_nodes()
        .map_err(|e| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let all_edges = g.all_edges()
        .map_err(|e| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut facts: Vec<SyncFact> = Vec::new();
    for node in &all_nodes {
        let edges: Vec<SyncEdge> = all_edges.iter()
            .filter(|e| e.from == node.label)
            .map(|e| SyncEdge {
                from: e.from.clone(),
                to: e.to.clone(),
                relationship: e.relationship.clone(),
                confidence: e.confidence,
            })
            .collect();

        facts.push(SyncFact {
            label: node.label.clone(),
            confidence: node.confidence,
            provenance: "local".to_string(),
            edges,
            created_at: now_millis(),
            updated_at: now_millis(),
            topics: vec![],
            access_level: 2, // Public by default
            properties: node.properties.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        });
    }

    // Filter based on share policy
    let filtered = sync::filter_facts_for_peer(&facts, &share_policy);

    let response = SyncResponse {
        sender: mesh.identity.public.clone(),
        facts: filtered,
        has_more: false,
        peer_chain: vec![mesh.identity.public.clone()],
    };

    Ok(Json(serde_json::to_value(&response).unwrap_or_default()))
}

// ── POST /mesh/receive — Process incoming SyncResponse ──

pub async fn receive_sync(
    State(state): State<AppState>,
    Json(response): Json<SyncResponse>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;

    // Look up the sending peer
    let registry = mesh.peers.read()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "peer registry lock poisoned"))?;
    let peer = registry.get(&response.sender)
        .ok_or_else(|| mesh_err(StatusCode::FORBIDDEN, format!("unknown peer: {}", response.sender.to_hex())))?;
    if !peer.approved {
        return Err(mesh_err(StatusCode::FORBIDDEN, "peer not approved"));
    }
    let peer_clone = peer.clone();
    let peer_name = peer.name.clone();
    drop(registry);

    // Process incoming facts with trust model and conflict resolution
    let mut g = state.graph.write()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "graph write lock poisoned"))?;

    let local_lookup = |label: &str| -> Option<(f32, u64, String)> {
        let node = g.get_node(label).ok()??;
        Some((node.confidence, node.updated_at as u64, "local".to_string()))
    };

    let (sync_result, accepted_facts) = sync::process_incoming(
        &response,
        &peer_clone,
        &local_lookup,
    );

    // Store accepted facts
    let prov = engram_core::graph::Provenance {
        source_type: engram_core::graph::SourceType::Api,
        source_id: format!("mesh:{peer_name}"),
    };

    for (fact, local_conf) in &accepted_facts {
        let _ = g.store_with_confidence(&fact.label, *local_conf, &prov);

        // Store properties
        for (k, v) in &fact.properties {
            let _ = g.set_property(&fact.label, k, v);
        }

        // Store edges
        for edge in &fact.edges {
            // Ensure target node exists
            if g.find_node_id(&edge.to).unwrap_or(None).is_none() {
                let _ = g.store(&edge.to, &prov);
            }
            let _ = g.relate_with_confidence(
                &edge.from, &edge.to, &edge.relationship,
                edge.confidence, &prov,
            );
        }
    }

    drop(g);

    if sync_result.accepted > 0 || sync_result.disputed > 0 {
        state.mark_dirty();
        state.fire_rules_async();
    }

    // Log to audit
    let hops = response.peer_chain.len() as u8;
    let mut audit = mesh.audit.write()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "audit lock poisoned"))?;
    for (fact, local_conf) in &accepted_facts {
        audit.record_accepted(
            &peer_clone.public_key, &peer_name, &fact.label,
            fact.confidence, *local_conf, hops,
            Resolution::AcceptIncoming, now_millis(),
        );
    }

    let result = serde_json::json!({
        "accepted": sync_result.accepted,
        "rejected": sync_result.rejected,
        "disputed": sync_result.disputed,
        "skipped": sync_result.skipped,
        "has_more": sync_result.has_more,
    });

    Ok(Json(result))
}

// ── GET /mesh/peers — List registered peers ──

pub async fn list_peers(
    State(state): State<AppState>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;
    let registry = mesh.peers.read()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "peer registry lock poisoned"))?;
    Ok(Json(serde_json::to_value(&*registry).unwrap_or_default()))
}

// ── POST /mesh/peers — Register a new peer ──

#[derive(serde::Deserialize)]
pub struct RegisterPeerRequest {
    pub public_key: String,
    pub name: String,
    pub endpoint: String,
    pub trust: Option<f32>,
    pub approved: Option<bool>,
}

pub async fn register_peer(
    State(state): State<AppState>,
    Json(req): Json<RegisterPeerRequest>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;

    let public_key = engram_mesh::identity::PublicKey::from_hex(&req.public_key)
        .ok_or_else(|| mesh_err(StatusCode::BAD_REQUEST, "invalid public key hex"))?;

    let peer = PeerConfig {
        public_key,
        name: req.name.clone(),
        endpoint: req.endpoint.clone(),
        trust: req.trust.unwrap_or(0.5),
        approved: req.approved.unwrap_or(false),
        subscribed_topics: vec![],
        share_policy: SyncPolicy::default(),
        accept_policy: SyncPolicy::default(),
        last_sync: 0,
        online: false,
    };

    let mut registry = mesh.peers.write()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "peer registry lock poisoned"))?;
    registry.register(peer);

    Ok(Json(serde_json::json!({
        "registered": true,
        "peer_key": req.public_key,
        "name": req.name,
    })))
}

// ── DELETE /mesh/peers/{key} — Remove a peer ──

pub async fn remove_peer(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;
    let public_key = engram_mesh::identity::PublicKey::from_hex(&key)
        .ok_or_else(|| mesh_err(StatusCode::BAD_REQUEST, "invalid public key hex"))?;
    let mut registry = mesh.peers.write()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "peer registry lock poisoned"))?;
    let existed = registry.remove(&public_key).is_some();
    Ok(Json(serde_json::json!({
        "removed": existed,
        "peer_key": key,
    })))
}

// ── GET /mesh/audit — Recent audit entries ──

pub async fn audit(
    State(state): State<AppState>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;
    let audit = mesh.audit.read()
        .map_err(|_| mesh_err(StatusCode::INTERNAL_SERVER_ERROR, "audit lock poisoned"))?;
    let recent = audit.recent(100);
    let (accepted_count, rejected_count) = audit.stats();
    Ok(Json(serde_json::json!({
        "entries": recent,
        "stats": {
            "total_accepted": accepted_count,
            "total_rejected": rejected_count,
        }
    })))
}

// ── GET /mesh/identity — Show local node identity ──

pub async fn identity(
    State(state): State<AppState>,
) -> MeshResult<serde_json::Value> {
    let mesh = state.mesh.as_ref()
        .ok_or_else(|| mesh_err(StatusCode::SERVICE_UNAVAILABLE, "mesh not enabled"))?;
    Ok(Json(serde_json::json!({
        "public_key": mesh.identity.public.to_hex(),
    })))
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
