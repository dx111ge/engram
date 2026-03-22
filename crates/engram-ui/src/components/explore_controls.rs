//! Compact graph controls bar for the Explore page chat panel.
//! Replaces the sidebar Controls card with a dense inline bar.

use leptos::prelude::*;

/// Helper to extract checkbox state from a web_sys::Event.
fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}

/// Compact controls bar for Explore page (sits at top of chat panel).
#[component]
pub fn ExploreControls(
    depth: ReadSignal<u32>,
    set_depth: WriteSignal<u32>,
    min_confidence: ReadSignal<f32>,
    set_min_confidence: WriteSignal<f32>,
    show_edge_labels: ReadSignal<bool>,
    set_show_edge_labels: WriteSignal<bool>,
    temporal_current_only: ReadSignal<bool>,
    set_temporal_current_only: WriteSignal<bool>,
    edge_bundling: ReadSignal<bool>,
    set_edge_bundling: WriteSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="explore-controls"
            style="flex-shrink:0;padding:0.4rem 0.6rem;border-bottom:1px solid var(--border, #2d3139);\
                   background:var(--bg-tertiary, #232730);display:flex;align-items:center;\
                   gap:0.5rem;flex-wrap:wrap;">
            // Depth
            <span style="font-size:0.68rem;color:var(--text-muted, #8b8fa3);white-space:nowrap;">
                "Depth " <strong style="color:var(--text, #c9ccd3);">{move || depth.get().to_string()}</strong>
            </span>
            <input type="range" min="1" max="5" step="1"
                style="width:50px;height:4px;accent-color:var(--accent, #4a9eff);"
                prop:value=move || depth.get().to_string()
                on:input=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse() {
                        set_depth.set(v);
                    }
                }
            />
            // Confidence
            <span style="font-size:0.68rem;color:var(--text-muted, #8b8fa3);white-space:nowrap;">
                "Conf " <strong style="color:var(--text, #c9ccd3);">{move || format!("{:.0}%", min_confidence.get() * 100.0)}</strong>
            </span>
            <input type="range" min="0" max="1" step="0.05"
                style="width:50px;height:4px;accent-color:var(--accent, #4a9eff);"
                prop:value=move || min_confidence.get().to_string()
                on:input=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse() {
                        set_min_confidence.set(v);
                    }
                }
            />
            // Separator
            <span style="color:var(--border, #2d3139);">"|"</span>
            // Toggles (inline)
            <div style="display:flex;align-items:center;gap:0.5rem;">
                <label style="display:inline-flex;align-items:center;gap:0.25rem;font-size:0.68rem;\
                              color:var(--text-muted, #8b8fa3);cursor:pointer;white-space:nowrap;">
                    <input type="checkbox"
                        style="accent-color:var(--accent, #4a9eff);width:12px;height:12px;"
                        prop:checked=show_edge_labels
                        on:change=move |ev| set_show_edge_labels.set(event_target_checked(&ev))
                    />
                    "Labels"
                </label>
                <label style="display:inline-flex;align-items:center;gap:0.25rem;font-size:0.68rem;\
                              color:var(--text-muted, #8b8fa3);cursor:pointer;white-space:nowrap;">
                    <input type="checkbox"
                        style="accent-color:var(--accent, #4a9eff);width:12px;height:12px;"
                        prop:checked=temporal_current_only
                        on:change=move |ev| set_temporal_current_only.set(event_target_checked(&ev))
                    />
                    "Current only"
                </label>
                <label style="display:inline-flex;align-items:center;gap:0.25rem;font-size:0.68rem;\
                              color:var(--text-muted, #8b8fa3);cursor:pointer;white-space:nowrap;">
                    <input type="checkbox"
                        style="accent-color:var(--accent, #4a9eff);width:12px;height:12px;"
                        prop:checked=edge_bundling
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_edge_bundling.set(checked);
                            let code = format!(
                                "window.__engram_graph && window.__engram_graph.toggleBundling({})",
                                checked,
                            );
                            let _ = js_sys::eval(&code);
                        }
                    />
                    "Bundle"
                </label>
            </div>
        </div>
    }
}
