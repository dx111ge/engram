//! Hand-rolled markdown-to-HTML converter for chat messages.
//! Supports: fenced code blocks, headers, lists, bold, italic, inline code, paragraphs.

/// Escape HTML special characters to prevent XSS.
pub fn html_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Convert a markdown string to HTML for rendering in chat bubbles.
pub fn markdown_to_html(input: &str) -> String {
    let escaped = html_escape(input);
    let lines: Vec<&str> = escaped.lines().collect();
    let mut html = String::with_capacity(escaped.len() * 2);
    let mut i = 0;
    let mut in_paragraph = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Fenced code block
        if trimmed.starts_with("```") {
            if in_paragraph {
                html.push_str("</p>");
                in_paragraph = false;
            }
            let lang = trimmed.strip_prefix("```").unwrap_or("").trim();
            let lang_attr = if lang.is_empty() {
                String::new()
            } else {
                format!(" data-lang=\"{}\"", lang)
            };
            html.push_str(&format!("<pre><code class=\"chat-code-block\"{}>\n", lang_attr));
            i += 1;
            while i < lines.len() {
                if lines[i].trim().starts_with("```") {
                    break;
                }
                html.push_str(lines[i]);
                html.push('\n');
                i += 1;
            }
            html.push_str("</code></pre>\n");
            i += 1;
            continue;
        }

        // Headers
        if trimmed.starts_with("#### ") {
            if in_paragraph { html.push_str("</p>"); in_paragraph = false; }
            let text = inline_format(&trimmed[5..]);
            html.push_str(&format!("<h5 class=\"chat-md-h5\">{}</h5>\n", text));
            i += 1;
            continue;
        }
        if trimmed.starts_with("### ") {
            if in_paragraph { html.push_str("</p>"); in_paragraph = false; }
            let text = inline_format(&trimmed[4..]);
            html.push_str(&format!("<h4 class=\"chat-md-h4\">{}</h4>\n", text));
            i += 1;
            continue;
        }
        if trimmed.starts_with("## ") {
            if in_paragraph { html.push_str("</p>"); in_paragraph = false; }
            let text = inline_format(&trimmed[3..]);
            html.push_str(&format!("<h3 class=\"chat-md-h3\">{}</h3>\n", text));
            i += 1;
            continue;
        }

        // Unordered list
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            if in_paragraph { html.push_str("</p>"); in_paragraph = false; }
            html.push_str("<ul class=\"chat-md-list\">\n");
            while i < lines.len() {
                let t = lines[i].trim();
                if t.starts_with("- ") {
                    html.push_str(&format!("<li>{}</li>\n", inline_format(&t[2..])));
                } else if t.starts_with("* ") {
                    html.push_str(&format!("<li>{}</li>\n", inline_format(&t[2..])));
                } else if t.is_empty() || !(t.starts_with("- ") || t.starts_with("* ") || t.starts_with("  ")) {
                    break;
                } else {
                    // Continuation line
                    html.push_str(&format!("<li>{}</li>\n", inline_format(t)));
                }
                i += 1;
            }
            html.push_str("</ul>\n");
            continue;
        }

        // Ordered list
        if trimmed.len() > 2 && trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            if let Some(dot_pos) = trimmed.find(". ") {
                if dot_pos <= 3 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                    if in_paragraph { html.push_str("</p>"); in_paragraph = false; }
                    html.push_str("<ol class=\"chat-md-list\">\n");
                    while i < lines.len() {
                        let t = lines[i].trim();
                        if let Some(dp) = t.find(". ") {
                            if dp <= 3 && t[..dp].chars().all(|c| c.is_ascii_digit()) {
                                html.push_str(&format!("<li>{}</li>\n", inline_format(&t[dp + 2..])));
                                i += 1;
                                continue;
                            }
                        }
                        if t.is_empty() {
                            break;
                        }
                        break;
                    }
                    html.push_str("</ol>\n");
                    continue;
                }
            }
        }

        // Blank line ends paragraph
        if trimmed.is_empty() {
            if in_paragraph {
                html.push_str("</p>\n");
                in_paragraph = false;
            }
            i += 1;
            continue;
        }

        // Regular text -> paragraph
        if !in_paragraph {
            html.push_str("<p class=\"chat-md-p\">");
            in_paragraph = true;
        } else {
            html.push_str("<br>");
        }
        html.push_str(&inline_format(trimmed));
        i += 1;
    }

    if in_paragraph {
        html.push_str("</p>\n");
    }

    html
}

/// Apply inline formatting: **bold**, *italic*, `code`.
fn inline_format(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Inline code: `...`
        if chars[i] == '`' {
            if let Some(end) = find_char(&chars, '`', i + 1) {
                out.push_str("<code class=\"chat-inline-code\">");
                for j in (i + 1)..end {
                    out.push(chars[j]);
                }
                out.push_str("</code>");
                i = end + 1;
                continue;
            }
        }

        // Bold: **...**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_double_char(&chars, '*', i + 2) {
                out.push_str("<strong>");
                for j in (i + 2)..end {
                    out.push(chars[j]);
                }
                out.push_str("</strong>");
                i = end + 2;
                continue;
            }
        }

        // Italic: *...*
        if chars[i] == '*' && (i + 1 < len && chars[i + 1] != '*') {
            if let Some(end) = find_char(&chars, '*', i + 1) {
                out.push_str("<em>");
                for j in (i + 1)..end {
                    out.push(chars[j]);
                }
                out.push_str("</em>");
                i = end + 1;
                continue;
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn find_char(chars: &[char], target: char, start: usize) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == target {
            return Some(i);
        }
    }
    None
}

fn find_double_char(chars: &[char], target: char, start: usize) -> Option<usize> {
    for i in start..chars.len().saturating_sub(1) {
        if chars[i] == target && chars[i + 1] == target {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;");
        assert_eq!(html_escape("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    }

    #[test]
    fn test_bold_italic_code() {
        let input = "This is **bold** and *italic* and `code`";
        let html = markdown_to_html(input);
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("<code class=\"chat-inline-code\">code</code>"));
    }

    #[test]
    fn test_headers() {
        assert!(markdown_to_html("## Title").contains("<h3 class=\"chat-md-h3\">Title</h3>"));
        assert!(markdown_to_html("### Sub").contains("<h4 class=\"chat-md-h4\">Sub</h4>"));
    }

    #[test]
    fn test_unordered_list() {
        let input = "- one\n- two\n- three";
        let html = markdown_to_html(input);
        assert!(html.contains("<ul class=\"chat-md-list\">"));
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("<li>two</li>"));
        assert!(html.contains("<li>three</li>"));
    }

    #[test]
    fn test_ordered_list() {
        let input = "1. first\n2. second";
        let html = markdown_to_html(input);
        assert!(html.contains("<ol class=\"chat-md-list\">"));
        assert!(html.contains("<li>first</li>"));
        assert!(html.contains("<li>second</li>"));
    }

    #[test]
    fn test_code_block() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        let html = markdown_to_html(input);
        assert!(html.contains("<pre><code class=\"chat-code-block\" data-lang=\"json\">"));
        assert!(html.contains("&quot;key&quot;"));
    }

    #[test]
    fn test_xss_in_markdown() {
        let input = "**<script>alert(1)</script>**";
        let html = markdown_to_html(input);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_paragraphs() {
        let input = "First paragraph.\n\nSecond paragraph.";
        let html = markdown_to_html(input);
        let p_count = html.matches("<p class=\"chat-md-p\">").count();
        assert_eq!(p_count, 2);
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(markdown_to_html(""), "");
    }
}
