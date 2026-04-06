use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::ActionRule;

#[component]
pub fn ActionsPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (rules, set_rules) = signal(Vec::<ActionRule>::new());
    let (rule_toml, set_rule_toml) = signal(String::new());
    let (status_msg, set_status_msg) = signal(String::new());
    let (dry_run_result, set_dry_run_result) = signal(String::new());

    let api1 = api.clone();
    let load_rules = Action::new_local(move |_: &()| {
        let api = api1.clone();
        async move {
            match api.get::<Vec<ActionRule>>("/actions/rules").await {
                Ok(r) => set_rules.set(r),
                Err(e) => set_status_msg.set(format!("Error loading rules: {e}")),
            }
        }
    });

    load_rules.dispatch(());

    let api2 = api.clone();
    let submit_rules = Action::new_local(move |_: &()| {
        let api = api2.clone();
        let toml = rule_toml.get_untracked();
        async move {
            let body = serde_json::json!({"toml": toml});
            match api.post_text("/actions/rules", &body).await {
                Ok(r) => {
                    set_status_msg.set(format!("Rules loaded: {r}"));
                    load_rules.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Error: {e}")),
            }
        }
    });

    let api3 = api.clone();
    let do_dry_run = Action::new_local(move |_: &()| {
        let api = api3.clone();
        async move {
            let body = serde_json::json!({});
            match api.post_text("/actions/dry-run", &body).await {
                Ok(r) => set_dry_run_result.set(r),
                Err(e) => set_dry_run_result.set(format!("Error: {e}")),
            }
        }
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-bolt"></i>" Actions"</h2>
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        <div class="actions-grid">
            <div class="card">
                <h3>"Active Rules"</h3>
                <div class="table-wrapper">
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"ID"</th>
                                <th>"Name"</th>
                                <th>"Enabled"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each={move || rules.get()}
                                key={|r| r.id.clone()}
                                children={move |rule| {
                                    let enabled_badge = if rule.enabled {
                                        view! { <span class="badge badge-success">"Enabled"</span> }.into_any()
                                    } else {
                                        view! { <span class="badge badge-muted">"Disabled"</span> }.into_any()
                                    };
                                    view! {
                                        <tr>
                                            <td><code>{rule.id.clone()}</code></td>
                                            <td>{rule.description.clone().unwrap_or_default()}</td>
                                            <td>{enabled_badge}</td>
                                        </tr>
                                    }
                                }}
                            />
                        </tbody>
                    </table>
                </div>
            </div>

            <div class="card">
                <h3>"Load Rules (TOML)"</h3>
                <textarea
                    class="code-area"
                    placeholder="Paste TOML rule definitions..."
                    prop:value=rule_toml
                    on:input=move |ev| set_rule_toml.set(event_target_value(&ev))
                />
                <div class="button-group">
                    <button class="btn btn-primary" on:click=move |_| { submit_rules.dispatch(()); }>
                        <i class="fa-solid fa-upload"></i>" Load Rules"
                    </button>
                    <button class="btn btn-secondary" on:click=move |_| { do_dry_run.dispatch(()); }>
                        <i class="fa-solid fa-flask"></i>" Dry Run"
                    </button>
                </div>
            </div>
        </div>

        {move || {
            let dr = dry_run_result.get();
            (!dr.is_empty()).then(|| view! {
                <div class="card">
                    <h3>"Dry Run Result"</h3>
                    <pre class="code-area">{dr}</pre>
                </div>
            })
        }}
    }
}
