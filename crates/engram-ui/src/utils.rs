use leptos::prelude::*;

/// Returns a CSS color string for a confidence value (0.0 - 1.0).
pub fn confidence_color(c: f32) -> &'static str {
    if c >= 0.7 {
        "var(--confidence-high)"
    } else if c >= 0.4 {
        "var(--confidence-mid)"
    } else {
        "var(--confidence-low)"
    }
}

/// Returns a hex color for canvas rendering (vis.js can't use CSS variables).
pub fn confidence_color_hex(c: f32) -> &'static str {
    if c >= 0.7 {
        "#00b894"
    } else if c >= 0.4 {
        "#fdcb6e"
    } else {
        "#d63031"
    }
}

/// Renders a visual confidence bar.
pub fn confidence_bar(c: f32) -> impl IntoView {
    let pct = format!("{}%", (c * 100.0).round());
    let color = confidence_color(c);
    let style = format!("width: {pct}; background: {color};");
    view! {
        <div class="confidence-bar">
            <div class="confidence-bar-fill" style=style></div>
        </div>
    }
}

/// Renders a memory tier badge (Core/Active/Archival).
pub fn tier_badge(c: f32) -> impl IntoView {
    let (label, class) = if c >= 0.7 {
        ("Core", "badge badge-core")
    } else if c >= 0.3 {
        ("Active", "badge badge-active")
    } else {
        ("Archival", "badge badge-archival")
    };
    view! { <span class=class>{label}</span> }
}

/// Renders a strength badge (Strong/Moderate/Weak).
pub fn strength_badge(c: f32) -> impl IntoView {
    let (label, class) = if c >= 0.7 {
        ("Strong", "badge badge-core")
    } else if c >= 0.4 {
        ("Moderate", "badge badge-active")
    } else {
        ("Weak", "badge badge-archival")
    };
    view! { <span class=class>{label}</span> }
}

/// Format a timestamp (seconds since epoch) as a human-readable relative time.
pub fn format_time_ago(timestamp: i64) -> String {
    let now = js_sys::Date::now() as i64 / 1000;
    let diff = now - timestamp;
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}
