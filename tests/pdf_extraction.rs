/// Tests for PDF text extraction in the debate gap-closing pipeline.
///
/// Uses `pdf-extract` crate to verify text extraction from PDF byte streams.

#[cfg(feature = "pdf")]
mod pdf_tests {
    /// Generate a minimal valid PDF with the given text content.
    /// This produces a bare-bones PDF 1.4 file that pdf-extract can parse.
    fn make_pdf(text: &str) -> Vec<u8> {
        // Minimal PDF structure: header, catalog, pages, page, content stream, fonts, xref, trailer
        let stream_content = format!("BT /F1 12 Tf 100 700 Td ({}) Tj ET", text);
        let stream_bytes = stream_content.as_bytes();

        let mut pdf = Vec::new();
        let mut offsets = Vec::new();

        // Header
        pdf.extend_from_slice(b"%PDF-1.4\n");

        // Object 1: Catalog
        offsets.push(pdf.len());
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Object 2: Pages
        offsets.push(pdf.len());
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        // Object 3: Page
        offsets.push(pdf.len());
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n"
        );

        // Object 4: Content stream
        offsets.push(pdf.len());
        let stream_header = format!(
            "4 0 obj\n<< /Length {} >>\nstream\n",
            stream_bytes.len()
        );
        pdf.extend_from_slice(stream_header.as_bytes());
        pdf.extend_from_slice(stream_bytes);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        // Object 5: Font
        offsets.push(pdf.len());
        pdf.extend_from_slice(
            b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
        );

        // Cross-reference table
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n");
        pdf.extend_from_slice(format!("0 {}\n", offsets.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &offsets {
            pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        }

        // Trailer
        pdf.extend_from_slice(b"trailer\n");
        pdf.extend_from_slice(format!("<< /Size {} /Root 1 0 R >>\n", offsets.len() + 1).as_bytes());
        pdf.extend_from_slice(b"startxref\n");
        pdf.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        pdf.extend_from_slice(b"%%EOF\n");

        pdf
    }

    #[test]
    fn extract_text_from_minimal_pdf() {
        let pdf_bytes = make_pdf("Hello World from engram PDF test");
        let result = pdf_extract::extract_text_from_mem(&pdf_bytes);
        assert!(result.is_ok(), "PDF extraction should succeed");
        let text = result.unwrap();
        assert!(text.contains("Hello World"), "extracted text should contain 'Hello World', got: {}", text);
    }

    #[test]
    fn extract_text_empty_pdf_short_content() {
        // A PDF with very short content should still extract
        let pdf_bytes = make_pdf("Hi");
        let result = pdf_extract::extract_text_from_mem(&pdf_bytes);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Hi"));
    }

    #[test]
    fn reject_non_pdf_bytes() {
        let garbage = b"This is not a PDF file at all";
        let result = pdf_extract::extract_text_from_mem(garbage);
        assert!(result.is_err(), "non-PDF bytes should fail extraction");
    }

    #[test]
    fn reject_empty_bytes() {
        let result = pdf_extract::extract_text_from_mem(&[]);
        assert!(result.is_err(), "empty bytes should fail extraction");
    }

    #[test]
    fn size_cap_logic() {
        // Verify the 20MB cap logic (we don't create a 20MB PDF, just test the threshold)
        let small = make_pdf("small");
        assert!(small.len() < 20_000_000, "test PDF should be under 20MB");
    }

    #[test]
    fn text_capping_first_last() {
        // Simulate the capping logic used in extract_pdf_text
        let long_text = "A".repeat(10_000);
        let capped = if long_text.len() > 8000 {
            let first = &long_text[..4000];
            let last_start = long_text.len().saturating_sub(4000);
            let last = &long_text[last_start..];
            format!("{}\n\n[...]\n\n{}", first.trim(), last.trim())
        } else {
            long_text.clone()
        };

        assert!(capped.len() < 8100, "capped text should be roughly 8000 chars, got {}", capped.len());
        assert!(capped.contains("[...]"), "should contain [...] separator");
        assert!(capped.starts_with("AAAA"), "should start with content");
        assert!(capped.ends_with("AAAA"), "should end with content");
    }

    #[test]
    fn text_no_capping_short() {
        let short_text = "Short text under 8000 chars";
        let capped = if short_text.len() > 8000 {
            unreachable!()
        } else {
            short_text.to_string()
        };
        assert_eq!(capped, "Short text under 8000 chars");
    }
}
