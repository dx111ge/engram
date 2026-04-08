/// Document provenance API handlers.
///
/// Provides endpoints for tracing entity provenance back to source documents
/// and listing/reading ingested documents.

use axum::extract::State;
use axum::Json;
use axum::http::StatusCode;
use crate::state::AppState;
use crate::handlers::{ApiResult, api_err, read_lock_err};

// ── Request/Response types ──

#[derive(serde::Deserialize)]
pub struct ProvenanceRequest {
    pub entity: String,
}

#[derive(serde::Serialize)]
pub struct ProvenanceResponse {
    pub entity: String,
    pub document_count: usize,
    pub documents: Vec<ProvenanceDocument>,
}

#[derive(serde::Serialize)]
pub struct ProvenanceDocument {
    pub label: String,
    pub title: String,
    pub url: String,
    pub doc_date: String,
    pub ingested_at: String,
    pub publisher: String,
    pub facts: Vec<ProvenanceFact>,
}

#[derive(serde::Serialize)]
pub struct ProvenanceFact {
    pub label: String,
    pub claim: String,
}

#[derive(serde::Deserialize)]
pub struct DocumentsRequest {
    pub limit: Option<usize>,
}

#[derive(serde::Serialize)]
pub struct DocumentListItem {
    pub label: String,
    pub title: String,
    pub url: String,
    pub doc_date: String,
    pub ingested_at: String,
    pub content_length: String,
    pub publisher: String,
    pub fact_count: usize,
    pub ner_complete: bool,
    pub original_language: String,
}

#[derive(serde::Serialize)]
pub struct DocumentsResponse {
    pub count: usize,
    pub documents: Vec<DocumentListItem>,
}

#[derive(serde::Serialize)]
pub struct DocumentContentResponse {
    pub label: String,
    pub title: String,
    pub url: String,
    pub content: String,
    pub mime_type: String,
}

// ── Handlers ──

/// POST /provenance -- trace entity back to source documents.
pub async fn provenance(
    State(state): State<AppState>,
    Json(req): Json<ProvenanceRequest>,
) -> ApiResult<ProvenanceResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let doc_facts = g.documents_for_entity(&req.entity)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut documents = Vec::new();
    for (doc_label, facts) in &doc_facts {
        let props = g.get_properties(doc_label)
            .unwrap_or_default().unwrap_or_default();
        let publisher = g.edges_from(doc_label).unwrap_or_default()
            .into_iter()
            .find(|e| e.relationship == "published_by")
            .map(|e| e.to)
            .unwrap_or_default();

        documents.push(ProvenanceDocument {
            label: doc_label.clone(),
            title: props.get("title").cloned().unwrap_or_default(),
            url: props.get("url").cloned().unwrap_or_default(),
            doc_date: props.get("doc_date").cloned().unwrap_or_default(),
            ingested_at: props.get("ingested_at").cloned().unwrap_or_default(),
            publisher,
            facts: facts.iter().map(|(fl, claim)| ProvenanceFact {
                label: fl.clone(),
                claim: claim.clone(),
            }).collect(),
        });
    }

    Ok(Json(ProvenanceResponse {
        entity: req.entity,
        document_count: documents.len(),
        documents,
    }))
}

