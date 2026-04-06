use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{ActionRule, InferenceRule};

#[derive(Clone, Debug, serde::Deserialize)]
struct RuleNamesResponse {
    #[serde(default)]
    names: Vec<String>,
}

#[component]
pub fn RulesModal(
    #[prop(into)] open: Signal<bool>,
    set_modal_open: WriteSignal<String>,
    set_status_msg: WriteSignal<String>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (active_tab, set_active_tab) = signal("inference".to_string());

    // ── Inference rules ──
    let (rule_names, set_rule_names) = signal(Vec::<String>::new());
    let (new_rules_text, set_new_rules_text) = signal(String::new());

    let api_load_inf = api.clone();
    let load_inference = Action::new_local(move |_: &()| {
        let api = api_load_inf.clone();
        async move {
            match api.get::<RuleNamesResponse>("/rules").await {
                Ok(r) => set_rule_names.set(r.names),
                Err(e) => set_status_msg.set(format!("Rules load error: {e}")),
            }
        }
    });
    load_inference.dispatch(());

    let api_add_inf = api.clone();
    let reload_inf = load_inference.clone();
    let add_inference = Action::new_local(move |_: &()| {
        let api = api_add_inf.clone();
        let reload = reload_inf.clone();
        let text = new_rules_text.get_untracked();
        let rules: Vec<InferenceRule> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| InferenceRule {
                name: None,
                rule: l.trim().to_string(),
                description: None,
            })
            .collect();
        async move {
            let body = serde_json::json!({ "rules": rules, "append": true });
            match api.post_text("/rules", &body).await {
                Ok(r) => {
                    set_status_msg.set(format!("Rules loaded: {r}"));
                    set_new_rules_text.set(String::new());
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Rules add error: {e}")),
            }
        }
    });

    // ── Action rules ──
    let (action_rules, set_action_rules) = signal(Vec::<ActionRule>::new());
    let (new_rule_json, set_new_rule_json) = signal(String::new());
    let (dry_run_output, set_dry_run_output) = signal(String::new());
    let (dry_running, set_dry_running) = signal(false);

    let api_load_act = api.clone();
    let load_action_rules = Action::new_local(move |_: &()| {
        let api = api_load_act.clone();
        async move {
            match api.get::<Vec<ActionRule>>("/actions/rules").await {
                Ok(list) => set_action_rules.set(list),
                Err(e) => set_status_msg.set(format!("Action rules error: {e}")),
            }
        }
    });
    load_action_rules.dispatch(());

    let api_dry = api.clone();
    let dry_run = Action::new_local(move |_: &()| {
        let api = api_dry.clone();
        async move {
            set_dry_running.set(true);
            let body = serde_json::json!({});
            match api.post_text("/actions/dry-run", &body).await {
                Ok(r) => set_dry_run_output.set(r),
                Err(e) => set_dry_run_output.set(format!("Error: {e}")),
            }
            set_dry_running.set(false);
        }
    });

    let api_add_act = api.clone();
    let reload_act = load_action_rules.clone();
    let add_action_rule = Action::new_local(move |_: &()| {
        let api = api_add_act.clone();
        let reload = reload_act.clone();
        let json_str = new_rule_json.get_untracked();
        async move {
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(body) => {
                    match api.post_text("/actions/rules", &body).await {
                        Ok(r) => {
                            set_status_msg.set(format!("Rule added: {r}"));
                            set_new_rule_json.set(String::new());
                            reload.dispatch(());
                        }
                        Err(e) => set_status_msg.set(format!("Add rule error: {e}")),
                    }
                }
                Err(e) => set_status_msg.set(format!("Invalid JSON: {e}")),
            }
        }
    });

    let api_del = api.clone();
    let reload_act2 = load_action_rules.clone();
    let delete_rule = Action::new_local(move |id: &String| {
        let api = api_del.clone();
        let reload = reload_act2.clone();
        let path = format!("/actions/rules/{}", js_sys::encode_uri_component(id));
        async move {
            match api.delete(&path).await {
                Ok(_) => {
                    set_status_msg.set("Rule deleted".to_string());
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Delete error: {e}")),
            }
        }
    });

    let api_toggle = api.clone();
    let reload_act3 = load_action_rules.clone();
    let toggle_rule = Action::new_local(move |args: &(String, bool)| {
        let api = api_toggle.clone();
        let reload = reload_act3.clone();
        let (id, new_state) = args.clone();
        let path = format!("/actions/rules/{}", js_sys::encode_uri_component(&id));
        let body = serde_json::json!({ "enabled": new_state });
        async move {
            match api.patch::<_, serde_json::Value>(&path, &body).await {
                Ok(_) => { reload.dispatch(()); },
                Err(e) => set_status_msg.set(format!("Toggle error: {e}")),
            }
        }
    });

    view! {
        <div class=move || if open.get() { "modal-overlay active" } else { "modal-overlay" }
            on:click=move |_| set_modal_open.set(String::new())>
            <div class="wizard-modal" style="max-width: 700px;" on:click=|e| e.stop_propagation()>
                <div class="wizard-modal-header">
                    <h3><i class="fa-solid fa-gavel"></i>" Rules"</h3>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| set_modal_open.set(String::new())>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="wizard-modal-body">
                    // Tab bar
                    <div class="rules-tabs" style="display: flex; gap: 0; margin-bottom: 1rem; border-bottom: 1px solid var(--border);">
                        <button
                            class=move || if active_tab.get() == "inference" { "rules-tab rules-tab-active" } else { "rules-tab" }
                            on:click=move |_| set_active_tab.set("inference".into())>
                            <i class="fa-solid fa-brain"></i>" Inference"
                        </button>
                        <button
                            class=move || if active_tab.get() == "action" { "rules-tab rules-tab-active" } else { "rules-tab" }
                            on:click=move |_| set_active_tab.set("action".into())>
                            <i class="fa-solid fa-bolt"></i>" Action"
                        </button>
                    </div>

                    // ── Inference tab ──
                    <div style=move || if active_tab.get() == "inference" { "" } else { "display:none" }>
                        {move || {
                            let names = rule_names.get();
                            if names.is_empty() {
                                view! { <p style="opacity: 0.6;">"No inference rules loaded."</p> }.into_any()
                            } else {
                                view! {
                                    <div style="margin-bottom: 1rem;">
                                        {names.into_iter().map(|n| view! {
                                            <div style="padding: 0.25rem 0;">
                                                <i class="fa-solid fa-gavel" style="margin-right: 0.5rem; opacity: 0.6;"></i>{n}
                                            </div>
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }
                        }}
                        <div class="form-group" style="margin-bottom: 0.5rem;">
                            <label>"Add Rules (one per line, format: IF condition THEN action)"</label>
                            <textarea
                                class="code-area"
                                rows="4"
                                placeholder="IF entity has_type Person AND missing email THEN suggest find email for {entity}"
                                prop:value=new_rules_text
                                on:input=move |ev| set_new_rules_text.set(event_target_value(&ev))
                            />
                        </div>
                        <button class="btn btn-primary" on:click=move |_| { add_inference.dispatch(()); }>
                            <i class="fa-solid fa-upload"></i>" Load Rules"
                        </button>
                    </div>

                    // ── Action tab ──
                    <div style=move || if active_tab.get() == "action" { "" } else { "display:none" }>
                        <div class="button-group" style="margin-bottom: 1rem;">
                            <button
                                class="btn btn-warning btn-sm"
                                on:click=move |_| { dry_run.dispatch(()); }
                                disabled=move || dry_running.get()
                            >
                                <i class="fa-solid fa-flask"></i>
                                {move || if dry_running.get() { " Running..." } else { " Dry Run" }}
                            </button>
                        </div>

                        {move || {
                            let output = dry_run_output.get();
                            (!output.is_empty()).then(|| view! {
                                <pre style="background: #1a1a2e; padding: 0.75rem; border-radius: 4px; overflow-x: auto; margin-bottom: 1rem; font-size: 0.85rem;">{output}</pre>
                            })
                        }}

                        {move || {
                            let rules = action_rules.get();
                            if rules.is_empty() {
                                return view! {
                                    <p style="opacity: 0.6;">"No action rules configured."</p>
                                }.into_any();
                            }
                            view! {
                                <table class="data-table">
                                    <thead>
                                        <tr>
                                            <th>"Name"</th>
                                            <th>"Description"</th>
                                            <th>"On"</th>
                                            <th></th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rules.into_iter().map(|r| {
                                            let id_toggle = r.id.clone();
                                            let enabled = r.enabled;
                                            let id_del = r.id.clone();
                                            view! {
                                                <tr>
                                                    <td><strong>{r.id.clone()}</strong></td>
                                                    <td>{r.description.clone().unwrap_or_default()}</td>
                                                    <td>
                                                        <input type="checkbox" prop:checked=enabled
                                                            on:change=move |_| {
                                                                toggle_rule.dispatch((id_toggle.clone(), !enabled));
                                                            } />
                                                    </td>
                                                    <td>
                                                        <button class="btn btn-sm btn-danger"
                                                            on:click=move |_| { delete_rule.dispatch(id_del.clone()); }>
                                                            <i class="fa-solid fa-trash"></i>
                                                        </button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }}

                        <div style="margin-top: 1rem;">
                            <h4>"Add Action Rule (JSON)"</h4>
                            <textarea
                                class="code-area"
                                rows="6"
                                placeholder="{\"name\": \"my-rule\", \"trigger\": \"node_created\", \"conditions\": {}, \"actions\": {}}"
                                prop:value=new_rule_json
                                on:input=move |ev| set_new_rule_json.set(event_target_value(&ev))
                            />
                            <button class="btn btn-primary" style="margin-top: 0.5rem;"
                                on:click=move |_| { add_action_rule.dispatch(()); }>
                                <i class="fa-solid fa-plus"></i>" Add Rule"
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
