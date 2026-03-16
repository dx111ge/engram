mod info;
mod connections;
mod investigate;
mod edit;

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::ApiClient;
use crate::api::types::NodeResponse;

use info::render_info_tab;
use connections::render_connections_tab;
use investigate::render_investigate_tab;
use edit::render_edit_tab;

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
                    <button
                        class=move || if active_tab.get() == "edit" { "btn btn-sm btn-primary" } else { "btn btn-sm btn-secondary" }
                        style="border-radius: 0; border: none; border-bottom: 2px solid transparent; padding: 0.5rem 1rem;"
                        style:border-bottom-color=move || if active_tab.get() == "edit" { "var(--accent-bright)" } else { "transparent" }
                        on:click=move |_| set_active_tab.set("edit".to_string())
                    >
                        <i class="fa-solid fa-pen-to-square"></i>" Edit"
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
                                render_investigate_tab(d, api.clone())
                            }
                            "edit" => {
                                render_edit_tab(d, api.clone(), on_close, set_detail)
                            }
                            _ => view! { <div></div> }.into_any(),
                        }
                    }}
                </div>
            </div>
        </div>
    }
}
