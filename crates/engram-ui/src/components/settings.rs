use leptos::prelude::*;

use crate::api::ApiClient;

#[component]
pub fn SettingsModal(
    #[prop(into)] open: ReadSignal<bool>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let (url_input, set_url_input) = signal(ApiClient::load_base_url());

    let overlay_class = move || {
        if open.get() {
            "modal-overlay active"
        } else {
            "modal-overlay"
        }
    };

    let close = move |_| {
        on_close.run(());
    };

    let save = move |_| {
        let url = url_input.get();
        ApiClient::save_base_url(&url);
        // Reload page to apply new base URL
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    };

    view! {
        <div class=overlay_class on:click=close>
            <div class="modal" on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h3><i class="fa-solid fa-gear"></i>" API Settings"</h3>
                    <button class="btn-icon modal-close" on:click=close>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="modal-body">
                    <label for="api-url-input">"API Base URL"</label>
                    <input
                        type="text"
                        id="api-url-input"
                        placeholder="http://localhost:3030"
                        prop:value=url_input
                        on:input=move |ev| {
                            set_url_input.set(event_target_value(&ev));
                        }
                    />
                    <p class="help-text">"The engram HTTP API endpoint. Default: http://localhost:3030"</p>
                </div>
                <div class="modal-footer">
                    <button class="btn btn-secondary" on:click=close>"Cancel"</button>
                    <button class="btn btn-primary" on:click=save>"Save"</button>
                </div>
            </div>
        </div>
    }
}
