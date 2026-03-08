/// HTTP server setup — routes, middleware, startup.

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;
use crate::tools;

async fn tools_handler() -> axum::Json<serde_json::Value> {
    axum::Json(tools::tool_definitions())
}

/// Build the axum router with all REST endpoints.
pub fn router(state: AppState) -> Router {
    Router::new()
        // Core graph operations
        .route("/store", post(handlers::store))
        .route("/relate", post(handlers::relate))
        .route("/batch", post(handlers::batch))
        .route("/query", post(handlers::query))
        .route("/similar", post(handlers::similar))
        .route("/search", post(handlers::search))
        .route("/ask", post(handlers::ask))
        .route("/tell", post(handlers::tell))
        .route("/node/{label}", get(handlers::get_node))
        .route("/node/{label}", delete(handlers::delete_node))
        // Learning operations
        .route("/learn/reinforce", post(handlers::reinforce))
        .route("/learn/correct", post(handlers::correct))
        .route("/learn/decay", post(handlers::decay))
        .route("/learn/derive", post(handlers::derive))
        // Rules (push-based inference triggers)
        .route("/rules", post(handlers::load_rules))
        .route("/rules", get(handlers::list_rules))
        .route("/rules", delete(handlers::clear_rules))
        // JSON-LD export/import
        .route("/export/jsonld", get(handlers::export_jsonld))
        .route("/import/jsonld", post(handlers::import_jsonld))
        // Vector quantization
        .route("/quantize", post(handlers::set_quantization))
        // Mesh networking (available when compiled with `mesh` feature)
        .route("/mesh/heartbeat", get(mesh_heartbeat_handler))
        .route("/mesh/sync", post(mesh_sync_handler))
        .route("/mesh/receive", post(mesh_receive_handler))
        .route("/mesh/peers", get(mesh_list_peers_handler))
        .route("/mesh/peers", post(mesh_register_peer_handler))
        .route("/mesh/peers/{key}", delete(mesh_remove_peer_handler))
        .route("/mesh/audit", get(mesh_audit_handler))
        .route("/mesh/identity", get(mesh_identity_handler))
        // Proxy (CORS bypass for browser-based intel dashboard)
        .route("/proxy/gdelt", get(handlers::proxy_gdelt))
        .route("/proxy/rss", get(handlers::proxy_news_rss))
        // System
        .route("/health", get(handlers::health))
        .route("/stats", get(handlers::stats))
        .route("/compute", get(handlers::compute))
        .route("/explain/{label}", get(handlers::explain))
        .route("/tools", get(tools_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ── Mesh route handlers (feature-gated) ──

#[cfg(feature = "mesh")]
use crate::mesh;

#[cfg(feature = "mesh")]
async fn mesh_heartbeat_handler(state: axum::extract::State<AppState>) -> impl axum::response::IntoResponse {
    mesh::heartbeat(state).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_heartbeat_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(feature = "mesh")]
async fn mesh_sync_handler(state: axum::extract::State<AppState>, body: axum::Json<serde_json::Value>) -> impl axum::response::IntoResponse {
    let req: engram_mesh::gossip::SyncRequest = match serde_json::from_value(body.0) {
        Ok(r) => r,
        Err(e) => return Err((axum::http::StatusCode::BAD_REQUEST, axum::Json(crate::types::ErrorResponse { error: e.to_string() }))),
    };
    mesh::serve_sync(state, axum::Json(req)).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_sync_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(feature = "mesh")]
async fn mesh_receive_handler(state: axum::extract::State<AppState>, body: axum::Json<serde_json::Value>) -> impl axum::response::IntoResponse {
    let resp: engram_mesh::gossip::SyncResponse = match serde_json::from_value(body.0) {
        Ok(r) => r,
        Err(e) => return Err((axum::http::StatusCode::BAD_REQUEST, axum::Json(crate::types::ErrorResponse { error: e.to_string() }))),
    };
    mesh::receive_sync(state, axum::Json(resp)).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_receive_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(feature = "mesh")]
async fn mesh_list_peers_handler(state: axum::extract::State<AppState>) -> impl axum::response::IntoResponse {
    mesh::list_peers(state).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_list_peers_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(feature = "mesh")]
async fn mesh_register_peer_handler(state: axum::extract::State<AppState>, body: axum::Json<mesh::RegisterPeerRequest>) -> impl axum::response::IntoResponse {
    mesh::register_peer(state, body).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_register_peer_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(feature = "mesh")]
async fn mesh_remove_peer_handler(state: axum::extract::State<AppState>, path: axum::extract::Path<String>) -> impl axum::response::IntoResponse {
    mesh::remove_peer(state, path).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_remove_peer_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(feature = "mesh")]
async fn mesh_audit_handler(state: axum::extract::State<AppState>) -> impl axum::response::IntoResponse {
    mesh::audit(state).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_audit_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(feature = "mesh")]
async fn mesh_identity_handler(state: axum::extract::State<AppState>) -> impl axum::response::IntoResponse {
    mesh::identity(state).await
}
#[cfg(not(feature = "mesh"))]
async fn mesh_identity_handler() -> impl axum::response::IntoResponse {
    mesh_not_enabled()
}

#[cfg(not(feature = "mesh"))]
fn mesh_not_enabled() -> (axum::http::StatusCode, axum::Json<crate::types::ErrorResponse>) {
    (axum::http::StatusCode::NOT_IMPLEMENTED,
     axum::Json(crate::types::ErrorResponse { error: "mesh feature not enabled — rebuild with --features mesh".into() }))
}

/// Start the HTTP server on the given address.
///
/// Spawns a background checkpoint task that flushes dirty writes to disk
/// every 5 seconds. This decouples expensive msync/FlushViewOfFile from
/// the request path — writes go to WAL + mmap immediately and are
/// crash-recoverable, but the full checkpoint happens asynchronously.
pub async fn serve(state: AppState, addr: &str) -> std::io::Result<()> {
    // Background checkpoint timer — flush dirty writes every 5 seconds
    let checkpoint_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if checkpoint_state.checkpoint_if_dirty() {
                tracing::debug!("background checkpoint complete");
            }
        }
    });

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("engram API listening on {}", addr);
    axum::serve(listener, app).await
}
