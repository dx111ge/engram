/// engram-api: HTTP REST server for the engram knowledge graph.
///
/// Wraps engram-core::Graph with a JSON API. All endpoints return
/// structured responses suitable for LLM tool-calling integration.

pub mod auth;
pub mod grpc;
pub mod handlers;
pub mod mcp;
pub mod natural;
pub mod server;
pub mod state;
pub mod tools;
pub mod types;
