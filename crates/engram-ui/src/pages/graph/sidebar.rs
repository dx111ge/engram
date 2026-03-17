use leptos::prelude::*;

use crate::api::types::NodeHit;
use super::search::event_target_checked;

/// Renders the search results list (shown when smart search finds no traverse match).
pub(super) fn search_results_view(
    search_results: ReadSignal<Vec<NodeHit>>,
    show_results_list: ReadSignal<bool>,
    set_show_results_list: WriteSignal<bool>,
    set_search_term: WriteSignal<String>,
    on_re_search: Callback<()>,
) -> impl IntoView {
    move || {
        if !show_results_list.get() { return None; }
        let results = search_results.get();
        if results.is_empty() { return None; }
        Some(view! {
            <div class="card">
                <h3><i class="fa-solid fa-list"></i>" Search Results"</h3>
                <p class="text-secondary" style="font-size: 0.8rem; margin-bottom: 0.5rem;">
                    {format!("{} matches found. Click to explore.", results.len())}
                </p>
                <div style="display: grid; gap: 0.25rem;">
                    {results.iter().map(|hit| {
                        let label = hit.label.clone();
                        let ntype = hit.node_type.clone().unwrap_or_else(|| "Entity".to_string());
                        let conf = hit.confidence.unwrap_or(0.5);
                        let label_click = label.clone();
                        view! {
                            <div class="prop-row" style="cursor: pointer; padding: 0.4rem 0.5rem; border-radius: 4px;"
                                title="Click to traverse from this entity"
                                on:click=move |_| {
                                    set_search_term.set(label_click.clone());
                                    set_show_results_list.set(false);
                                    on_re_search.run(());
                                }>
                                <span style="display: flex; align-items: center; gap: 0.5rem;">
                                    <strong style="font-size: 0.85rem;">{label}</strong>
                                    <span class="badge badge-active" style="font-size: 0.65rem;">{ntype}</span>
                                </span>
                                <span class="text-secondary" style="font-size: 0.75rem;">
                                    {format!("{:.0}%", conf * 100.0)}
                                </span>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </div>
        })
    }
}

/// Renders the controls card (depth, min strength, direction, layout, toggles).
pub(super) fn controls_view(
    depth: ReadSignal<u32>,
    set_depth: WriteSignal<u32>,
    min_strength: ReadSignal<f32>,
    set_min_strength: WriteSignal<f32>,
    direction: ReadSignal<String>,
    set_direction: WriteSignal<String>,
    show_edge_labels: ReadSignal<bool>,
    set_show_edge_labels: WriteSignal<bool>,
    temporal_current_only: ReadSignal<bool>,
    set_temporal_current_only: WriteSignal<bool>,
    edge_bundling: ReadSignal<bool>,
    set_edge_bundling: WriteSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="card">
            <h3><i class="fa-solid fa-sliders"></i>" Controls"</h3>
            <div class="slider-group mt-1">
                <label>
                    <span>"Depth"</span>
                    <span>{move || depth.get().to_string()}</span>
                </label>
                <input type="range" min="1" max="5" step="1"
                    prop:value=move || depth.get().to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse() {
                            set_depth.set(v);
                        }
                    }
                />
            </div>
            <div class="slider-group">
                <label>
                    <span>"Min Strength"</span>
                    <span>{move || format!("{:.0}%", min_strength.get() * 100.0)}</span>
                </label>
                <input type="range" min="0" max="1" step="0.05"
                    prop:value=move || min_strength.get().to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse() {
                            set_min_strength.set(v);
                        }
                    }
                />
            </div>
            <div class="form-group">
                <label>"Direction"</label>
                <select prop:value=direction
                    on:change=move |ev| set_direction.set(event_target_value(&ev))>
                    <option value="both">"Both directions"</option>
                    <option value="out">"Outgoing"</option>
                    <option value="in">"Incoming"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Layout"</label>
                <select>
                    <option value="forceAtlas2">"Force Atlas"</option>
                    <option value="barnesHut">"Barnes Hut"</option>
                    <option value="repulsion">"Repulsion"</option>
                </select>
            </div>
            // Edge labels toggle
            <div class="form-group" style="margin-top: 0.5rem;">
                <label class="checkbox-label">
                    <input type="checkbox"
                        prop:checked=show_edge_labels
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_show_edge_labels.set(checked);
                        }
                    />
                    " Show edge labels"
                </label>
            </div>
            // Temporal mode toggle
            <div class="form-group">
                <label class="checkbox-label">
                    <input type="checkbox"
                        prop:checked=temporal_current_only
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_temporal_current_only.set(checked);
                        }
                    />
                    " Current relations only"
                </label>
            </div>
            // Edge bundling toggle
            <div class="form-group">
                <label class="checkbox-label">
                    <input type="checkbox"
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
                    " Bundle parallel edges"
                </label>
            </div>
        </div>
    }
}

