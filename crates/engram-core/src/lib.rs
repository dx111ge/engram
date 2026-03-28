//! engram-core: storage engine, knowledge graph, and search indexes.
//!
//! Provides the core data structures and operations for engram:
//! - [`storage`] -- custom binary `.brain` file format with mmap, WAL, crash recovery
//! - [`graph`] -- knowledge graph with typed nodes, edges, properties, confidence lifecycle
//! - [`index`] -- BM25 full-text, HNSW vector, bitmap, temporal, and hybrid search
//! - [`learning`] -- confidence model integration (reinforcement, decay, correction)

pub mod storage;
pub mod index;
pub mod graph;
pub mod learning;
pub mod events;

pub use storage::brain_file::BrainFile;
pub use storage::doc_store::{DocStore, MimeType as DocMimeType, ContentHash};
pub use storage::edge::Edge;
pub use storage::node::Node;
pub use storage::error::StorageError;
pub use graph::Graph;
pub use index::embedding::{Embedder, EmbedError};
pub use index::embed_api::ApiEmbedder;
#[cfg(feature = "onnx")]
pub use index::embed_onnx::OnnxEmbedder;
pub use index::hnsw::{HnswIndex, QuantizationMode};
pub use events::{EventBus, GraphEvent, ThresholdDirection, ConflictType, OverflowStrategy};
