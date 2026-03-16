use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::ApiClient;
use crate::api::types::NodeResponse;

#[component]
pub fn DetailModal(
    #[prop(into)] open: Signal<bool>,
    #[prop(into)] node_id: Signal<Option<String>>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (detail, set_detail) = signal(Option::<NodeResponse>::None);
    let (loading, set_loading) = signal(false);
    let (active_tab, set_active_tab) = signal("info".to_string());
    // Internal node_id for navigation within the modal
    let (current_node_id, set_current_node_id) = signal(Option::<String>::None);

    // Sync external node_id prop into current_node_id
    Effect::new(move || {
        let nid = node_id.get();
        set_current_node_id.set(nid);
    });

    // Fetch node details whenever current_node_id changes and modal is open
    let api_fetch = api.clone();
    Effect::new(move || {
        let is_open = open.get();
        let nid = current_node_id.get();
        if is_open {
            if let Some(id) = nid {
                let api = api_fetch.clone();
                set_loading.set(true);
                set_detail.set(None);
                wasm_bindgen_futures::spawn_local(async move {
                    let encoded = js_sys::encode_uri_component(&id);
                    match api.get::<NodeResponse>(&format!("/node/{encoded}")).await {
                        Ok(d) => set_detail.set(Some(d)),
                        Err(_) => set_detail.set(None),
                    }
                    set_loading.set(false);
                });
            }
        }
    });

    // Reset tab when modal opens
    Effect::new(move || {
        if open.get() {
            set_active_tab.set("info".to_string());
        }
    });

    let on_close_click = move |_| {
        on_close.run(());
    };

    // Overlay click (close when clicking backdrop)
    let on_overlay_click = move |ev: web_sys::MouseEvent| {
        // Only close if clicking the overlay itself, not modal content
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
                if el.class_list().contains("modal-overlay") {
                    on_close.run(());
                }
            }
        }
    };

    view! {
        <div
            class="modal-overlay"
            style=move || if open.get() { "display: flex;" } else { "display: none;" }
            on:click=on_overlay_click
        >
            <div class="wizard-modal" style="width: 900px; max-width: 95vw; max-height: 85vh; display: flex; flex-direction: column;">
                // Header
                {move || {
                    let d = detail.get();
                    let label = d.as_ref().map(|d| d.label.clone()).unwrap_or_default();
                    let ntype = d.as_ref().and_then(|d| d.node_type.clone());
                    let conf = d.as_ref().map(|d| d.confidence).unwrap_or(0.0);
                    view! {
                        <div class="wizard-modal-header" style="justify-content: space-between;">
                            <div style="display: flex; align-items: center; gap: 0.75rem;">
                                <h3>
                                    <i class="fa-solid fa-circle-nodes" style="margin-right: 0.5rem;"></i>
                                    {label}
                                </h3>
                                {ntype.map(|t| view! {
                                    <span class="badge badge-active">{t}</span>
                                })}
                                <span class="text-secondary" style="font-size: 0.85rem;">
                                    {format!("{:.0}%", conf * 100.0)}
                                </span>
                            </div>
                            <button class="btn btn-sm btn-secondary" on:click=on_close_click
                                style="min-width: auto; padding: 0.25rem 0.5rem;">
                                <i class="fa-solid fa-xmark"></i>
                            </button>
                        </div>
                    }
                }}

                // Tab bar
                <div style="display: flex; gap: 0; border-bottom: 1px solid var(--border); background: var(--bg-card);">
                    <button
                        class=move || if active_tab.get() == "info" { "btn btn-sm btn-primary" } else { "btn btn-sm btn-secondary" }
                        style="border-radius: 0; border: none; border-bottom: 2px solid transparent; padding: 0.5rem 1rem;"
                        style:border-bottom-color=move || if active_tab.get() == "info" { "var(--accent-bright)" } else { "transparent" }
                        on:click=move |_| set_active_tab.set("info".to_string())
                    >
                        <i class="fa-solid fa-circle-info"></i>" Info"
                    </button>
                    <button
                        class=move || if active_tab.get() == "connections" { "btn btn-sm btn-primary" } else { "btn btn-sm btn-secondary" }
                        style="border-radius: 0; border: none; border-bottom: 2px solid transparent; padding: 0.5rem 1rem;"
                        style:border-bottom-color=move || if active_tab.get() == "connections" { "var(--accent-bright)" } else { "transparent" }
                        on:click=move |_| set_active_tab.set("connections".to_string())
                    >
                        <i class="fa-solid fa-diagram-project"></i>" Connections"
                    </button>
                    <button
                        class=move || if active_tab.get() == "investigate" { "btn btn-sm btn-primary" } else { "btn btn-sm btn-secondary" }
                        style="border-radius: 0; border: none; border-bottom: 2px solid transparent; padding: 0.5rem 1rem;"
                        style:border-bottom-color=move || if active_tab.get() == "investigate" { "var(--accent-bright)" } else { "transparent" }
                        on:click=move |_| set_active_tab.set("investigate".to_string())
                    >
                        <i class="fa-solid fa-magnifying-glass-chart"></i>" Investigate"
                    </button>
                </div>

                // Body
                <div class="wizard-modal-body" style="flex: 1; overflow-y: auto;">
                    {move || {
                        if loading.get() {
                            return view! {
                                <div style="display: flex; align-items: center; justify-content: center; padding: 2rem;">
                                    <span class="spinner"></span>
                                    <span style="margin-left: 0.75rem;">"Loading..."</span>
                                </div>
                            }.into_any();
                        }
                        let d = detail.get();
                        if d.is_none() {
                            return view! {
                                <div style="padding: 2rem; text-align: center;">
                                    <p class="text-secondary">"No data available."</p>
                                </div>
                            }.into_any();
                        }
                        let d = d.unwrap();
                        let tab = active_tab.get();

                        match tab.as_str() {
                            "info" => {
                                render_info_tab(d)
                            }
                            "connections" => {
                                render_connections_tab(d, set_current_node_id)
                            }
                            "investigate" => {
                                view! {
                                    <div style="display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 3rem 1rem; text-align: center;">
                                        <i class="fa-solid fa-flask" style="font-size: 3rem; color: var(--accent-bright); margin-bottom: 1rem; opacity: 0.6;"></i>
                                        <h3 style="margin-bottom: 0.5rem;">"Coming Soon"</h3>
                                        <p class="text-secondary" style="max-width: 400px;">
                                            "Research, enrich, and ingest new facts about this entity. The investigation workflow will combine web search, knowledge base lookups, and guided entity review in a single stepped flow."
                                        </p>
                                    </div>
                                }.into_any()
                            }
                            _ => view! { <div></div> }.into_any(),
                        }
                    }}
                </div>
            </div>
        </div>
    }
}