/// Renders the collapsible filters card (node types + relationship types).
pub(super) fn filters_view(
    type_counts: Signal<Vec<(String, usize)>>,
    rel_counts: Signal<Vec<(String, usize)>>,
    hidden_types: ReadSignal<Vec<String>>,
    set_hidden_types: WriteSignal<Vec<String>>,
    hidden_rels: ReadSignal<Vec<String>>,
    set_hidden_rels: WriteSignal<Vec<String>>,
    filters_open: ReadSignal<bool>,
    set_filters_open: WriteSignal<bool>,
) -> impl IntoView {
    move || {
        let tc = type_counts.get();
        let rc = rel_counts.get();
        if tc.is_empty() && rc.is_empty() { return None; }
        let hidden_t_count = hidden_types.get().len();
        let hidden_r_count = hidden_rels.get().len();
        let filter_summary = if hidden_t_count > 0 || hidden_r_count > 0 {
            format!(" ({} hidden)", hidden_t_count + hidden_r_count)
        } else { String::new() };
        Some(view! {
            <div class="card">
                <h3 style="cursor: pointer; user-select: none;" on:click=move |_| set_filters_open.set(!filters_open.get_untracked())>
                    <i class=move || if filters_open.get() { "fa-solid fa-chevron-down" } else { "fa-solid fa-chevron-right" }></i>
                    " Filters"
                    <span class="text-secondary" style="font-size: 0.75rem; font-weight: 400;">{filter_summary}</span>
                </h3>
                <div style=move || if filters_open.get() { "" } else { "display:none" }>
                    // Node types
                    {(!tc.is_empty()).then(|| view! {
                        <div style="margin-bottom: 0.5rem;">
                            <span class="text-secondary" style="font-size: 0.75rem; font-weight: 600; text-transform: uppercase;">"Node Types"</span>
                            <div class="filter-chips">
                                {tc.iter().map(|(t, count)| {
                                    let t_lower = t.to_lowercase();
                                    let t_clone = t_lower.clone();
                                    let t_display = t.clone();
                                    let count = *count;
                                    let is_hidden = {
                                        let t_check = t_lower.clone();
                                        move || hidden_types.get().contains(&t_check)
                                    };
                                    view! {
                                        <button
                                            class=move || if is_hidden() { "filter-chip filter-chip-hidden" } else { "filter-chip filter-chip-active" }
                                            on:click=move |_| {
                                                let t_val = t_clone.clone();
                                                set_hidden_types.update(|v| {
                                                    if let Some(pos) = v.iter().position(|x| *x == t_val) {
                                                        v.remove(pos);
                                                    } else {
                                                        v.push(t_val);
                                                    }
                                                });
                                            }
                                        >
                                            {t_display} " " <span class="chip-count">{count.to_string()}</span>
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })}
                    // Relationship types
                    {(!rc.is_empty()).then(|| view! {
                        <div>
                            <span class="text-secondary" style="font-size: 0.75rem; font-weight: 600; text-transform: uppercase;">"Relationships"</span>
                            <div class="filter-chips">
                                {rc.iter().map(|(r, count)| {
                                    let r_lower = r.to_lowercase();
                                    let r_clone = r_lower.clone();
                                    let r_display = r.clone();
                                    let count = *count;
                                    let is_hidden = {
                                        let r_check = r_lower.clone();
                                        move || hidden_rels.get().contains(&r_check)
                                    };
                                    view! {
                                        <button
                                            class=move || if is_hidden() { "filter-chip filter-chip-hidden" } else { "filter-chip filter-chip-active" }
                                            on:click=move |_| {
                                                let r_val = r_clone.clone();
                                                set_hidden_rels.update(|v| {
                                                    if let Some(pos) = v.iter().position(|x| *x == r_val) {
                                                        v.remove(pos);
                                                    } else {
                                                        v.push(r_val);
                                                    }
                                                });
                                            }
                                        >
                                            {r_display} " " <span class="chip-count">{count.to_string()}</span>
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })}
                </div>
            </div>
        })
    }
}

/// Renders the Find Path sidebar section.
pub(super) fn find_path_view(
    nodes: ReadSignal<Vec<serde_json::Value>>,
    path_from: ReadSignal<Option<String>>,
    set_path_from: WriteSignal<Option<String>>,
    path_target_query: ReadSignal<String>,
    set_path_target_query: WriteSignal<String>,
    path_autocomplete_open: ReadSignal<bool>,
    set_path_autocomplete_open: WriteSignal<bool>,
    path_results: ReadSignal<Vec<Vec<String>>>,
    set_path_results: WriteSignal<Vec<Vec<String>>>,
    path_selected: ReadSignal<Vec<bool>>,
    set_path_selected: WriteSignal<Vec<bool>>,
) -> impl IntoView {
    move || {
        let pf = path_from.get();
        if pf.is_none() { return None; }
        let from_label = pf.unwrap();
        let results = path_results.get();
        let selected = path_selected.get();

        // Autocomplete: filter node labels by typed query
        // First check loaded nodes, then fall back to all graph nodes via search
        let tq = path_target_query.get();
        let ac_open = path_autocomplete_open.get();
        let suggestions: Vec<String> = if tq.len() >= 2 && ac_open {
            let tq_lower = tq.to_lowercase();
            // Search loaded graph nodes first
            let mut local: Vec<String> = nodes.get().iter()
                .filter_map(|n| n.get("label").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .filter(|l| l.to_lowercase().contains(&tq_lower) && l != &from_label)
                .collect();
            // Also search all graph nodes via fulltext index (sync XHR for simplicity)
            let search_code = format!(
                r#"(function(){{
                    var xhr = new XMLHttpRequest();
                    xhr.open('POST', '/search', false);
                    xhr.setRequestHeader('Content-Type', 'application/json');
                    var token = localStorage.getItem('engram_token');
                    if (token) xhr.setRequestHeader('Authorization', 'Bearer ' + token);
                    xhr.send(JSON.stringify({{query: "{}", limit: 10}}));
                    if (xhr.status === 200) {{
                        var data = JSON.parse(xhr.responseText);
                        return JSON.stringify(data.results.map(function(r){{ return r.label; }}));
                    }}
                    return '[]';
                }})()"#,
                tq.replace('"', r#"\""#).replace('\\', r#"\\"#),
            );
            if let Ok(result) = js_sys::eval(&search_code) {
                if let Some(s) = result.as_string() {
                    if let Ok(labels) = serde_json::from_str::<Vec<String>>(&s) {
                        for l in labels {
                            if l.to_lowercase().contains(&tq_lower) && l != from_label && !local.contains(&l) {
                                local.push(l);
                            }
                        }
                    }
                }
            }
            local.truncate(8);
            local
        } else {
            Vec::new()
        };

        let from_label_for_find = from_label.clone();
        Some(view! {
            <div class="card">
                <h3><i class="fa-solid fa-route"></i>" Find Path"</h3>
                <div class="prop-row mt-1">
                    <span class="prop-key">"From"</span>
                    <strong style="font-size: 0.85rem;">{from_label.clone()}</strong>
                </div>
                <div class="form-group" style="margin-top: 0.5rem; position: relative;">
                    <label>"To"</label>
                    <input type="text" placeholder="Type entity name..."
                        prop:value=path_target_query
                        on:input=move |ev| {
                            set_path_target_query.set(event_target_value(&ev));
                            set_path_autocomplete_open.set(true);
                        }
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" {
                                set_path_autocomplete_open.set(false);
                                // Trigger the Find Paths button click
                            }
                        }
                    />
                    // Autocomplete dropdown
                    {(!suggestions.is_empty()).then(|| view! {
                        <div class="path-autocomplete">
                            {suggestions.iter().map(|s| {
                                let s_click = s.clone();
                                let s_display = s.clone();
                                view! {
                                    <div class="path-autocomplete-item"
                                        on:click=move |_| {
                                            set_path_target_query.set(s_click.clone());
                                            set_path_autocomplete_open.set(false);
                                        }>
                                        {s_display}
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    })}
                </div>
                <button class="btn btn-sm btn-primary" style="margin-top: 0.5rem;"
                    on:click=move |_| {
                        let target = path_target_query.get_untracked();
                        let from = path_from.get_untracked().unwrap_or_default();
                        if !target.is_empty() && !from.is_empty() {
                            // Call server-side path finding API
                            let code = format!(
                                r#"(function(){{
                                    var xhr = new XMLHttpRequest();
                                    xhr.open('POST', '/paths', false);
                                    xhr.setRequestHeader('Content-Type', 'application/json');
                                    var token = localStorage.getItem('engram_token');
                                    if (token) xhr.setRequestHeader('Authorization', 'Bearer ' + token);
                                    xhr.send(JSON.stringify({{from: "{from}", to: "{to}", max_depth: 5}}));
                                    if (xhr.status === 200) {{
                                        var data = JSON.parse(xhr.responseText);
                                        return JSON.stringify(data.paths);
                                    }}
                                    return '[]';
                                }})()"#,
                                from = from.replace('"', r#"\""#).replace('\\', r#"\\"#),
                                to = target.replace('"', r#"\""#).replace('\\', r#"\\"#),
                            );
                            if let Ok(result) = js_sys::eval(&code) {
                                if let Some(s) = result.as_string() {
                                    if let Ok(paths) = serde_json::from_str::<Vec<Vec<String>>>(&s) {
                                        let sel = vec![true; paths.len()];
                                        set_path_results.set(paths.clone());
                                        set_path_selected.set(sel);
                                        let paths_json = serde_json::to_string(&paths).unwrap_or_default();
                                        let show_code = format!(
                                            "window.__engram_graph.showPaths('{}')",
                                            paths_json.replace('\'', "\\'"),
                                        );
                                        let _ = js_sys::eval(&show_code);
                                    }
                                }
                            }
                        }
                    }>
                    <i class="fa-solid fa-magnifying-glass"></i>" Find Paths"
                </button>

                // Path results
                {(!results.is_empty()).then(|| {
                    view! {
                        <div style="margin-top: 0.75rem;">
                            <span class="text-secondary" style="font-size: 0.75rem; text-transform: uppercase;">"Paths Found"</span>
                            <div style="display: grid; gap: 0.25rem; margin-top: 0.25rem;">
                                {results.iter().enumerate().map(|(idx, path)| {
                                    let hops = path.len().saturating_sub(1);
                                    let path_str = path.join(" > ");
                                    let path_title = path_str.clone();
                                    let is_selected = selected.get(idx).copied().unwrap_or(true);
                                    view! {
                                        <div class="prop-row" style="cursor: pointer; padding: 0.35rem 0.5rem; border-radius: 4px; font-size: 0.8rem;"
                                            on:click=move |_| {
                                                set_path_selected.update(|v| {
                                                    if let Some(s) = v.get_mut(idx) {
                                                        *s = !*s;
                                                    }
                                                });
                                                // Re-show only selected paths
                                                let paths = path_results.get_untracked();
                                                let sel = path_selected.get_untracked();
                                                let active: Vec<&Vec<String>> = paths.iter().enumerate()
                                                    .filter(|(i, _)| sel.get(*i).copied().unwrap_or(false))
                                                    .map(|(_, p)| p)
                                                    .collect();
                                                let paths_json = serde_json::to_string(&active).unwrap_or_default();
                                                let code = format!(
                                                    "window.__engram_graph.showPaths('{}')",
                                                    paths_json.replace('\'', "\\'"),
                                                );
                                                let _ = js_sys::eval(&code);
                                            }>
                                            <span style="display: flex; align-items: center; gap: 0.5rem;">
                                                <i class=move || if is_selected { "fa-solid fa-square-check" } else { "fa-regular fa-square" }
                                                    style="color: var(--accent-bright);"></i>
                                                <span>{format!("Path {} ({} hop{})", idx + 1, hops, if hops != 1 { "s" } else { "" })}</span>
                                            </span>
                                            <span class="text-secondary" style="font-size: 0.7rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 200px;"
                                                title=path_title>
                                                {path_str}
                                            </span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                            <button class="btn btn-sm btn-secondary" style="margin-top: 0.5rem;"
                                on:click=move |_| {
                                    set_path_from.set(None);
                                    set_path_results.set(Vec::new());
                                    set_path_selected.set(Vec::new());
                                    set_path_target_query.set(String::new());
                                    let _ = js_sys::eval("window.__engram_graph.clearPath()");
                                }>
                                <i class="fa-solid fa-xmark"></i>" Clear Paths"
                            </button>
                        </div>
                    }
                })}
            </div>
        })
    }
}

/// Renders the compact node preview panel in the sidebar.
pub(super) fn node_preview_view(
    node_detail: ReadSignal<Option<crate::api::types::NodeResponse>>,
    set_detail_node_id: WriteSignal<Option<String>>,
    set_detail_modal_open: WriteSignal<bool>,
    set_start_node: WriteSignal<Option<String>>,
    set_path_from: WriteSignal<Option<String>>,
    set_path_results: WriteSignal<Vec<Vec<String>>>,
    set_path_selected: WriteSignal<Vec<bool>>,
    set_path_target_query: WriteSignal<String>,
) -> impl IntoView {
    (
        move || if node_detail.get().is_none() {
            Some(view! {
                <div class="card">
                    <h3><i class="fa-solid fa-circle-info"></i>" DETAILS"</h3>
                    <p class="text-secondary">"Click a node in the graph to see its details here."</p>
                </div>
            })
        } else { None },
        move || node_detail.get().map(|detail| {
            let label = detail.label.clone();
            let label_for_start = label.clone();
            let label_for_open = label.clone();
            view! {
                <div class="card node-info-panel">
                    <h3>{detail.label.clone()}</h3>
                    {detail.node_type.clone().map(|t| view! {
                        <span class="badge badge-active">{t}</span>
                    })}
                    <div class="prop-row mt-1">
                        <span class="prop-key">"Confidence"</span>
                        <span>{format!("{:.0}%", detail.confidence * 100.0)}</span>
                    </div>
                    <div class="prop-row">
                        <span class="prop-key">"Connections"</span>
                        <span>{format!("{} out / {} in", detail.edges_from.len(), detail.edges_to.len())}</span>
                    </div>
                    <div class="node-actions mt-1">
                        <button class="btn btn-sm btn-primary" title="Open full detail modal"
                            on:click=move |_| {
                                set_detail_node_id.set(Some(label_for_open.clone()));
                                set_detail_modal_open.set(true);
                            }>
                            <i class="fa-solid fa-expand"></i>" Open"
                        </button>
                        <button class="btn btn-sm btn-secondary" title="Set as start node"
                            on:click=move |_| {
                                set_start_node.set(Some(label_for_start.clone()));
                            }>
                            <i class="fa-solid fa-bullseye"></i>" Start"
                        </button>
                        <button class="btn btn-sm btn-secondary" title="Find paths from this node"
                            on:click={
                                let lbl = label.clone();
                                move |_| {
                                    set_path_from.set(Some(lbl.clone()));
                                    set_path_results.set(Vec::new());
                                    set_path_selected.set(Vec::new());
                                    set_path_target_query.set(String::new());
                                }
                            }>
                            <i class="fa-solid fa-route"></i>" Path"
                        </button>
                    </div>
                </div>
            }
        }),
    )
}
