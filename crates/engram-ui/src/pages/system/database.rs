use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{ConfigStatusResponse, ResetResponse};

pub(crate) fn render_database_modal(
    api: ApiClient,
    import_text: ReadSignal<String>,
    set_import_text: WriteSignal<String>,
    set_status_msg: WriteSignal<String>,
) -> impl IntoView {
    let api_export = api.clone();
    let do_export = Action::new_local(move |_: &()| {
        let api = api_export.clone();
        async move {
            match api.get_text("/export/jsonld").await {
                Ok(text) => set_import_text.set(text),
                Err(e) => set_status_msg.set(format!("Export error: {e}")),
            }
        }
    });

    let api_import = api.clone();
    let do_import = Action::new_local(move |_: &()| {
        let api = api_import.clone();
        let text = import_text.get_untracked();
        async move {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(body) => {
                    match api.post_text("/import/jsonld", &body).await {
                        Ok(r) => set_status_msg.set(format!("Import complete: {r}")),
                        Err(e) => set_status_msg.set(format!("Import error: {e}")),
                    }
                }
                Err(e) => set_status_msg.set(format!("Invalid JSON: {e}")),
            }
        }
    });

    let api_for_db = api.clone();

    view! {
        // ── Import / Export ──
        <h4><i class="fa-solid fa-download" style="margin-right: 0.25rem;"></i>" Export"</h4>
        <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
            "Export your knowledge base as JSON-LD for backup or sharing."
        </p>
        <button class="btn btn-primary" on:click=move |_| { do_export.dispatch(()); }>
            <i class="fa-solid fa-file-export"></i>" Export as JSON-LD"
        </button>

        <h4 style="margin-top: 1.25rem;"><i class="fa-solid fa-upload" style="margin-right: 0.25rem;"></i>" Import"</h4>
        <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
            "Import knowledge from a JSON-LD file."
        </p>
        <div class="form-row">
            <input type="file" accept=".json,.jsonld" id="import-file" />
        </div>
        <div class="form-row">
            <textarea
                class="code-area"
                rows="6"
                placeholder="Or paste JSON-LD data here..."
                prop:value=import_text
                on:input=move |ev| set_import_text.set(event_target_value(&ev))
            />
        </div>
        <button class="btn btn-success" on:click=move |_| { do_import.dispatch(()); }>
            <i class="fa-solid fa-file-import"></i>" Import"
        </button>

        // ── Database Management (inlined) ──
        <DatabaseManagementInline api=api_for_db set_status_msg=set_status_msg />
    }
}

// ── Database Management section ──

#[allow(unused_variables)]
#[component]
fn DatabaseManagementInline(
    api: ApiClient,
    set_status_msg: WriteSignal<String>,
) -> impl IntoView {
    let (config_status, set_config_status) = signal(Option::<ConfigStatusResponse>::None);
    let (reset_confirm, set_reset_confirm) = signal(String::new());
    let (reset_result, set_reset_result) = signal(Option::<String>::None);

    // Load config status
    let api_status = api.clone();
    Effect::new(move |_| {
        let api = api_status.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(status) = api.get::<ConfigStatusResponse>("/config/status").await {
                set_config_status.set(Some(status));
            }
        });
    });

    // Reset action
    let api_reset = api.clone();
    let do_reset = Action::new_local(move |_: &()| {
        let api = api_reset.clone();
        async move {
            if reset_confirm.get_untracked() != "yes" {
                set_reset_result.set(Some("Type 'yes' to confirm reset.".into()));
                return;
            }
            set_reset_result.set(Some("Resetting...".into()));
            match api.post::<_, ResetResponse>("/admin/reset", &serde_json::json!({})).await {
                Ok(r) => {
                    if r.success {
                        let cleaned = if r.sidecars_cleaned.is_empty() {
                            "none".into()
                        } else {
                            r.sidecars_cleaned.join(", ")
                        };
                        set_reset_result.set(Some(format!("Reset complete. Sidecars cleaned: {cleaned}")));
                        set_reset_confirm.set(String::new());
                        // Refresh status
                        if let Ok(status) = api.get::<ConfigStatusResponse>("/config/status").await {
                            set_config_status.set(Some(status));
                        }
                    } else {
                        set_reset_result.set(Some("Reset failed.".into()));
                    }
                }
                Err(e) => set_reset_result.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div>
            <h4 style="margin-top: 1.5rem;"><i class="fa-solid fa-database"></i>" Database Management"</h4>
            // Stats
            {move || config_status.get().map(|status| view! {
                <div class="stat-grid" style="margin-bottom: 1rem;">
                    <div class="stat-card">
                        <div class="stat-value">{status.node_count.to_string()}</div>
                        <div class="stat-label">"Nodes"</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">{status.edge_count.to_string()}</div>
                        <div class="stat-label">"Edges"</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">{if status.ready { "Ready" } else { "Not Ready" }}</div>
                        <div class="stat-label">"Status"</div>
                    </div>
                </div>

                {if !status.warnings.is_empty() {
                    Some(view! {
                        <div style="color: var(--warning); font-size: 0.85rem; margin-bottom: 0.75rem;">
                            {status.warnings.iter().map(|w| view! {
                                <p><i class="fa-solid fa-triangle-exclamation"></i>" " {w.clone()}</p>
                            }).collect::<Vec<_>>()}
                        </div>
                    })
                } else { None }}

                <div class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                    <strong>"Configured: "</strong>{status.configured.join(", ")}
                </div>
                {if !status.missing.is_empty() {
                    Some(view! {
                        <div class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                            <strong>"Missing: "</strong>{status.missing.join(", ")}
                        </div>
                    })
                } else { None }}
            })}

            // Reset section
            <h4 style="margin-top: 1rem; color: var(--danger, #e74c3c);">
                <i class="fa-solid fa-triangle-exclamation"></i>" Reset Database"
            </h4>
            <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                "This will delete all nodes, edges, and learned data. Configuration, users, and secrets are preserved."
            </p>

            {move || reset_result.get().map(|msg| view! {
                <div class="card" style="padding: 0.5rem; margin-bottom: 0.75rem;">
                    <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                    " " {msg}
                </div>
            })}

            <div class="form-row">
                <label>"Type 'yes' to confirm"</label>
                <input type="text" placeholder="yes"
                    prop:value=reset_confirm
                    on:input=move |ev| set_reset_confirm.set(event_target_value(&ev))
                />
            </div>
            <div class="button-group">
                <button class="btn btn-danger"
                    on:click=move |_| { do_reset.dispatch(()); }
                    disabled=move || reset_confirm.get() != "yes">
                    <i class="fa-solid fa-trash"></i>" Reset Database"
                </button>
            </div>

            // ── Rerun Onboarding Wizard ──
            <h4 style="margin-top: 1.5rem;"><i class="fa-solid fa-hat-wizard"></i>" Onboarding Wizard"</h4>
            <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                "Rerun the setup wizard to reconfigure settings or seed a new topic into your knowledge graph."
            </p>
            <div class="button-group">
                <button class="btn btn-primary" on:click={
                    move |_| {
                        // Open wizard directly via shared context signal
                        if let Some(set_open) = use_context::<WriteSignal<bool>>() {
                            set_open.set(true);
                        }
                    }
                }>
                    <i class="fa-solid fa-hat-wizard"></i>" Run Setup Wizard"
                </button>
            </div>
        </div>
    }
}
