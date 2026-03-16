use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    NodeResponse, ReinforceRequest, ReinforceResponse, CorrectRequest,
    StoreRequest, StoreResponse, RelateRequest,
};

pub(super) fn render_edit_tab(
    detail: NodeResponse,
    api: ApiClient,
    on_close: Callback<()>,
    set_detail: WriteSignal<Option<NodeResponse>>,
) -> leptos::prelude::AnyView {
    let entity_label = detail.label.clone();
    let original_confidence = detail.confidence;

    // -- Confidence slider --
    let (conf_value, set_conf_value) = signal((original_confidence * 100.0) as i32);
    let (conf_saving, set_conf_saving) = signal(false);
    let (conf_result, set_conf_result) = signal(Option::<String>::None);

    let api_conf = api.clone();
    let entity_conf = entity_label.clone();
    let do_save_confidence = Action::new_local(move |_: &()| {
        let api = api_conf.clone();
        let entity = entity_conf.clone();
        async move {
            set_conf_saving.set(true);
            set_conf_result.set(None);
            let new_pct = conf_value.get_untracked();
            let new_frac = new_pct as f32 / 100.0;
            let original = original_confidence;
            let result = if new_frac >= original {
                // Reinforce
                let body = ReinforceRequest {
                    entity: entity.clone(),
                    source: Some("ui_edit".into()),
                };
                api.post::<_, ReinforceResponse>("/reinforce", &body).await
                    .map(|r| {
                        let nc = r.new_confidence.unwrap_or(new_frac);
                        format!("Reinforced -- new confidence: {:.0}%", nc * 100.0)
                    })
                    .map_err(|e| format!("{e}"))
            } else {
                // Correct (decrease)
                let body = CorrectRequest {
                    entity: entity.clone(),
                    reason: Some("Manual confidence adjustment via UI".into()),
                    source: Some("ui_edit".into()),
                    depth: None,
                };
                api.post_text("/correct", &body).await
                    .map(|_| format!("Corrected -- confidence decreased"))
                    .map_err(|e| format!("{e}"))
            };
            match result {
                Ok(msg) => set_conf_result.set(Some(msg)),
                Err(e) => set_conf_result.set(Some(format!("Error: {e}"))),
            }
            set_conf_saving.set(false);
        }
    });

    // -- Entity type editor --
    let (type_value, set_type_value) = signal(detail.node_type.clone().unwrap_or_default());
    let (type_saving, set_type_saving) = signal(false);
    let (type_result, set_type_result) = signal(Option::<String>::None);

    let api_type = api.clone();
    let entity_type_label = entity_label.clone();
    let do_save_type = Action::new_local(move |_: &()| {
        let api = api_type.clone();
        let entity = entity_type_label.clone();
        async move {
            set_type_saving.set(true);
            set_type_result.set(None);
            let new_type = type_value.get_untracked();
            let body = StoreRequest {
                entity,
                confidence: None,
                node_type: if new_type.is_empty() { None } else { Some(new_type.clone()) },
                content: None,
                source: Some("ui_edit".into()),
                properties: None,
            };
            match api.post::<_, StoreResponse>("/store", &body).await {
                Ok(_) => set_type_result.set(Some(format!("Type updated to \"{}\"", new_type))),
                Err(e) => set_type_result.set(Some(format!("Error: {e}"))),
            }
            set_type_saving.set(false);
        }
    });

    // -- Properties editor --
    let initial_props: Vec<(String, String)> = detail
        .properties
        .as_ref()
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let val = if let Some(s) = v.as_str() { s.to_string() } else { v.to_string() };
                    (k.clone(), val)
                })
                .collect()
        })
        .unwrap_or_default();

    let (props, set_props) = signal(initial_props);
    let (new_prop_key, set_new_prop_key) = signal(String::new());
    let (new_prop_val, set_new_prop_val) = signal(String::new());
    let (props_saving, set_props_saving) = signal(false);
    let (props_result, set_props_result) = signal(Option::<String>::None);

    let api_props = api.clone();
    let entity_props = entity_label.clone();
    let do_save_props = Action::new_local(move |_: &()| {
        let api = api_props.clone();
        let entity = entity_props.clone();
        async move {
            set_props_saving.set(true);
            set_props_result.set(None);
            let current = props.get_untracked();
            let mut map = serde_json::Map::new();
            for (k, v) in &current {
                map.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            let body = StoreRequest {
                entity,
                confidence: None,
                node_type: None,
                content: None,
                source: Some("ui_edit".into()),
                properties: Some(serde_json::Value::Object(map)),
            };
            match api.post::<_, StoreResponse>("/store", &body).await {
                Ok(_) => set_props_result.set(Some("Properties saved.".into())),
                Err(e) => set_props_result.set(Some(format!("Error: {e}"))),
            }
            set_props_saving.set(false);
        }
    });

    // -- Add Relation --
    let (rel_to, set_rel_to) = signal(String::new());
    let (rel_type, set_rel_type) = signal(String::new());
    let (rel_conf, set_rel_conf) = signal(70i32);
    let (rel_saving, set_rel_saving) = signal(false);
    let (rel_result, set_rel_result) = signal(Option::<String>::None);

    let api_rel = api.clone();
    let entity_rel = entity_label.clone();
    let do_add_relation = Action::new_local(move |_: &()| {
        let api = api_rel.clone();
        let from = entity_rel.clone();
        async move {
            set_rel_saving.set(true);
            set_rel_result.set(None);
            let to = rel_to.get_untracked();
            let rtype = rel_type.get_untracked();
            let conf = rel_conf.get_untracked() as f32 / 100.0;
            if to.is_empty() || rtype.is_empty() {
                set_rel_result.set(Some("Both 'To' entity and relation type are required.".into()));
                set_rel_saving.set(false);
                return;
            }
            let body = RelateRequest {
                from,
                to: to.clone(),
                relationship: rtype.clone(),
                confidence: Some(conf),
            };
            match api.post_text("/relate", &body).await {
                Ok(_) => {
                    set_rel_result.set(Some(format!("Relation added: --[{}]--> {}", rtype, to)));
                    set_rel_to.set(String::new());
                    set_rel_type.set(String::new());
                }
                Err(e) => set_rel_result.set(Some(format!("Error: {e}"))),
            }
            set_rel_saving.set(false);
        }
    });

    // -- Delete Entity --
    let (delete_confirm, set_delete_confirm) = signal(String::new());
    let (delete_saving, set_delete_saving) = signal(false);
    let (delete_result, set_delete_result) = signal(Option::<String>::None);

    let api_del = api.clone();
    let entity_del = entity_label.clone();
    let entity_del_check = entity_label.clone();
    let do_delete = Action::new_local(move |_: &()| {
        let api = api_del.clone();
        let entity = entity_del.clone();
        async move {
            set_delete_saving.set(true);
            set_delete_result.set(None);
            let confirm_text = delete_confirm.get_untracked();
            if confirm_text != entity {
                set_delete_result.set(Some("Entity name does not match. Deletion cancelled.".into()));
                set_delete_saving.set(false);
                return;
            }
            let encoded = js_sys::encode_uri_component(&entity);
            match api.delete(&format!("/node/{encoded}")).await {
                Ok(_) => {
                    set_delete_result.set(Some("Entity deleted.".into()));
                    set_detail.set(None);
                    on_close.run(());
                }
                Err(e) => set_delete_result.set(Some(format!("Error: {e}"))),
            }
            set_delete_saving.set(false);
        }
    });

    view! {
        <div>
            // -- Confidence Slider --
            <div style="margin-bottom: 1.5rem;">
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-gauge" style="margin-right: 0.25rem;"></i>"Confidence"
                </h4>
                <div class="slider-group" style="display: flex; align-items: center; gap: 0.75rem;">
                    <input type="range" min="0" max="100" step="1"
                        prop:value=move || conf_value.get().to_string()
                        style="flex: 1;"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                set_conf_value.set(v);
                            }
                        }
                    />
                    <span style="min-width: 3rem; text-align: right; font-weight: 600;">{move || format!("{}%", conf_value.get())}</span>
                    <button class="btn btn-sm btn-primary" disabled=conf_saving
                        on:click=move |_| { do_save_confidence.dispatch(()); }>
                        {move || if conf_saving.get() {
                            view! { <span class="spinner"></span> }.into_any()
                        } else {
                            view! { <i class="fa-solid fa-floppy-disk"></i> }.into_any()
                        }}
                    </button>
                </div>
                {move || conf_result.get().map(|msg| {
                    let is_err = msg.starts_with("Error");
                    view! {
                        <div style=format!("margin-top: 0.5rem; font-size: 0.8rem; color: {};",
                            if is_err { "#ef5350" } else { "#66bb6a" })>
                            {msg}
                        </div>
                    }
                })}
            </div>

            // -- Entity Type --
            <div style="margin-bottom: 1.5rem;">
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-tag" style="margin-right: 0.25rem;"></i>"Entity Type"
                </h4>
                <div class="form-row" style="display: flex; gap: 0.5rem; align-items: center;">
                    <input type="text" class="form-group"
                        style="flex: 1; padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary);"
                        prop:value=move || type_value.get()
                        on:input=move |ev| set_type_value.set(event_target_value(&ev))
                        placeholder="e.g. Person, Organization, Location"
                    />
                    <button class="btn btn-sm btn-primary" disabled=type_saving
                        on:click=move |_| { do_save_type.dispatch(()); }>
                        {move || if type_saving.get() {
                            view! { <span class="spinner"></span> }.into_any()
                        } else {
                            view! { <><i class="fa-solid fa-floppy-disk"></i>" Save"</> }.into_any()
                        }}
                    </button>
                </div>
                {move || type_result.get().map(|msg| {
                    let is_err = msg.starts_with("Error");
                    view! {
                        <div style=format!("margin-top: 0.5rem; font-size: 0.8rem; color: {};",
                            if is_err { "#ef5350" } else { "#66bb6a" })>
                            {msg}
                        </div>
                    }
                })}
            </div>

            // -- Properties Editor --
            <div style="margin-bottom: 1.5rem;">
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-table-list" style="margin-right: 0.25rem;"></i>"Properties"
                </h4>
                <div style="display: grid; gap: 0.25rem; margin-bottom: 0.5rem;">
                    {move || {
                        let current = props.get();
                        current.iter().enumerate().map(|(idx, (k, v))| {
                            let key = k.clone();
                            let val = v.clone();
                            view! {
                                <div class="prop-row" style="display: flex; align-items: center; gap: 0.5rem;">
                                    <span class="prop-key" style="min-width: 8rem;">{key}</span>
                                    <span style="flex: 1; font-size: 0.85rem; word-break: break-word;">{val}</span>
                                    <button class="btn btn-sm btn-danger" style="min-width: auto; padding: 0.15rem 0.4rem;"
                                        on:click=move |_| {
                                            set_props.update(|v| { v.remove(idx); });
                                        }>
                                        <i class="fa-solid fa-xmark"></i>
                                    </button>
                                </div>
                            }
                        }).collect::<Vec<_>>()
                    }}
                </div>
                // Add property row
                <div class="form-row" style="display: flex; gap: 0.5rem; align-items: center;">
                    <input type="text" placeholder="Key"
                        style="flex: 1; padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary);"
                        prop:value=move || new_prop_key.get()
                        on:input=move |ev| set_new_prop_key.set(event_target_value(&ev))
                    />
                    <input type="text" placeholder="Value"
                        style="flex: 2; padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary);"
                        prop:value=move || new_prop_val.get()
                        on:input=move |ev| set_new_prop_val.set(event_target_value(&ev))
                    />
                    <button class="btn btn-sm btn-secondary" style="min-width: auto;"
                        on:click=move |_| {
                            let k = new_prop_key.get_untracked();
                            let v = new_prop_val.get_untracked();
                            if !k.is_empty() {
                                set_props.update(|list| list.push((k, v)));
                                set_new_prop_key.set(String::new());
                                set_new_prop_val.set(String::new());
                            }
                        }>
                        <i class="fa-solid fa-plus"></i>" Add"
                    </button>
                </div>
                <div style="margin-top: 0.5rem;">
                    <button class="btn btn-sm btn-primary" disabled=props_saving
                        on:click=move |_| { do_save_props.dispatch(()); }>
                        {move || if props_saving.get() {
                            view! { <><span class="spinner"></span>" Saving..."</> }.into_any()
                        } else {
                            view! { <><i class="fa-solid fa-floppy-disk"></i>" Save Properties"</> }.into_any()
                        }}
                    </button>
                </div>
                {move || props_result.get().map(|msg| {
                    let is_err = msg.starts_with("Error");
                    view! {
                        <div style=format!("margin-top: 0.5rem; font-size: 0.8rem; color: {};",
                            if is_err { "#ef5350" } else { "#66bb6a" })>
                            {msg}
                        </div>
                    }
                })}
            </div>

            // -- Add Relation --
            <div style="margin-bottom: 1.5rem;">
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-link" style="margin-right: 0.25rem;"></i>"Add Relation"
                </h4>
                <div style="display: grid; gap: 0.5rem;">
                    <div class="form-row" style="display: flex; gap: 0.5rem; align-items: center;">
                        <label class="text-secondary" style="font-size: 0.8rem; min-width: 5rem;">"To entity"</label>
                        <input type="text" placeholder="Target entity label"
                            style="flex: 1; padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary);"
                            prop:value=move || rel_to.get()
                            on:input=move |ev| set_rel_to.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-row" style="display: flex; gap: 0.5rem; align-items: center;">
                        <label class="text-secondary" style="font-size: 0.8rem; min-width: 5rem;">"Relation"</label>
                        <input type="text" placeholder="e.g. works_at, located_in, related_to"
                            style="flex: 1; padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary);"
                            prop:value=move || rel_type.get()
                            on:input=move |ev| set_rel_type.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="slider-group" style="display: flex; align-items: center; gap: 0.75rem;">
                        <label class="text-secondary" style="font-size: 0.8rem; min-width: 5rem;">"Confidence"</label>
                        <input type="range" min="0" max="100" step="1"
                            prop:value=move || rel_conf.get().to_string()
                            style="flex: 1;"
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                    set_rel_conf.set(v);
                                }
                            }
                        />
                        <span style="min-width: 3rem; text-align: right; font-size: 0.85rem;">{move || format!("{}%", rel_conf.get())}</span>
                    </div>
                    <div>
                        <button class="btn btn-sm btn-primary" disabled=rel_saving
                            on:click=move |_| { do_add_relation.dispatch(()); }>
                            {move || if rel_saving.get() {
                                view! { <><span class="spinner"></span>" Adding..."</> }.into_any()
                            } else {
                                view! { <><i class="fa-solid fa-plus"></i>" Add Relation"</> }.into_any()
                            }}
                        </button>
                    </div>
                </div>
                {move || rel_result.get().map(|msg| {
                    let is_err = msg.starts_with("Error");
                    view! {
                        <div style=format!("margin-top: 0.5rem; font-size: 0.8rem; color: {};",
                            if is_err { "#ef5350" } else { "#66bb6a" })>
                            {msg}
                        </div>
                    }
                })}
            </div>

            // -- Danger Zone: Delete Entity --
            <div style="margin-top: 2rem; padding: 1rem; border: 1px solid rgba(239, 83, 80, 0.4); border-radius: 6px; background: rgba(239, 83, 80, 0.05);">
                <h4 style="font-size: 0.85rem; color: #ef5350; margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-triangle-exclamation" style="margin-right: 0.25rem;"></i>"Danger Zone"
                </h4>
                <p class="text-secondary" style="font-size: 0.8rem; margin-bottom: 0.75rem;">
                    "Deleting this entity is permanent. Type the entity name to confirm."
                </p>
                <div class="form-row" style="display: flex; gap: 0.5rem; align-items: center;">
                    <input type="text"
                        style="flex: 1; padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid rgba(239, 83, 80, 0.3); border-radius: 4px; color: var(--text-primary);"
                        prop:value=move || delete_confirm.get()
                        on:input=move |ev| set_delete_confirm.set(event_target_value(&ev))
                        placeholder=entity_del_check.clone()
                    />
                    <button class="btn btn-sm btn-danger"
                        disabled=move || delete_saving.get() || delete_confirm.get() != entity_del_check
                        on:click=move |_| { do_delete.dispatch(()); }>
                        {move || if delete_saving.get() {
                            view! { <><span class="spinner"></span>" Deleting..."</> }.into_any()
                        } else {
                            view! { <><i class="fa-solid fa-trash"></i>" Delete Entity"</> }.into_any()
                        }}
                    </button>
                </div>
                {move || delete_result.get().map(|msg| {
                    let is_err = msg.starts_with("Error");
                    view! {
                        <div style=format!("margin-top: 0.5rem; font-size: 0.8rem; color: {};",
                            if is_err { "#ef5350" } else { "#66bb6a" })>
                            {msg}
                        </div>
                    }
                })}
            </div>
        </div>
    }
    .into_any()
}
