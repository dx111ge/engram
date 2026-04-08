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
/// Runs a post-pass to detect space-aligned tables and convert to markdown pipe tables.
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

    Ok(convert_space_tables_to_markdown(&text))
}

/// Detect space-aligned tabular data in PDF-extracted text and convert to markdown pipe tables.
/// Looks for runs of 3+ consecutive lines where each line has 2+ multi-space gaps (columns).
fn convert_space_tables_to_markdown(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < lines.len() {
        // Check if this line looks tabular: has 2+ gaps of 2+ spaces between non-empty tokens
        if is_tabular_line(lines[i]) {
            // Collect consecutive tabular lines
            let table_start = i;
            while i < lines.len() && (is_tabular_line(lines[i]) || lines[i].trim().is_empty()) {
                // Allow one blank line within a table, but not two
                if lines[i].trim().is_empty() {
                    if i + 1 < lines.len() && is_tabular_line(lines[i + 1]) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            let table_end = i;

            // Only convert if we found 3+ tabular lines (header + data)
            let table_lines: Vec<&str> = lines[table_start..table_end]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .copied()
                .collect();

            if table_lines.len() >= 3 {
                // Convert to markdown pipe table
                result.push_str("\n[Table]\n");
                for (ri, line) in table_lines.iter().enumerate() {
                    let cells = split_by_multi_space(line);
                    result.push_str("| ");
                    result.push_str(&cells.join(" | "));
                    result.push_str(" |\n");
                    if ri == 0 {
                        result.push('|');
                        for _ in &cells {
                            result.push_str(" --- |");
                        }
                        result.push('\n');
                    }
                }
                result.push('\n');
            } else {
                // Not enough lines for a table, keep as-is
                for line in &lines[table_start..table_end] {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        } else {
            result.push_str(lines[i]);
            result.push('\n');
            i += 1;
        }
    }

    result
}

/// Check if a line looks like a table row: non-empty, has 2+ multi-space gaps.
fn is_tabular_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 10 || trimmed.is_empty() {
        return false;
    }
    // Count gaps of 2+ spaces between non-space content
    let mut gaps = 0;
    let mut in_space = false;
    let mut space_count = 0;
    for ch in trimmed.chars() {
        if ch == ' ' {
            space_count += 1;
            if space_count >= 2 && !in_space {
                gaps += 1;
                in_space = true;
            }
        } else {
            space_count = 0;
            in_space = false;
        }
    }
    // Skip lines that are all dots (table of contents)
    if trimmed.matches('.').count() > trimmed.len() / 3 {
        return false;
    }
    gaps >= 2
}

/// Split a line by multi-space gaps (2+ spaces) into cells.
fn split_by_multi_space(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let mut cells = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            let start = i;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            if i - start >= 2 && !current.is_empty() {
                cells.push(current.trim().to_string());
                current = String::new();
            } else {
                for _ in 0..(i - start) {
                    current.push(' ');
                }
            }
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }
    if !current.trim().is_empty() {
        cells.push(current.trim().to_string());
    }
    cells
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

    #[test]
    fn space_table_to_markdown() {
        let input = "Some intro text.\n\n\
                      Country       GDP (B$)    Growth %\n\
                      Russia        1800        1.3\n\
                      Ukraine       160         -6.8\n\
                      Germany       4200        0.2\n\n\
                      Some more text.";
        let result = convert_space_tables_to_markdown(input);
        assert!(result.contains("| Country | GDP (B$) | Growth % |"), "Should have pipe table header: {}", result);
        assert!(result.contains("| Russia | 1800 | 1.3 |"), "Should have Russia row: {}", result);
        assert!(result.contains("| --- |"), "Should have header separator: {}", result);
        assert!(result.contains("Some intro text."), "Should preserve non-table text");
        assert!(result.contains("Some more text."), "Should preserve trailing text");
    }

    #[test]
    fn skip_table_of_contents_dots() {
        let input = "Introduction  ............................  3\n\
                      Chapter 1  .............................  5\n\
                      Chapter 2  .............................  12\n";
        let result = convert_space_tables_to_markdown(input);
        // Table of contents lines with dots should NOT be converted to tables
        assert!(!result.contains("[Table]"), "Should not detect TOC as table: {}", result);
    }

    #[test]
    fn is_tabular_detects_columns() {
        assert!(is_tabular_line("Russia       1800        1.3"));
        assert!(is_tabular_line("Country       GDP (B$)    Growth %"));
        assert!(!is_tabular_line("Just a normal sentence without tables."));
        assert!(!is_tabular_line("Short"));
        assert!(!is_tabular_line(""));
    }
}
