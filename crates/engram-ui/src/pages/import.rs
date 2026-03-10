use leptos::prelude::*;

use crate::api::ApiClient;

#[component]
pub fn ImportPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (export_data, set_export_data) = signal(String::new());
    let (import_text, set_import_text) = signal(String::new());
    let (status_msg, set_status_msg) = signal(String::new());

    let api1 = api.clone();
    let do_export = Action::new_local(move |_: &()| {
        let api = api1.clone();
        async move {
            match api.get_text("/export/jsonld").await {
                Ok(data) => set_export_data.set(data),
                Err(e) => set_status_msg.set(format!("Export failed: {e}")),
            }
        }
    });

    let api2 = api.clone();
    let do_import = Action::new_local(move |_: &()| {
        let api = api2.clone();
        let text = import_text.get_untracked();
        async move {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(json) => {
                    match api.post_text("/import/jsonld", &json).await {
                        Ok(resp) => set_status_msg.set(format!("Imported: {resp}")),
                        Err(e) => set_status_msg.set(format!("Import failed: {e}")),
                    }
                }
                Err(e) => set_status_msg.set(format!("Invalid JSON: {e}")),
            }
        }
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-file-import"></i>" Import / Export"</h2>
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        <div class="import-export-grid">
            <div class="card">
                <h3><i class="fa-solid fa-download"></i>" Export JSON-LD"</h3>
                <button class="btn btn-primary" on:click=move |_| { do_export.dispatch(()); }>
                    "Export Graph"
                </button>
                {move || {
                    let data = export_data.get();
                    (!data.is_empty()).then(|| view! {
                        <textarea class="code-area" readonly>{data}</textarea>
                    })
                }}
            </div>

            <div class="card">
                <h3><i class="fa-solid fa-upload"></i>" Import JSON-LD"</h3>
                <textarea
                    class="code-area"
                    placeholder="Paste JSON-LD here..."
                    prop:value=import_text
                    on:input=move |ev| set_import_text.set(event_target_value(&ev))
                />
                <button class="btn btn-primary" on:click=move |_| { do_import.dispatch(()); }>
                    "Import"
                </button>
            </div>
        </div>
    }
}
