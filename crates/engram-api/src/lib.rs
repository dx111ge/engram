/// engram-api: HTTP REST server for the engram knowledge graph.
///
/// Wraps engram-core::Graph with a JSON API. All endpoints return
/// structured responses suitable for LLM tool-calling integration.

/// Debug log for debate flow. Appends to `debate_debug.log`. Toggle via POST /config {"debate_debug": true}.
macro_rules! dbg_debate {
    ($($arg:tt)*) => {
        if $crate::handlers::debate::DEBATE_DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
            let msg = format!($($arg)*);
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            let s = now.as_secs() % 86400; // seconds since midnight UTC
            let line = format!("[{:02}:{:02}:{:02}.{:03}] {}\n", s/3600, (s%3600)/60, s%60, now.subsec_millis(), msg);
            let _ = std::fs::OpenOptions::new().create(true).append(true).open("debate_debug.log")
                .and_then(|mut f| { use std::io::Write; f.write_all(line.as_bytes()) });
        }
    };
}

pub mod auth;
pub mod grpc;
pub mod handlers;
pub mod mcp;
#[cfg(feature = "mesh")]
pub mod mesh;
pub mod natural;
pub mod secrets;
pub mod server;
pub mod state;
pub mod tools;
pub mod types;
