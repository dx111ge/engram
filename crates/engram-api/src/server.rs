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
        // System
        .route("/health", get(handlers::health))
        .route("/stats", get(handlers::stats))
        .route("/explain/{label}", get(handlers::explain))
        .route("/tools", get(tools_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the HTTP server on the given address.
pub async fn serve(state: AppState, addr: &str) -> std::io::Result<()> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("engram API listening on {}", addr);
    axum::serve(listener, app).await
}
