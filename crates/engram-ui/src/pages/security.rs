use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    ApiKeyInfo, ChangePasswordRequest, CreateApiKeyRequest, CreateUserRequest, UserInfo,
};
use crate::auth;
use crate::components::collapsible_section::CollapsibleSection;

#[component]
pub fn SecurityPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let auth_state = auth::use_auth();

    let (result_msg, set_result_msg) = signal(String::new());
    let (msg_is_error, set_msg_is_error) = signal(false);

    // ── My Account: change password state ──
    let (cur_pw, set_cur_pw) = signal(String::new());
    let (new_pw, set_new_pw) = signal(String::new());

    let api_pw = api.clone();
    let change_password = Action::new_local(move |_: &()| {
        let api = api_pw.clone();
        let current = cur_pw.get_untracked();
        let new = new_pw.get_untracked();
        async move {
            if new.len() < 8 {
                set_msg_is_error.set(true);
                set_result_msg.set("Password must be at least 8 characters.".to_string());
                return;
            }
            let body = ChangePasswordRequest {
                current_password: current,
                new_password: new,
            };
            match api.post_text("/auth/change-password", &body).await {
                Ok(_) => {
                    set_msg_is_error.set(false);
                    set_result_msg.set("Password changed successfully.".to_string());
                    set_cur_pw.set(String::new());
                    set_new_pw.set(String::new());
                }
                Err(e) => {
                    set_msg_is_error.set(true);
                    set_result_msg.set(format!("Error: {e}"));
                }
            }
        }
    });

    // ── API Keys state ──
    let (api_keys, set_api_keys) = signal(Vec::<ApiKeyInfo>::new());
    let (new_key_label, set_new_key_label) = signal(String::new());
    let (generated_key, set_generated_key) = signal(Option::<String>::None);

    let api_load_keys = api.clone();
    let load_keys = Action::new_local(move |_: &()| {
        let api = api_load_keys.clone();
        async move {
            match api.get::<Vec<ApiKeyInfo>>("/auth/api-keys").await {
                Ok(keys) => set_api_keys.set(keys),
                Err(e) => {
                    set_msg_is_error.set(true);
                    set_result_msg.set(format!("Failed to load API keys: {e}"));
                }
            }
        }
    });
    load_keys.dispatch(());

    let api_gen = api.clone();
    let load_keys_after_gen = load_keys.clone();
    let generate_key = Action::new_local(move |_: &()| {
        let api = api_gen.clone();
        let label = new_key_label.get_untracked();
        let reload = load_keys_after_gen.clone();
        async move {
            if label.trim().is_empty() {
                set_msg_is_error.set(true);
                set_result_msg.set("API key label is required.".to_string());
                return;
            }
            let body = CreateApiKeyRequest { label };
            match api.post::<_, ApiKeyInfo>("/auth/api-keys", &body).await {
                Ok(info) => {
                    set_generated_key.set(info.key.clone());
                    set_new_key_label.set(String::new());
                    set_msg_is_error.set(false);
                    set_result_msg.set("API key generated. Copy it now -- it will not be shown again.".to_string());
                    reload.dispatch(());
                }
                Err(e) => {
                    set_msg_is_error.set(true);
                    set_result_msg.set(format!("Error: {e}"));
                }
            }
        }
    });

    let api_revoke = api.clone();
    let load_keys_after_revoke = load_keys.clone();
    let revoke_key = Action::new_local(move |id: &String| {
        let api = api_revoke.clone();
        let id = id.clone();
        let reload = load_keys_after_revoke.clone();
        async move {
            match api.delete(&format!("/auth/api-keys/{id}")).await {
                Ok(_) => {
                    set_msg_is_error.set(false);
                    set_result_msg.set("API key revoked.".to_string());
                    reload.dispatch(());
                }
                Err(e) => {
                    set_msg_is_error.set(true);
                    set_result_msg.set(format!("Error: {e}"));
                }
            }
        }
    });

    // ── User Management state ──
    let (users, set_users) = signal(Vec::<UserInfo>::new());
    let (new_username, set_new_username) = signal(String::new());
    let (new_user_pw, set_new_user_pw) = signal(String::new());
    let (new_user_role, set_new_user_role) = signal("user".to_string());
    let (new_user_trust, set_new_user_trust) = signal(0.5_f32);

    let api_load_users = api.clone();
    let load_users = Action::new_local(move |_: &()| {
        let api = api_load_users.clone();
        async move {
            match api.get::<serde_json::Value>("/auth/users").await {
                Ok(val) => {
                    // API returns {"users": [...]} wrapper
                    let users_val = val.get("users").cloned().unwrap_or(serde_json::Value::Array(vec![]));
                    let u: Vec<UserInfo> = serde_json::from_value(users_val).unwrap_or_default();
                    set_users.set(u);
                }
                Err(e) => {
                    set_msg_is_error.set(true);
                    set_result_msg.set(format!("Failed to load users: {e}"));
                }
            }
        }
    });
    load_users.dispatch(());

    let api_create_user = api.clone();
    let load_users_after_create = load_users.clone();
    let create_user = Action::new_local(move |_: &()| {
        let api = api_create_user.clone();
        let username = new_username.get_untracked();
        let password = new_user_pw.get_untracked();
        let role = new_user_role.get_untracked();
        let trust = new_user_trust.get_untracked();
        let reload = load_users_after_create.clone();
        async move {
            if username.trim().is_empty() || password.len() < 8 {
                set_msg_is_error.set(true);
                set_result_msg.set("Username required and password must be >= 8 chars.".to_string());
                return;
            }
            let body = CreateUserRequest {
                username,
                password,
                role: Some(role),
                trust_level: Some(trust),
            };
            match api.post_text("/auth/users", &body).await {
                Ok(_) => {
                    set_msg_is_error.set(false);
                    set_result_msg.set("User created.".to_string());
                    set_new_username.set(String::new());
                    set_new_user_pw.set(String::new());
                    reload.dispatch(());
                }
                Err(e) => {
                    set_msg_is_error.set(true);
                    set_result_msg.set(format!("Error: {e}"));
                }
            }
        }
    });

    let api_del_user = api.clone();
    let load_users_after_del = load_users.clone();
    let _delete_user = Action::new_local(move |username: &String| {
        let api = api_del_user.clone();
        let username = username.clone();
        let reload = load_users_after_del.clone();
        async move {
            match api.delete(&format!("/auth/users/{username}")).await {
                Ok(_) => {
                    set_msg_is_error.set(false);
                    set_result_msg.set(format!("User '{username}' deleted."));
                    reload.dispatch(());
                }
                Err(e) => {
                    set_msg_is_error.set(true);
                    set_result_msg.set(format!("Error: {e}"));
                }
            }
        }
    });

    // ── Copy to clipboard helper ──
    let copy_to_clipboard = move |text: String| {
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&text),
                )
                .await;
            }
        });
    };

    let base_url = api.base_url.clone();

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-shield-halved"></i>" Security"</h2>
            <p class="text-secondary">"Users, API keys, access control"</p>
        </div>

        {move || {
            let msg = result_msg.get();
            (!msg.is_empty()).then(|| {
                let cls = if msg_is_error.get() { "alert alert-error" } else { "alert alert-success" };
                view! { <div class=cls>{msg}</div> }
            })
        }}

        // ── Section 1: My Account ──
        <CollapsibleSection title="My Account" icon="fa-solid fa-circle-user">
            {move || {
                auth_state.get().map(|info| view! {
                    <div class="account-info" style="display: flex; align-items: center; gap: 1.5rem; margin-bottom: 1.5rem; padding: 1rem; background: var(--bg-secondary, #1a2332); border-radius: 8px;">
                        <i class="fa-solid fa-user-circle" style="font-size: 3rem; color: var(--color-primary, #4a9eff);"></i>
                        <div>
                            <div style="font-size: 1.2rem; font-weight: 600;">{info.username.clone()}</div>
                            <div style="display: flex; gap: 0.5rem; align-items: center; margin-top: 0.25rem;">
                                "Role: "
                                <span class="badge badge-active"><i class="fa-solid fa-user-shield"></i>" "{info.role.clone()}</span>
                            </div>
                            <div style="margin-top: 0.25rem; color: var(--text-secondary, #8899aa);">
                                {format!("Trust Level: {:.2}", info.trust_level)}
                            </div>
                        </div>
                    </div>
                })
            }}
            <h4><i class="fa-solid fa-lock"></i>" Change Password"</h4>
            <div style="display: flex; gap: 0.75rem; align-items: end; margin-top: 0.5rem;">
                <div style="flex: 1;">
                    <label class="form-label">"Current Password"</label>
                    <input
                        type="password"
                        prop:value=cur_pw
                        on:input=move |ev| set_cur_pw.set(event_target_value(&ev))
                    />
                </div>
                <div style="flex: 1;">
                    <label class="form-label">"New Password"</label>
                    <input
                        type="password"
                        prop:value=new_pw
                        on:input=move |ev| set_new_pw.set(event_target_value(&ev))
                    />
                </div>
                <button class="btn btn-primary" on:click=move |_| { change_password.dispatch(()); }>
                    <i class="fa-solid fa-key"></i>" Change"
                </button>
            </div>
        </CollapsibleSection>

        // ── Section 2: API Keys ──
        <CollapsibleSection title="API Keys" icon="fa-solid fa-key">
            <p class="text-secondary" style="margin-bottom: 1rem;">
                "API keys provide persistent access for integrations (HTTP API, MCP server, scripts). Keys inherit your role and trust level."
            </p>

            {move || {
                let keys = api_keys.get();
                if keys.is_empty() {
                    view! { <p class="text-secondary"><i class="fa-solid fa-circle-info"></i>" No API keys yet"</p> }.into_any()
                } else {
                    view! {
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"LABEL"</th>
                                    <th>"KEY ID"</th>
                                    <th>"CREATED"</th>
                                    <th>"ACTIONS"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {keys.into_iter().map(|k| {
                                    let id = k.id.clone();
                                    let id_short = if id.len() > 8 { id[..8].to_string() } else { id.clone() };
                                    let id_for_revoke = id.clone();
                                    view! {
                                        <tr>
                                            <td>{k.label.clone().unwrap_or_default()}</td>
                                            <td><code>{id_short}</code></td>
                                            <td>{k.created.clone().unwrap_or_else(|| "-".to_string())}</td>
                                            <td>
                                                <button
                                                    class="btn btn-sm btn-danger"
                                                    on:click=move |_| { revoke_key.dispatch(id_for_revoke.clone()); }
                                                >
                                                    <i class="fa-solid fa-trash"></i>" Revoke"
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

            {move || generated_key.get().map(|key| {
                let key_copy = key.clone();
                view! {
                    <div class="generated-key-box" style="background: var(--bg-highlight, #1a3a1a); border: 1px solid var(--color-success, #4caf50); padding: 0.75rem 1rem; border-radius: 4px; margin: 1rem 0; display: flex; align-items: center; gap: 0.5rem;">
                        <i class="fa-solid fa-circle-exclamation" style="color: var(--color-warning, #ff9800);"></i>
                        <code style="flex: 1; word-break: break-all;">{key}</code>
                        <button
                            class="btn btn-sm btn-secondary"
                            on:click=move |_| { copy_to_clipboard(key_copy.clone()); }
                        >
                            <i class="fa-solid fa-copy"></i>" Copy"
                        </button>
                    </div>
                }
            })}

            <h4><i class="fa-solid fa-plus"></i>" Generate New Key"</h4>
            <p class="text-secondary" style="margin-bottom: 0.5rem; font-size: 0.85rem;">"Label (e.g. \"MCP Server\", \"CI Pipeline\")"</p>
            <div class="form-row" style="display: flex; gap: 0.5rem; align-items: center;">
                <input
                    type="text"
                    placeholder="My integration"
                    prop:value=new_key_label
                    on:input=move |ev| set_new_key_label.set(event_target_value(&ev))
                    style="flex: 1;"
                />
                <button class="btn btn-primary" on:click=move |_| { generate_key.dispatch(()); }>
                    <i class="fa-solid fa-wand-magic-sparkles"></i>" Generate"
                </button>
            </div>
        </CollapsibleSection>

        // ── Section 3: User Management (admin only) ──
        {move || {
            let is_admin = auth_state.get()
                .map(|a| a.role == "admin")
                .unwrap_or(false);

            is_admin.then(|| view! {
                <CollapsibleSection title="User Management" icon="fa-solid fa-users-gear">
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"USER"</th>
                                <th>"ROLE"</th>
                                <th>"TRUST"</th>
                                <th>"STATUS"</th>
                                <th>"API KEYS"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || users.get().into_iter().map(|u| {
                                let uname = u.username.clone();
                                let _uname_del = uname.clone();
                                let role = u.role.clone().unwrap_or_else(|| "user".to_string());
                                let trust = u.trust_level.unwrap_or(0.5);
                                let active = u.active.unwrap_or(true);
                                view! {
                                    <tr>
                                        <td><i class="fa-solid fa-user"></i>" "{uname}</td>
                                        <td><span class="badge badge-active"><i class="fa-solid fa-user-shield"></i>" "{role}</span></td>
                                        <td>{format!("{:.2}", trust)}</td>
                                        <td>
                                            {if active {
                                                view! { <i class="fa-solid fa-circle-check" style="color: var(--color-success, #4caf50);"></i> }.into_any()
                                            } else {
                                                view! { <i class="fa-solid fa-circle-xmark" style="color: var(--color-error, #f44336);"></i> }.into_any()
                                            }}
                                        </td>
                                        <td>"0"</td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>

                    <div style="display: flex; gap: 0.5rem; align-items: end; margin-top: 1rem; padding-top: 0.75rem; border-top: 1px solid rgba(255,255,255,0.08);">
                        <div style="flex: 2;">
                            <label class="form-label">"Username"</label>
                            <input
                                type="text"
                                prop:value=new_username
                                on:input=move |ev| set_new_username.set(event_target_value(&ev))
                            />
                        </div>
                        <div style="flex: 2;">
                            <label class="form-label">"Password"</label>
                            <input
                                type="password"
                                prop:value=new_user_pw
                                on:input=move |ev| set_new_user_pw.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="form-label">"Role"</label>
                            <select
                                prop:value=new_user_role
                                on:change=move |ev| set_new_user_role.set(event_target_value(&ev))
                            >
                                <option value="admin">"Admin"</option>
                                <option value="user" selected=true>"User"</option>
                                <option value="viewer">"Viewer"</option>
                            </select>
                        </div>
                        <div>
                            <label class="form-label">"Trust"</label>
                            <input
                                type="number"
                                min="0" max="1" step="0.1"
                                style="width: 5rem;"
                                prop:value=move || format!("{:.2}", new_user_trust.get())
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<f32>() {
                                        set_new_user_trust.set(v);
                                    }
                                }
                            />
                        </div>
                        <button class="btn btn-primary" on:click=move |_| { create_user.dispatch(()); }>
                            <i class="fa-solid fa-user-plus"></i>" Add User"
                        </button>
                    </div>
                </CollapsibleSection>
            })
        }}

        // ── Section 4: Integration Guide ──
        <CollapsibleSection title="Integration Guide" icon="fa-solid fa-book" collapsed=true>
            <p>"Use these examples to integrate with the engram API from your tools and scripts."</p>

            <h4><i class="fa-solid fa-terminal"></i>" cURL"</h4>
            <pre><code>{
                let url = base_url.clone();
                format!(r#"# Query the graph with Bearer auth
curl -s -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{{"query":"Berlin","limit":5}}' \
  {url}/query"#)
            }</code></pre>

            <h4><i class="fa-solid fa-gear"></i>" MCP Configuration"</h4>
            <pre><code>{
                let url = base_url.clone();
                format!(r#"{{
  "mcpServers": {{
    "engram": {{
      "url": "{url}/mcp",
      "env": {{
        "ENGRAM_API_KEY": "YOUR_API_KEY"
      }}
    }}
  }}
}}"#)
            }</code></pre>

            <h4><i class="fa-brands fa-python"></i>" Python"</h4>
            <pre><code>{
                let url = base_url.clone();
                format!(r#"import requests

API = "{url}"
KEY = "YOUR_API_KEY"
headers = {{"Authorization": f"Bearer {{KEY}}"}}

# Store a fact
requests.post(f"{{API}}/store", headers=headers, json={{
    "entity": "Berlin",
    "content": "Capital of Germany",
    "confidence": 0.95
}})

# Query
resp = requests.post(f"{{API}}/query", headers=headers, json={{
    "query": "Berlin",
    "limit": 10
}})
print(resp.json())"#)
            }</code></pre>
        </CollapsibleSection>
    }
}
