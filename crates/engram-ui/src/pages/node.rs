use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use leptos_router::components::A;

use crate::api::ApiClient;
use crate::api::types::{NodeResponse, ReinforceRequest, CorrectRequest};
use crate::utils::{confidence_bar, tier_badge};

#[component]
pub fn NodePage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let params = use_params_map();

    let label = move || {
        params.get().get("label").unwrap_or_default()
    };

    let (node_data, set_node_data) = signal(Option::<NodeResponse>::None);
    let (error, set_error) = signal(Option::<String>::None);
    let (action_msg, set_action_msg) = signal(Option::<String>::None);

    // Load node data
    let api_load = api.clone();
    let load_node = Action::new_local(move |lbl: &String| {
        let api = api_load.clone();
        let lbl = lbl.clone();
        async move {
            set_error.set(None);
            let encoded = js_sys::encode_uri_component(&lbl);
            match api.get::<NodeResponse>(&format!("/node/{encoded}")).await {
                Ok(node) => set_node_data.set(Some(node)),
                Err(e) => set_error.set(Some(format!("{e}"))),
            }
        }
    });

    // Trigger load on mount
    Effect::new(move |_| {
        let lbl = label();
        if !lbl.is_empty() {
            load_node.dispatch(lbl);
        }
    });

    // Reinforce action
    let api_reinforce = api.clone();
    let reinforce = Action::new_local(move |_: &()| {
        let api = api_reinforce.clone();
        let entity = label();
        async move {
            set_action_msg.set(None);
            let body = ReinforceRequest { entity, source: Some("ui".into()) };
            match api.post_text("/learn/reinforce", &body).await {
                Ok(_) => {
                    set_action_msg.set(Some("Reinforced successfully".into()));
                    load_node.dispatch(label());
                }
                Err(e) => set_action_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    // Correct action
    let (correct_reason, set_correct_reason) = signal(String::new());
    let api_correct = api.clone();
    let correct = Action::new_local(move |_: &()| {
        let api = api_correct.clone();
        let entity = label();
        let reason = correct_reason.get_untracked();
        async move {
            set_action_msg.set(None);
            let body = CorrectRequest {
                entity,
                reason: if reason.is_empty() { None } else { Some(reason) },
                source: Some("ui".into()),
                depth: None,
            };
            match api.post_text("/learn/correct", &body).await {
                Ok(_) => {
                    set_action_msg.set(Some("Marked as incorrect".into()));
                    load_node.dispatch(label());
                }
                Err(e) => set_action_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    // Delete action
    let api_del = api.clone();
    let delete_node = Action::new_local(move |_: &()| {
        let api = api_del.clone();
        let lbl = label();
        async move {
            let encoded = js_sys::encode_uri_component(&lbl);
            match api.delete(&format!("/node/{encoded}")).await {
                Ok(_) => {
                    set_action_msg.set(Some("Deleted".into()));
                    set_node_data.set(None);
                }
                Err(e) => set_action_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div class="page-header">
            <h2>
                <a href="/graph" class="text-secondary" style="margin-right: 0.5rem;">
                    <i class="fa-solid fa-arrow-left"></i>
                </a>
                <i class="fa-solid fa-circle-nodes"></i>
                " Entity: " {label}
            </h2>
        </div>

        {move || error.get().map(|e| view! {
            <div class="card" style="border-color: var(--error); margin-bottom: 1rem;">
                <i class="fa-solid fa-exclamation-triangle" style="color: var(--error);"></i>
                " " {e}
            </div>
        })}

        {move || action_msg.get().map(|m| view! {
            <div class="card" style="margin-bottom: 1rem; padding: 0.75rem;">
                <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                " " {m}
            </div>
        })}

        {move || node_data.get().map(|node| {
            let conf = node.confidence;
            let node_type = node.node_type.clone().unwrap_or_else(|| "unknown".into());
            let props = node.properties.clone();
            let edges_from = node.edges_from.clone();
            let edges_to = node.edges_to.clone();

            view! {
                <div class="node-detail-layout">
                    <div class="node-heading">
                        <h1>{node.label.clone()}</h1>
                        <div class="node-meta">
                            {tier_badge(conf)}
                            <span class="badge badge-active">{node_type}</span>
                            <span class="text-secondary">{format!("{:.0}%", conf * 100.0)}</span>
                        </div>
                    </div>

                    <div class="grid-2" style="margin-bottom: 1.5rem;">
                        <div class="card">
                            <h3><i class="fa-solid fa-chart-bar"></i>" Confidence"</h3>
                            <div style="margin-top: 0.5rem;">
                                {confidence_bar(conf)}
                            </div>
                        </div>
                        <div class="card">
                            <h3><i class="fa-solid fa-link"></i>" Connections"</h3>
                            <p style="font-size: 2rem; font-weight: 700; color: var(--accent-bright);">
                                {edges_from.len() + edges_to.len()}
                            </p>
                            <p class="text-secondary">
                                {format!("{} outgoing, {} incoming", edges_from.len(), edges_to.len())}
                            </p>
                        </div>
                    </div>

                    // Properties
                    {props.map(|p| {
                        if let Some(obj) = p.as_object() {
                            let entries: Vec<_> = obj.iter()
                                .map(|(k, v)| (k.clone(), v.to_string()))
                                .collect();
                            if entries.is_empty() {
                                None
                            } else {
                                Some(view! {
                                    <div class="card" style="margin-bottom: 1rem;">
                                        <h3><i class="fa-solid fa-tags"></i>" Properties"</h3>
                                        <table style="margin-top: 0.5rem;">
                                            <thead><tr><th>"Key"</th><th>"Value"</th></tr></thead>
                                            <tbody>
                                                {entries.into_iter().map(|(k, v)| view! {
                                                    <tr><td class="text-secondary">{k}</td><td>{v}</td></tr>
                                                }).collect::<Vec<_>>()}
                                            </tbody>
                                        </table>
                                    </div>
                                })
                            }
                        } else {
                            None
                        }
                    }).flatten()}

                    // Outgoing edges
                    <div class="card" style="margin-bottom: 1rem;">
                        <h3><i class="fa-solid fa-arrow-right"></i>" Outgoing Edges (" {edges_from.len()} ")"</h3>
                        {if edges_from.is_empty() {
                            view! { <p class="text-muted mt-1">"No outgoing edges"</p> }.into_any()
                        } else {
                            view! {
                                <ul class="edge-list">
                                    {edges_from.iter().map(|e| {
                                        let to_label = e.to.clone();
                                        let encoded = js_sys::encode_uri_component(&to_label).as_string().unwrap_or_default();
                                        view! {
                                            <li>
                                                <i class="fa-solid fa-arrow-right"></i>
                                                <span class="edge-rel">{e.relationship.clone()}</span>
                                                <A href=format!("/node/{encoded}")>{to_label}</A>
                                                <span class="text-muted">{format!("{:.0}%", e.confidence * 100.0)}</span>
                                            </li>
                                        }
                                    }).collect::<Vec<_>>()}
                                </ul>
                            }.into_any()
                        }}
                    </div>

                    // Incoming edges
                    <div class="card" style="margin-bottom: 1rem;">
                        <h3><i class="fa-solid fa-arrow-left"></i>" Incoming Edges (" {edges_to.len()} ")"</h3>
                        {if edges_to.is_empty() {
                            view! { <p class="text-muted mt-1">"No incoming edges"</p> }.into_any()
                        } else {
                            view! {
                                <ul class="edge-list">
                                    {edges_to.iter().map(|e| {
                                        let from_label = e.from.clone();
                                        let encoded = js_sys::encode_uri_component(&from_label).as_string().unwrap_or_default();
                                        view! {
                                            <li>
                                                <A href=format!("/node/{encoded}")>{from_label}</A>
                                                <span class="edge-rel">{e.relationship.clone()}</span>
                                                <i class="fa-solid fa-arrow-left"></i>
                                                <span class="text-muted">{format!("{:.0}%", e.confidence * 100.0)}</span>
                                            </li>
                                        }
                                    }).collect::<Vec<_>>()}
                                </ul>
                            }.into_any()
                        }}
                    </div>

                    // Actions
                    <div class="card">
                        <h3><i class="fa-solid fa-wand-magic-sparkles"></i>" Actions"</h3>
                        <div class="node-actions mt-1">
                            <button class="btn btn-success btn-sm" on:click=move |_| { reinforce.dispatch(()); }>
                                <i class="fa-solid fa-thumbs-up"></i>" Reinforce"
                            </button>
                            <div class="flex" style="gap: 0.5rem; align-items: center;">
                                <input
                                    type="text"
                                    placeholder="Reason for correction..."
                                    prop:value=correct_reason
                                    on:input=move |ev| set_correct_reason.set(event_target_value(&ev))
                                    style="width: 200px;"
                                />
                                <button class="btn btn-danger btn-sm" on:click=move |_| { correct.dispatch(()); }>
                                    <i class="fa-solid fa-xmark"></i>" Correct"
                                </button>
                            </div>
                            <button class="btn btn-danger btn-sm" on:click=move |_| {
                                if web_sys::window()
                                    .and_then(|w| w.confirm_with_message("Delete this entity?").ok())
                                    .unwrap_or(false)
                                {
                                    delete_node.dispatch(());
                                }
                            }>
                                <i class="fa-solid fa-trash"></i>" Delete"
                            </button>
                        </div>
                    </div>
                </div>
            }
        })}
    }
}
