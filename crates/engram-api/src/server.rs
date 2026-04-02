/// HTTP server setup — routes, middleware, startup.

use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::handlers;
use crate::state::AppState;
use crate::tools;

async fn tools_handler() -> axum::Json<serde_json::Value> {
    axum::Json(tools::tool_definitions())
}

/// Build the axum router with all REST endpoints.
pub fn router(state: AppState) -> Router {
    router_with_frontend(state, None)
}

/// Build the axum router, optionally serving a frontend directory at `/`.
pub fn router_with_frontend(state: AppState, frontend_dir: Option<&str>) -> Router {
    let mut app = Router::new()
        // Core graph operations
        .route("/store", post(handlers::store))
        .route("/relate", post(handlers::relate))
        .route("/batch", post(handlers::batch))
        .route("/batch/stream", post(handlers::batch_stream))
        .route("/query", post(handlers::query))
        .route("/similar", post(handlers::similar))
        .route("/search", post(handlers::search))
        .route("/autocomplete", post(handlers::autocomplete))
        .route("/paths", post(handlers::find_paths))
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
        // Ingest pipeline (available when compiled with `ingest` feature)
        .route("/ingest", post(handlers::ingest))
        .route("/ingest/analyze", post(handlers::ingest_analyze))
        .route("/ingest/file", post(handlers::ingest_file))
        .route("/ingest/configure", post(handlers::ingest_configure))
        .route("/sources", get(handlers::list_sources))
        .route("/sources/{name}/usage", get(handlers::source_usage))
        .route("/sources/{name}/ledger", get(handlers::source_ledger))
        // Action engine (available when compiled with `actions` feature)
        .route("/actions/rules", post(handlers::load_action_rules))
        .route("/actions/rules", get(handlers::list_action_rules))
        .route("/actions/rules/{id}", get(handlers::get_action_rule))
        .route("/actions/rules/{id}", delete(handlers::delete_action_rule))
        .route("/actions/dry-run", post(handlers::dry_run_action))
        // Seed enrichment (interactive multi-phase flow)
        .route("/ingest/seed/start", post(handlers::seed_start))
        .route("/ingest/seed/confirm-aoi", post(handlers::seed_confirm_aoi))
        .route("/ingest/seed/confirm-entities", post(handlers::seed_confirm_entities))
        .route("/ingest/seed/commit", post(handlers::seed_commit))
        .route("/ingest/seed/connections", get(handlers::seed_connections))
        .route("/ingest/seed/confirm-relations", post(handlers::seed_confirm_relations))
        .route("/ingest/seed/stream", get(handlers::seed_stream))
        // Ingest review mode (review=true)
        .route("/ingest/review", get(handlers::ingest_review))
        .route("/ingest/review/confirm", post(handlers::ingest_review_confirm))
        // Streaming
        .route("/events/stream", get(handlers::event_stream))
        .route("/ingest/webhook/{pipeline_id}", post(handlers::webhook_receive))
        // Reason / gap detection (available when compiled with `reason` feature)
        .route("/reason/gaps", get(handlers::reason_gaps))
        .route("/reason/scan", post(handlers::reason_scan))
        .route("/reason/frontier", get(handlers::reason_frontier))
        .route("/reason/suggest", post(handlers::reason_suggest))
        // Mesh discovery (profiles, federated query)
        .route("/mesh/profiles", get(handlers::mesh_profiles))
        .route("/mesh/discover", get(handlers::mesh_discover))
        .route("/mesh/query", post(handlers::mesh_federated_query))
        // Batch job streaming
        .route("/batch/jobs/{id}/stream", get(handlers::batch_job_stream))
        // WebSocket ingest
        .route("/ingest/ws/{pipeline_id}", get(handlers::ws_ingest))
        // Enrichment SSE stream
        .route("/enrich/stream", get(handlers::enrich_stream))
        // Proxy (CORS bypass for browser-based intel dashboard)
        .route("/proxy/gdelt", get(handlers::proxy_gdelt))
        .route("/proxy/rss", get(handlers::proxy_news_rss))
        .route("/proxy/llm", post(handlers::proxy_llm))
        .route("/proxy/models", get(handlers::proxy_llm_models))
        .route("/proxy/fetch-models", post(handlers::proxy_fetch_models))
        .route("/proxy/search", get(handlers::proxy_web_search))
        // Assessments
        .route("/assessments", post(handlers::create_assessment))
        .route("/assessments", get(handlers::list_assessments))
        .route("/assessments/{label}", get(handlers::get_assessment))
        .route("/assessments/{label}", delete(handlers::delete_assessment))
        .route("/assessments/{label}", patch(handlers::update_assessment))
        .route("/assessments/{label}/evaluate", post(handlers::evaluate_assessment))
        .route("/assessments/{label}/evidence", post(handlers::add_assessment_evidence))
        .route("/assessments/{label}/evidence/{id}", delete(handlers::remove_assessment_evidence))
        .route("/assessments/{label}/history", get(handlers::assessment_history))
        .route("/assessments/{label}/watch", post(handlers::add_assessment_watch))
        .route("/assessments/{label}/watch/{entity}", delete(handlers::remove_assessment_watch))
        // Facts (list, confirm/debunk with trust propagation)
        .route("/facts", post(handlers::fact::list_facts))
        .route("/facts/{label}/confirm", post(handlers::fact_confirm))
        .route("/facts/{label}/debunk", post(handlers::fact_debunk))
        // Secrets (local-only, no mesh/A2A exposure)
        .route("/secrets", get(handlers::list_secrets))
        .route("/secrets/{key}", post(handlers::set_secret))
        .route("/secrets/{key}", delete(handlers::delete_secret))
        .route("/secrets/{key}/check", get(handlers::check_secret))
        // Configuration
        .route("/config", get(handlers::get_config))
        .route("/config", post(handlers::set_config))
        .route("/config/onnx-model", get(handlers::check_onnx_model))
        .route("/config/onnx-download", post(handlers::download_onnx_model))
        .route("/config/ner-download", post(handlers::download_ner_model))
        .route("/config/ner-download-onnx", post(handlers::download_ner_model_onnx))
        .route("/config/gliner2-download", post(handlers::download_gliner2_model))
        .route("/config/ollama-pull", post(handlers::ollama_pull))
        .route("/config/ner-model", get(handlers::check_ner_model))
        .route("/config/rel-download", post(handlers::download_rel_model))
        .route("/config/rel-model", get(handlers::check_rel_model))
        .route("/config/relation-templates/export", get(handlers::export_relation_templates))
        .route("/config/relation-templates/import", post(handlers::import_relation_templates))
        .route("/config/relation-types", get(handlers::list_relation_types))
        .route("/config/node-types", get(handlers::store::node_types))
        // Node + Edge operations
        .route("/node", patch(handlers::store::patch_node))
        .route("/edge", patch(handlers::rename_edge))
        .route("/edge/delete", post(handlers::delete_edge))
        // Admin
        .route("/admin/reset", post(handlers::admin_reset))
        .route("/admin/dedup-edges", post(handlers::admin_dedup_edges))
        // Config status & KB management
        .route("/config/status", get(handlers::config_status))
        .route("/config/wizard-complete", post(handlers::wizard_complete))
        .route("/config/kb", get(handlers::list_kb_endpoints))
        .route("/config/kb", post(handlers::add_kb_endpoint))
        .route("/config/kb/{name}", delete(handlers::delete_kb_endpoint))
        .route("/config/kb/{name}/test", post(handlers::test_kb_endpoint))
        // KGE training
        .route("/kge/train", post(handlers::kge_train))
        .merge(
            Router::new()
                .route("/config/onnx-model", post(handlers::upload_onnx_model))
                .route("/config/model-upload", post(handlers::upload_model))
                .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024 * 1024)) // 1 GB for model uploads
        )
        // Reindex
        .route("/reindex", post(handlers::reindex))
        // Auth
        .route("/auth/status", get(auth::auth_status))
        .route("/auth/setup", post(auth::auth_setup))
        .route("/auth/login", post(auth::auth_login))
        .route("/auth/logout", post(auth::auth_logout))
        .route("/auth/users", get(auth::list_users))
        .route("/auth/users", post(auth::create_user))
        .route("/auth/users/{username}", axum::routing::put(auth::update_user))
        .route("/auth/users/{username}", delete(auth::delete_user))
        .route("/auth/change-password", post(auth::change_password))
        .route("/auth/api-keys", get(auth::list_api_keys))
        .route("/auth/api-keys", post(auth::create_api_key))
        .route("/auth/api-keys/{id}", delete(auth::revoke_api_key))
        // Document provenance endpoints
        .route("/provenance", post(handlers::document::provenance))
        .route("/documents", post(handlers::document::documents))
        .route("/documents/content", post(handlers::document::document_content))
        .route("/documents/passage", post(handlers::document::document_passage))
        // Chat tool endpoints (intelligence analyst workbench)
        .route("/chat/temporal_query", post(handlers::chat::temporal_query))
        .route("/chat/timeline", post(handlers::chat::timeline))
        .route("/chat/current_state", post(handlers::chat::current_state))
        .route("/chat/changes", post(handlers::chat::changes))
        .route("/chat/compare", post(handlers::chat_analysis::compare))
        .route("/chat/shortest_path", post(handlers::chat_analysis::shortest_path))
        .route("/chat/most_connected", post(handlers::chat_analysis::most_connected))
        .route("/chat/isolated", post(handlers::chat_analysis::isolated))
        .route("/chat/what_if", post(handlers::chat_analysis::what_if))
        .route("/chat/influence_path", post(handlers::chat_analysis::influence_path))
        .route("/chat/briefing", post(handlers::chat_analysis::briefing))
        .route("/chat/export_subgraph", post(handlers::chat_analysis::export_subgraph))
        .route("/chat/entity_timeline", post(handlers::chat_analysis::entity_timeline))
        .route("/chat/fact_provenance", post(handlers::chat_analysis::fact_provenance))
        .route("/chat/network_analysis", post(handlers::chat_analysis::network_analysis))
        .route("/chat/entity_360", post(handlers::chat_analysis::entity_360))
        .route("/chat/entity_gaps", post(handlers::chat_analysis::entity_gaps))
        .route("/chat/contradictions", post(handlers::chat_analysis::contradictions))
        .route("/chat/situation_at", post(handlers::chat_analysis::situation_at))
        .route("/chat/watch", post(handlers::chat::watch))
        .route("/chat/schedule", post(handlers::chat::schedule))
        // System
        .route("/health", get(handlers::health))
        .route("/stats", get(handlers::stats))
        .route("/compute", get(handlers::compute))
        .route("/explain/{label}", get(handlers::explain))
        .route("/tools", get(tools_handler))
        // Auth middleware (route_layer = applies to routes only, not fallback)
        .route_layer(axum::middleware::from_fn_with_state::<_, AppState, (axum::extract::State<AppState>, axum::extract::Request)>(
            state.clone(),
            auth::auth_middleware,
        ))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Serve frontend static files as fallback (if directory exists)
    // SPA: unknown routes fall back to index.html so client-side routing works
    if let Some(dir) = frontend_dir {
        let index_path = std::path::PathBuf::from(dir).join("index.html");
        let serve_dir = ServeDir::new(dir)
            .fallback(tower_http::services::ServeFile::new(index_path));
        app = app.fallback_service(serve_dir);
    }

    app
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
    serve_with_frontend(state, addr, None).await
}

