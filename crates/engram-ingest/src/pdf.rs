/// PDF text extraction and pipeline parser.
///
/// Provides a shared `extract_text_from_pdf` function used by:
/// - debate gap-closing (capped at 8000 chars for speed)
/// - full document ingest (no cap, complete text)
/// - folder watch (auto-ingest PDFs from watched directories)

use crate::error::IngestError;
use crate::traits::Parser;
use crate::types::Content;

/// Extract all text from PDF bytes. No output cap -- returns the full text.
///
/// Synchronous. Callers should wrap in `tokio::task::spawn_blocking` for async use.
/// Returns an error if the PDF cannot be parsed or is empty.
pub fn extract_text_from_pdf(bytes: &[u8]) -> Result<String, IngestError> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| IngestError::Parse(format!("PDF extraction failed: {e}")))?;

    let text = text.trim().to_string();
    if text.len() < 50 {
        return Err(IngestError::Parse(format!(
            "PDF extraction too short ({} chars)",
            text.len()
        )));
    }

    Ok(text)
}

/// Cap extracted text at a max length, preserving first + last sections.
/// Used by debate gap-closing for fast-path extraction.
pub fn cap_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let half = max_chars / 2;
    let first = &text[..half];
    let last_start = text.len().saturating_sub(half);
    let last = &text[last_start..];
    format!("{}\n\n[...]\n\n{}", first.trim(), last.trim())
}

/// Parser that handles `Content::Bytes` with PDF MIME type.
///
/// Extracts text from raw PDF bytes and returns it as a single text segment.
/// The pipeline's own chunking (`chunk_text`) handles splitting for LLM fact
/// extraction downstream.
pub struct PdfParser;

impl Parser for PdfParser {
    fn parse(&self, content: &Content) -> Result<Vec<String>, IngestError> {
        match content {
            Content::Bytes { data, mime } if mime.contains("pdf") => {
                let text = extract_text_from_pdf(data)?;
                Ok(vec![text])
            }
            _ => Err(IngestError::Parse("PdfParser only handles PDF bytes".into())),
        }
    }

    fn supported_types(&self) -> Vec<String> {
        vec!["application/pdf".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_text_short() {
        let text = "Hello, world!";
        assert_eq!(cap_text(text, 100), text);
    }

    #[test]
    fn cap_text_long() {
        let text = "A".repeat(10000);
        let capped = cap_text(&text, 8000);
        // Should have first 4000 + separator + last 4000
        assert!(capped.len() < 8020);
        assert!(capped.contains("[...]"));
    }

    #[test]
    fn pdf_parser_rejects_non_pdf() {
        let parser = PdfParser;
        let content = Content::Text("hello".into());
        assert!(parser.parse(&content).is_err());
    }

    #[test]
    fn pdf_parser_rejects_non_pdf_bytes() {
        let parser = PdfParser;
        let content = Content::Bytes {
            data: vec![0, 1, 2],
            mime: "application/json".into(),
        };
        assert!(parser.parse(&content).is_err());
    }

    #[test]
    fn extract_rejects_garbage() {
        let result = extract_text_from_pdf(b"not a pdf");
        assert!(result.is_err());
    }
}
