use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::HealthResponse;

pub(crate) fn render_connection_modal(
    api_url: ReadSignal<String>,
    set_api_url: WriteSignal<String>,
    health_status: ReadSignal<String>,
    set_health_status: WriteSignal<String>,
    set_status_msg: WriteSignal<String>,
    set_modal_open: WriteSignal<String>,
) -> impl IntoView {
    let test_connection = Action::new_local(move |_: &()| {
        let url = api_url.get_untracked();
        let client = ApiClient::new(&url);
        async move {
            match client.get::<HealthResponse>("/health").await {
                Ok(h) => set_health_status.set(format!("Connected - {} ({} nodes, {} edges)", h.status, h.nodes, h.edges)),
                Err(e) => set_health_status.set(format!("Failed: {e}")),
            }
        }
    });

    let save_url = move |_| {
        let url = api_url.get_untracked();
        ApiClient::save_base_url(&url);
        set_status_msg.set("API URL saved. Reload the page to apply.".to_string());
        set_modal_open.set(String::new());
    };

    view! {
        <div class="form-row">
            <label>"API URL"</label>
            <input
                type="text"
                placeholder="http://localhost:3030"
                prop:value=api_url
                on:input=move |ev| set_api_url.set(event_target_value(&ev))
            />
        </div>
        <div class="button-group">
            <button class="btn btn-primary" on:click=move |_| { test_connection.dispatch(()); }>
                <i class="fa-solid fa-satellite-dish"></i>" Test"
            </button>
            <button class="btn btn-success" on:click=save_url>
                <i class="fa-solid fa-floppy-disk"></i>" Save"
            </button>
        </div>
        {move || {
            let st = health_status.get();
            (!st.is_empty()).then(|| view! {
                <div class="info-box" style="margin-top: 0.5rem;">
                    <i class="fa-solid fa-circle-info"></i>" "{st}
                </div>
            })
        }}
    }
}
