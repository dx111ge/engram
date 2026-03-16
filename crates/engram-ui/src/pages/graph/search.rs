use std::collections::HashSet;

/// Generate search variations for smart search (add/remove hyphens, spaces).
pub(super) fn search_variations(query: &str) -> Vec<String> {
    let mut variations = vec![query.to_string()];
    // Try removing hyphens
    if query.contains('-') {
        variations.push(query.replace('-', ""));
        variations.push(query.replace('-', " "));
    }
    // Try adding hyphens between letter-number boundaries
    let with_hyphen = add_hyphens_at_boundaries(query);
    if with_hyphen != query {
        variations.push(with_hyphen);
    }
    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    variations.retain(|v| seen.insert(v.clone()));
    variations
}

/// Insert hyphens between letter-digit boundaries: "F16" -> "F-16"
fn add_hyphens_at_boundaries(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        result.push(c);
        if i + 1 < chars.len() {
            let next = chars[i + 1];
            if (c.is_alphabetic() && next.is_ascii_digit())
                || (c.is_ascii_digit() && next.is_alphabetic())
            {
                result.push('-');
            }
        }
    }
    result
}

pub(super) fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
