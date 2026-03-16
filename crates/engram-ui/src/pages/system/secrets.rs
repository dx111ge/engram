use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::SecretListItem;

pub(crate) fn render_secrets_modal(
    api: ApiClient,
    secrets: LocalResource<Vec<SecretListItem>>,
    set_status_msg: WriteSignal<String>,
) -> impl IntoView {
    let (secret_key, set_secret_key) = signal(String::new());
    let (secret_value, set_secret_value) = signal(String::new());

    let api_add_secret = api.clone();
    let add_secret = Action::new_local(move |_: &()| {
        let api = api_add_secret.clone();
        let key = secret_key.get_untracked();
        let value = secret_value.get_untracked();
        async move {
            let path = format!("/secrets/{key}");
            let body = serde_json::json!({ "value": value });
            match api.post_text(&path, &body).await {
                Ok(_) => {
                    set_status_msg.set(format!("Secret '{key}' saved."));
                    set_secret_key.set(String::new());
                    set_secret_value.set(String::new());
                }
                Err(e) => set_status_msg.set(format!("Secret save error: {e}")),
            }
        }
    });

    // Delete secret needs spawn_local since it takes a parameter
    let api_del_secret = api.clone();
    let (del_secret_trigger, set_del_secret_trigger) = signal(Option::<String>::None);

    Effect::new(move |_| {
        if let Some(key) = del_secret_trigger.get() {
            let api = api_del_secret.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/secrets/{key}");
                match api.delete(&path).await {
                    Ok(_) => set_status_msg.set(format!("Secret '{key}' deleted.")),
                    Err(e) => set_status_msg.set(format!("Delete error: {e}")),
                }
            });
            set_del_secret_trigger.set(None);
        }
    });

    view! {
        <h4>"Stored Secrets"</h4>
        {move || {
            let list = secrets.get().unwrap_or_default();
            if list.is_empty() {
                view! {
                    <p class="text-muted">"No secrets stored."</p>
                }.into_any()
            } else {
                view! {
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Key"</th>
                                <th>"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {list.into_iter().map(|s| {
                                let k = s.key.clone();
                                let k2 = s.key.clone();
                                view! {
                                    <tr>
                                        <td><code>{k}</code></td>
                                        <td>
                                            <button
                                                class="btn btn-danger btn-sm"
                                                on:click=move |_| {
                                                    set_del_secret_trigger.set(Some(k2.clone()));
                                                }
                                            >
                                                <i class="fa-solid fa-trash"></i>
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                }.into_any()
            }
        }}
        <h4 style="margin-top: 1rem;">"Add Secret"</h4>
        <div class="form-row">
            <label>"Key"</label>
            <input
                type="text"
                placeholder="SECRET_NAME"
                prop:value=secret_key
                on:input=move |ev| set_secret_key.set(event_target_value(&ev))
            />
        </div>
        <div class="form-row">
            <label>"Value"</label>
            <input
                type="password"
                placeholder="secret value..."
                prop:value=secret_value
                on:input=move |ev| set_secret_value.set(event_target_value(&ev))
            />
        </div>
        <div class="button-group">
            <button class="btn btn-success" on:click=move |_| { add_secret.dispatch(()); }>
                <i class="fa-solid fa-plus"></i>" Save Secret"
            </button>
        </div>
    }
}
