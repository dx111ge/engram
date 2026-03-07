/// gRPC-compatible service layer.
///
/// Implements the engram gRPC service contract using JSON serialization
/// over the same handler functions as the REST API. The proto file at
/// `proto/engram.proto` defines the canonical contract.
///
/// When protoc is available, switch to tonic code generation for full
/// protobuf binary serialization. This module provides the same API
/// surface with JSON payloads via the existing axum infrastructure.
///
/// For high-performance programmatic access, this server binds on a
/// separate port and uses the same AppState as the HTTP server.

use axum::routing::{get, post};
use axum::Router;

use crate::handlers;
use crate::state::AppState;

/// Build a gRPC-style router (JSON-over-HTTP/2).
///
/// Uses the same handlers as the REST API but with gRPC-style paths:
///   /engram.EngramService/Store
///   /engram.EngramService/Relate
///   etc.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/engram.EngramService/Store", post(handlers::store))
        .route("/engram.EngramService/Relate", post(handlers::relate))
        .route("/engram.EngramService/Query", post(handlers::query))
        .route("/engram.EngramService/Search", post(handlers::search))
        .route("/engram.EngramService/GetNode", post(handlers::get_node_by_body))
        .route("/engram.EngramService/DeleteNode", post(handlers::delete_node_by_body))
        .route("/engram.EngramService/Reinforce", post(handlers::reinforce))
        .route("/engram.EngramService/Correct", post(handlers::correct))
        .route("/engram.EngramService/Decay", post(handlers::decay))
        .route("/engram.EngramService/Health", get(handlers::health))
        .route("/engram.EngramService/Stats", post(handlers::stats_post))
        .route("/engram.EngramService/Ask", post(handlers::ask))
        .route("/engram.EngramService/Tell", post(handlers::tell))
        .with_state(state)
}

/// Start the gRPC-style server on a separate port.
pub async fn serve_grpc(state: AppState, addr: &str) -> std::io::Result<()> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("engram gRPC service listening on {}", addr);
    axum::serve(listener, app).await
}