/// Start the HTTP server, optionally serving a frontend directory.
pub async fn serve_with_frontend(state: AppState, addr: &str, frontend_dir: Option<&str>) -> std::io::Result<()> {
    // Background checkpoint timer — flush dirty writes every 5 seconds
    let checkpoint_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut cleanup_counter = 0u32;
        loop {
            interval.tick().await;
            if checkpoint_state.checkpoint_if_dirty() {
                tracing::debug!("background checkpoint complete");
            }
            // Clean up expired sessions every 60 seconds (12 ticks)
            cleanup_counter += 1;
            if cleanup_counter >= 12 {
                cleanup_counter = 0;
                auth::cleanup_sessions(&checkpoint_state.sessions);
                // Clean up expired ingest review sessions (30 min TTL)
                if let Ok(mut sessions) = checkpoint_state.ingest_sessions.write() {
                    sessions.retain(|_, s| s.created_at.elapsed().as_secs() < 1800);
                }
            }
        }
    });

    // Assessment auto-evaluation subscriber (listens for FactStored events)
    #[cfg(feature = "assess")]
    {
        let assess_state = state.clone();
        tokio::spawn(async move {
            let mut rx = assess_state.event_bus.subscribe();
            loop {
                match rx.recv().await {
                    Ok(engram_core::events::GraphEvent::FactStored { node_id, label, confidence, .. }) => {
                        let engine = engram_assess::AssessmentEngine::new(
                            assess_state.assessments.clone(),
                            assess_state.graph.clone(),
                        );
                        let results = engine.on_fact_stored(node_id, &label, confidence);
                        if !results.is_empty() {
                            tracing::info!(
                                "assessment auto-eval: {} assessments updated",
                                results.len()
                            );
                            // Update current_probability on assessment graph nodes
                            if let Ok(mut g) = assess_state.graph.write() {
                                for result in &results {
                                    let prob_with_shift = format!("{}|{}", result.new_probability, result.shift);
                                    let _ = g.set_property(&result.label, "current_probability", &prob_with_shift);
                                }
                            }
                            assess_state.mark_dirty();
                        }
                    }
                    Ok(_) => {} // ignore other events
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("assessment subscriber lagged by {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("event bus closed, assessment subscriber stopping");
                        break;
                    }
                }
            }
        });
    }

    let app = router_with_frontend(state, frontend_dir);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("engram API listening on {}", addr);
    axum::serve(listener, app).await
}
