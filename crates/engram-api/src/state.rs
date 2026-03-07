/// Shared application state — wraps the Graph in an Arc<Mutex>.

use engram_core::Graph;
use std::sync::{Arc, Mutex};

/// Thread-safe shared graph state for the HTTP server.
#[derive(Clone)]
pub struct AppState {
    pub graph: Arc<Mutex<Graph>>,
}

impl AppState {
    pub fn new(graph: Graph) -> Self {
        AppState {
            graph: Arc::new(Mutex::new(graph)),
        }
    }
}