/// POST /documents -- list ingested documents.
pub async fn documents(
    State(state): State<AppState>,
    Json(req): Json<DocumentsRequest>,
) -> ApiResult<DocumentsResponse> {
    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let limit = req.limit.unwrap_or(20);

    let all_nodes = g.all_nodes()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut docs = Vec::new();
    for node in &all_nodes {
        let node_type = g.get_node_type(&node.label).unwrap_or_default();
        if node_type != "Document" {
            continue;
        }
        let props = g.get_properties(&node.label)
            .unwrap_or_default().unwrap_or_default();
        let fact_count = g.edges_to(&node.label).unwrap_or_default()
            .iter()
            .filter(|e| e.relationship == "extracted_from")
            .count();
        let publisher = g.edges_from(&node.label).unwrap_or_default()
            .into_iter()
            .find(|e| e.relationship == "published_by")
            .map(|e| e.to)
            .unwrap_or_default();

        docs.push(DocumentListItem {
            label: node.label.clone(),
            title: props.get("title").cloned().unwrap_or_default(),
            url: props.get("url").cloned().unwrap_or_default(),
            doc_date: props.get("doc_date").cloned().unwrap_or_default(),
            ingested_at: props.get("ingested_at").cloned().unwrap_or_default(),
            content_length: props.get("content_length").cloned().unwrap_or_default(),
            publisher,
            fact_count,
            ner_complete: props.get("ner_complete").is_some_and(|v| v == "true"),
            original_language: props.get("original_language").cloned().unwrap_or_default(),
        });
        if docs.len() >= limit {
            break;
        }
    }

    Ok(Json(DocumentsResponse {
        count: docs.len(),
        documents: docs,
    }))
}

/// POST /documents/content -- read cached document content.
pub async fn document_content(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<DocumentContentResponse> {
    let doc_label = req["document"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "missing 'document' field".to_string()))?;

    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let props = g.get_properties(doc_label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("document not found: {doc_label}")))?;

    let content_hash_hex = props.get("content_hash")
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "document has no content_hash".to_string()))?;

    // Parse hex hash back to bytes
    let hash_bytes: Vec<u8> = (0..content_hash_hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&content_hash_hex[i..i+2], 16).ok())
        .collect();

    if hash_bytes.len() != 32 {
        return Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, "invalid content hash".to_string()));
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hash_bytes);

    let store = state.doc_store.read().map_err(|_| read_lock_err())?;
    let (content_bytes, mime) = store.load(&hash)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("content not cached: {e}")))?;

    let content = String::from_utf8(content_bytes)
        .unwrap_or_else(|_| "[binary content]".to_string());

    Ok(Json(DocumentContentResponse {
        label: doc_label.to_string(),
        title: props.get("title").cloned().unwrap_or_default(),
        url: props.get("url").cloned().unwrap_or_default(),
        content,
        mime_type: mime.as_str().to_string(),
    }))
}

/// POST /documents/passage -- return a specific chunk from a document.
///
/// Accepts `{ document, chunk_index }`. Re-chunks the document with the same
/// parameters used during ingestion (3000 chars) and returns the specified chunk.
/// Falls back to full content if chunk_index is missing or out of range.
pub async fn document_passage(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<DocumentContentResponse> {
    let doc_label = req["document"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "missing 'document' field".to_string()))?;
    let chunk_index = req["chunk_index"].as_u64().map(|v| v as usize);

    let g = state.graph.read().map_err(|_| read_lock_err())?;
    let props = g.get_properties(doc_label)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("document not found: {doc_label}")))?;

    let content_hash_hex = props.get("content_hash")
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "document has no content_hash".to_string()))?;

    let hash_bytes: Vec<u8> = (0..content_hash_hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&content_hash_hex[i..i+2], 16).ok())
        .collect();

    if hash_bytes.len() != 32 {
        return Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, "invalid content hash".to_string()));
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hash_bytes);

    let store = state.doc_store.read().map_err(|_| read_lock_err())?;
    let (content_bytes, mime) = store.load(&hash)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("content not cached: {e}")))?;

    let full_content = String::from_utf8(content_bytes)
        .unwrap_or_else(|_| "[binary content]".to_string());

    // Extract the requested chunk
    let content = if let Some(idx) = chunk_index {
        #[cfg(feature = "ingest")]
        {
            let chunks = engram_ingest::fact_extract::chunk_text(&full_content, 3000);
            chunks.into_iter().nth(idx).unwrap_or(full_content)
        }
        #[cfg(not(feature = "ingest"))]
        full_content
    } else {
        full_content
    };

    Ok(Json(DocumentContentResponse {
        label: doc_label.to_string(),
        title: props.get("title").cloned().unwrap_or_default(),
        url: props.get("url").cloned().unwrap_or_default(),
        content,
        mime_type: mime.as_str().to_string(),
    }))
}
