/// HTTP handlers for the REST API.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use engram_core::Embedder;
use engram_core::graph::Provenance;

use crate::natural;
use crate::state::{AppState, EngineConfig};
use crate::types::*;

pub(crate) type ApiResult<T> = std::result::Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

pub(crate) fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg.into() }))
}

pub(crate) fn read_lock_err() -> (StatusCode, Json<ErrorResponse>) {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph read lock poisoned")
}

pub(crate) fn write_lock_err() -> (StatusCode, Json<ErrorResponse>) {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph write lock poisoned")
}

/// Maximum time to wait for a graph write lock before giving up.
/// Prevents any handler from blocking the async runtime indefinitely.
const GRAPH_LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Acquire a graph write lock off the async runtime with a timeout.
/// Runs the closure in `spawn_blocking` so std::sync::RwLock never blocks tokio.
/// Use this for ALL graph write operations in async handlers.
pub(crate) async fn with_graph_write<F, T>(
    state: &AppState,
    f: F,
) -> Result<T, (StatusCode, Json<ErrorResponse>)>
where
    F: FnOnce(&mut engram_core::Graph) -> T + Send + 'static,
    T: Send + 'static,
{
    let graph = state.graph.clone();
    let result = tokio::time::timeout(
        GRAPH_LOCK_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let mut g = graph.write().map_err(|_| ())?;
            Ok::<T, ()>(f(&mut g))
        }),
    )
    .await;

    match result {
        Ok(Ok(Ok(val))) => Ok(val),
        Ok(Ok(Err(_))) => Err(write_lock_err()),
        Ok(Err(_)) => Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph write task panicked")),
        Err(_) => Err(api_err(StatusCode::SERVICE_UNAVAILABLE, "graph write lock timeout -- try again")),
    }
}

/// Acquire a graph read lock off the async runtime with a timeout.
/// Runs the closure in `spawn_blocking` so std::sync::RwLock never blocks tokio.
/// Use this for ALL graph read operations in async handlers.
pub(crate) async fn with_graph_read<F, T>(
    state: &AppState,
    f: F,
) -> Result<T, (StatusCode, Json<ErrorResponse>)>
where
    F: FnOnce(&engram_core::Graph) -> T + Send + 'static,
    T: Send + 'static,
{
    let graph = state.graph.clone();
    let result = tokio::time::timeout(
        GRAPH_LOCK_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let g = graph.read().map_err(|_| ())?;
            Ok::<T, ()>(f(&g))
        }),
    )
    .await;

    match result {
        Ok(Ok(Ok(val))) => Ok(val),
        Ok(Ok(Err(_))) => Err(read_lock_err()),
        Ok(Err(_)) => Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, "graph read task panicked")),
        Err(_) => Err(api_err(StatusCode::SERVICE_UNAVAILABLE, "graph read lock timeout -- try again")),
    }
}

pub(crate) fn provenance(source: &Option<String>) -> Provenance {
    match source {
        Some(s) => Provenance::user(s),
        None => Provenance::user("api"),
    }
}

/// Resolve the engram home directory (~/.engram/).
/// Delegates to the canonical implementation in engram-ingest.
#[cfg(feature = "ingest")]
pub(crate) fn engram_home() -> Option<std::path::PathBuf> {
    engram_ingest::engram_home()
}

/// Resolve the engram home directory (~/.engram/) -- fallback when ingest feature is off.
#[cfg(not(feature = "ingest"))]
pub(crate) fn engram_home() -> Option<std::path::PathBuf> {
    std::env::var_os("ENGRAM_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| std::path::PathBuf::from(h).join(".engram"))
        })
}

#[allow(dead_code)]
pub(crate) fn feature_not_enabled(name: &str) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::NOT_IMPLEMENTED,
     Json(ErrorResponse { error: format!("{name} feature not enabled -- rebuild with --features {name}") }))
}

pub mod store;
pub mod query;
pub mod admin;
pub mod proxy;
pub mod ingest;
pub mod stream;
pub mod assess;
pub mod config;
pub mod models;
pub mod secrets;
pub mod kb;
pub mod seed;
pub mod chat;
pub mod chat_analysis;
pub mod chat_temporal;
pub mod chat_investigation;
pub mod chat_reporting;
pub mod document;
pub mod fact;
pub mod debate;
pub mod web_search;

// Re-export all public functions so `use crate::handlers::*` continues to work.
pub use store::*;
pub use query::*;
pub use admin::*;
pub use proxy::*;
pub use ingest::*;
pub use stream::*;
pub use assess::*;
pub use config::*;
pub use models::*;
pub use secrets::*;
pub use kb::*;
pub use seed::*;
pub use fact::*;
