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
