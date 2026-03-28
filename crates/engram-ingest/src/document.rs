/// Document context utilities for provenance tracking.
///
/// Builds `DocumentContext` from raw ingest items and provides
/// helper functions for document node labels.

use crate::types::{DocumentContext, RawItem, Content};
use engram_core::storage::doc_store::DocStore;
use std::sync::Arc;

/// Build a `DocumentContext` from a raw item and its extracted text.
pub fn build_doc_context(item: &RawItem, full_text: &str) -> Arc<DocumentContext> {
    let content_hash = DocStore::hash_content(full_text.as_bytes());
    let content_hash_hex = DocStore::hash_hex(&content_hash);
    let mime_type = match &item.content {
        Content::Text(_) => "text/plain".to_string(),
        Content::Structured(_) => "application/json".to_string(),
        Content::Bytes { mime, .. } => mime.clone(),
    };
    let title = item.metadata.get("title").cloned()
        .or_else(|| extract_title_from_url(item.source_url.as_deref()));

    Arc::new(DocumentContext {
        content_hash,
        content_hash_hex,
        url: item.source_url.clone(),
        file_path: item.metadata.get("file_path").cloned(),
        mime_type,
        full_text: full_text.to_string(),
        title,
        doc_date: item.metadata.get("doc_date").cloned()
            .or_else(|| item.metadata.get("date").cloned()),
        fetched_at: item.fetched_at,
    })
}

/// Generate a graph node label from a content hash hex string.
pub fn doc_label(hash_hex: &str) -> String {
    let short = if hash_hex.len() >= 8 { &hash_hex[..8] } else { hash_hex };
    format!("Doc:{short}")
}

/// Extract a human-readable title from a URL (last path segment, cleaned up).
fn extract_title_from_url(url: Option<&str>) -> Option<String> {
    let url = url?;
    let path = url.split('?').next().unwrap_or(url);
    let segment = path.rsplit('/').find(|s| !s.is_empty())?;
    if segment.contains('.') {
        // Has extension — likely a filename
        let name = segment.rsplit('.').last().unwrap_or(segment);
        Some(name.replace(['-', '_'], " "))
    } else {
        Some(segment.replace(['-', '_'], " "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_label() {
        assert_eq!(doc_label("a3f7b2c1deadbeef"), "Doc:a3f7b2c1");
        assert_eq!(doc_label("abcd"), "Doc:abcd");
    }

    #[test]
    fn test_extract_title_from_url() {
        assert_eq!(
            extract_title_from_url(Some("https://reuters.com/world/putin-addresses-nation")),
            Some("putin addresses nation".into())
        );
        assert_eq!(
            extract_title_from_url(Some("https://example.com/docs/report_2024.pdf")),
            Some("report 2024".into())
        );
        assert_eq!(extract_title_from_url(None), None);
    }
}