fn render_info_tab(detail: NodeResponse) -> leptos::prelude::AnyView {
    let confidence = detail.confidence;
    let conf_pct = confidence * 100.0;
    let conf_color = if confidence >= 0.7 {
        "#66bb6a"
    } else if confidence >= 0.4 {
        "#ffa726"
    } else {
        "#ef5350"
    };

    // Extract properties
    let props: Vec<(String, String)> = detail
        .properties
        .as_ref()
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let val = if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    };
                    (k.clone(), val)
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract KB ID if present
    let kb_id = detail
        .properties
        .as_ref()
        .and_then(|p| p.get("kb_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract ingest source
    let ingest_source = detail
        .properties
        .as_ref()
        .and_then(|p| p.get("ingest_source"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    view! {
        <div>
            // Confidence bar
            <div style="margin-bottom: 1rem;">
                <div style="display: flex; justify-content: space-between; margin-bottom: 0.25rem;">
                    <span class="text-secondary" style="font-size: 0.8rem;">"Confidence"</span>
                    <span style="font-size: 0.8rem; font-weight: 600;">{format!("{:.0}%", conf_pct)}</span>
                </div>
                <div style="height: 6px; background: var(--bg-tertiary); border-radius: 3px; overflow: hidden;">
                    <div style=format!("height: 100%; width: {:.0}%; background: {}; border-radius: 3px; transition: width 0.3s;", conf_pct, conf_color)></div>
                </div>
            </div>

            // Provenance
            {ingest_source.map(|src| view! {
                <div class="prop-row" style="margin-bottom: 0.5rem;">
                    <span class="prop-key"><i class="fa-solid fa-file-import" style="margin-right: 0.25rem;"></i>"Source"</span>
                    <span style="font-size: 0.85rem;">{src}</span>
                </div>
            })}

            // KB Link
            {kb_id.map(|id| {
                let url = if id.starts_with('Q') {
                    format!("https://www.wikidata.org/wiki/{}", id)
                } else {
                    id.clone()
                };
                view! {
                    <div class="prop-row" style="margin-bottom: 0.5rem;">
                        <span class="prop-key"><i class="fa-solid fa-link" style="margin-right: 0.25rem;"></i>"KB Link"</span>
                        <a href=url.clone() target="_blank" rel="noopener" style="font-size: 0.85rem; color: var(--accent-bright);">
                            {id}
                            " " <i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 0.7rem;"></i>
                        </a>
                    </div>
                }
            })}

            // Properties table
            {if props.is_empty() {
                view! {
                    <p class="text-secondary" style="font-size: 0.85rem;">"No properties stored for this entity."</p>
                }.into_any()
            } else {
                view! {
                    <div>
                        <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                            <i class="fa-solid fa-table-list" style="margin-right: 0.25rem;"></i>"Properties"
                        </h4>
                        <div style="display: grid; gap: 0.25rem;">
                            {props.iter().map(|(k, v)| view! {
                                <div class="prop-row">
                                    <span class="prop-key">{k.clone()}</span>
                                    <span style="font-size: 0.85rem; word-break: break-word;">{v.clone()}</span>
                                </div>
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
    .into_any()
}

fn render_connections_tab(
    detail: NodeResponse,
    set_current_node_id: WriteSignal<Option<String>>,
) -> leptos::prelude::AnyView {
    let edges_from = detail.edges_from.clone();
    let edges_to = detail.edges_to.clone();

    view! {
        <div>
            // Outgoing edges
            <div style="margin-bottom: 1.5rem;">
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-arrow-right" style="margin-right: 0.25rem;"></i>
                    "Outgoing (" {edges_from.len().to_string()} ")"
                </h4>
                {if edges_from.is_empty() {
                    view! { <p class="text-secondary" style="font-size: 0.85rem;">"No outgoing connections."</p> }.into_any()
                } else {
                    view! {
                        <div style="display: grid; gap: 0.25rem;">
                            {edges_from.iter().map(|e| {
                                let to = e.to.clone();
                                let rel = e.relationship.clone();
                                let conf = e.confidence;
                                let to_click = to.clone();
                                view! {
                                    <div class="prop-row" style="cursor: pointer; padding: 0.35rem 0.5rem; border-radius: 4px;"
                                        title="Click to navigate"
                                        on:click=move |_| {
                                            set_current_node_id.set(Some(to_click.clone()));
                                        }>
                                        <span style="font-size: 0.8rem; color: var(--accent-bright);">
                                            {rel}
                                        </span>
                                        <span style="font-size: 0.85rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <i class="fa-solid fa-arrow-right" style="font-size: 0.65rem; opacity: 0.5;"></i>
                                            <strong>{to}</strong>
                                            <span class="text-secondary" style="font-size: 0.75rem;">{format!("{:.0}%", conf * 100.0)}</span>
                                        </span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>

            // Incoming edges
            <div>
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-arrow-left" style="margin-right: 0.25rem;"></i>
                    "Incoming (" {edges_to.len().to_string()} ")"
                </h4>
                {if edges_to.is_empty() {
                    view! { <p class="text-secondary" style="font-size: 0.85rem;">"No incoming connections."</p> }.into_any()
                } else {
                    view! {
                        <div style="display: grid; gap: 0.25rem;">
                            {edges_to.iter().map(|e| {
                                let from = e.from.clone();
                                let rel = e.relationship.clone();
                                let conf = e.confidence;
                                let from_click = from.clone();
                                view! {
                                    <div class="prop-row" style="cursor: pointer; padding: 0.35rem 0.5rem; border-radius: 4px;"
                                        title="Click to navigate"
                                        on:click=move |_| {
                                            set_current_node_id.set(Some(from_click.clone()));
                                        }>
                                        <span style="font-size: 0.85rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <strong>{from}</strong>
                                            <i class="fa-solid fa-arrow-right" style="font-size: 0.65rem; opacity: 0.5;"></i>
                                        </span>
                                        <span style="font-size: 0.8rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <span style="color: var(--accent-bright);">{rel}</span>
                                            <span class="text-secondary" style="font-size: 0.75rem;">{format!("{:.0}%", conf * 100.0)}</span>
                                        </span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
    .into_any()
}
