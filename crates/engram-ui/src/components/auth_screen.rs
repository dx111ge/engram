use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{AuthStatusResponse, AuthLoginRequest, AuthLoginResponse, AuthSetupRequest};
use crate::auth::{self, AuthInfo};

#[component]
pub fn AuthScreen() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (mode, set_mode) = signal(AuthMode::Loading);
    let (error, set_error) = signal(Option::<String>::None);

    // Check auth status on mount
    let api_check = api.clone();
    let check_status = Action::new_local(move |_: &()| {
        let api = api_check.clone();
        async move {
            match api.get::<AuthStatusResponse>("/auth/status").await {
                Ok(status) => {
                    let is_setup = status.status.as_deref() == Some("setup_required");
                    if is_setup {
                        set_mode.set(AuthMode::Setup);
                    } else {
                        set_mode.set(AuthMode::Login);
                    }
                }
                Err(_) => {
                    // Server may not have auth enabled — try login mode
                    set_mode.set(AuthMode::Login);
                }
            }
        }
    });
    check_status.dispatch(());

    // Login form state
    let (login_user, set_login_user) = signal(String::new());
    let (login_pass, set_login_pass) = signal(String::new());
    let (submitting, set_submitting) = signal(false);

    // Setup form state
    let (setup_user, set_setup_user) = signal("admin".to_string());
    let (setup_pass, set_setup_pass) = signal(String::new());
    let (setup_confirm, set_setup_confirm) = signal(String::new());

    let api_login = api.clone();
    let do_login = Action::new_local(move |_: &()| {
        let api = api_login.clone();
        let username = login_user.get_untracked();
        let password = login_pass.get_untracked();
        async move {
            set_submitting.set(true);
            set_error.set(None);
            let body = AuthLoginRequest { username: username.clone(), password };
            match api.post::<_, AuthLoginResponse>("/auth/login", &body).await {
                Ok(resp) => {
                    let info = AuthInfo {
                        username,
                        role: resp.role.unwrap_or_else(|| "user".into()),
                        trust_level: resp.trust_level.unwrap_or(0.5),
                        token: resp.token,
                    };
                    auth::save_to_storage_pub(&info);
                    // Reload to apply auth state
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().reload();
                    }
                }
                Err(e) => set_error.set(Some(format!("Login failed: {e}"))),
            }
            set_submitting.set(false);
        }
    });

    let api_setup = api.clone();
    let do_setup = Action::new_local(move |_: &()| {
        let api = api_setup.clone();
        let username = setup_user.get_untracked();
        let password = setup_pass.get_untracked();
        let confirm = setup_confirm.get_untracked();
        async move {
            set_submitting.set(true);
            set_error.set(None);
            if password != confirm {
                set_error.set(Some("Passwords do not match".into()));
                set_submitting.set(false);
                return;
            }
            if password.len() < 8 {
                set_error.set(Some("Password must be at least 8 characters".into()));
                set_submitting.set(false);
                return;
            }
            let body = AuthSetupRequest { username: username.clone(), password: password.clone() };
            match api.post_text("/auth/setup", &body).await {
                Ok(_) => {
                    // Auto-login after setup
                    let login_body = AuthLoginRequest { username, password };
                    match api.post::<_, AuthLoginResponse>("/auth/login", &login_body).await {
                        Ok(resp) => {
                            let info = AuthInfo {
                                username: login_body.username,
                                role: resp.role.unwrap_or_else(|| "admin".into()),
                                trust_level: resp.trust_level.unwrap_or(1.0),
                                token: resp.token,
                            };
                            auth::save_to_storage_pub(&info);
                            if let Some(w) = web_sys::window() {
                                let _ = w.location().reload();
                            }
                        }
                        Err(e) => set_error.set(Some(format!("Setup succeeded but auto-login failed: {e}"))),
                    }
                }
                Err(e) => set_error.set(Some(format!("Setup failed: {e}"))),
            }
            set_submitting.set(false);
        }
    });

    view! {
        <div class="auth-container">
            <div class="auth-card">
                <div class="auth-header">
                    <i class="fa-solid fa-brain"></i>
                    <h1>"engram"</h1>
                    <p class="text-secondary">
                        {move || match mode.get() {
                            AuthMode::Setup => "Create your admin account to get started",
                            AuthMode::Login => "Sign in to your knowledge base",
                            AuthMode::Loading => "Connecting...",
                        }}
                    </p>
                </div>

                {move || error.get().map(|e| view! {
                    <div class="auth-error" style="display:block; margin-bottom:1rem;">
                        <i class="fa-solid fa-circle-exclamation"></i>
                        " "
                        {e}
                    </div>
                })}

                {move || match mode.get() {
                    AuthMode::Loading => view! {
                        <div class="loading-center">
                            <div class="spinner"></div>
                            <span>"Connecting..."</span>
                        </div>
                    }.into_any(),
                    AuthMode::Login => view! {
                        <div class="auth-form">
                            <div class="form-group">
                                <label><i class="fa-solid fa-user"></i>" Username"</label>
                                <input
                                    type="text"
                                    autocomplete="username"
                                    prop:value=login_user
                                    on:input=move |ev| set_login_user.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" { do_login.dispatch(()); }
                                    }
                                />
                            </div>
                            <div class="form-group">
                                <label><i class="fa-solid fa-lock"></i>" Password"</label>
                                <input
                                    type="password"
                                    autocomplete="current-password"
                                    prop:value=login_pass
                                    on:input=move |ev| set_login_pass.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" { do_login.dispatch(()); }
                                    }
                                />
                            </div>
                            <button
                                class="btn btn-primary w-100"
                                on:click=move |_| { do_login.dispatch(()); }
                                disabled=submitting
                            >
                                <i class="fa-solid fa-right-to-bracket"></i>
                                {move || if submitting.get() { " Signing in..." } else { " Sign In" }}
                            </button>
                        </div>
                    }.into_any(),
                    AuthMode::Setup => view! {
                        <div class="auth-form">
                            <div class="form-group">
                                <label><i class="fa-solid fa-user-shield"></i>" Admin Username"</label>
                                <input
                                    type="text"
                                    autocomplete="username"
                                    prop:value=setup_user
                                    on:input=move |ev| set_setup_user.set(event_target_value(&ev))
                                />
                            </div>
                            <div class="form-group">
                                <label><i class="fa-solid fa-lock"></i>" Password (min 8 chars)"</label>
                                <input
                                    type="password"
                                    autocomplete="new-password"
                                    prop:value=setup_pass
                                    on:input=move |ev| set_setup_pass.set(event_target_value(&ev))
                                />
                            </div>
                            <div class="form-group">
                                <label><i class="fa-solid fa-lock"></i>" Confirm Password"</label>
                                <input
                                    type="password"
                                    autocomplete="new-password"
                                    prop:value=setup_confirm
                                    on:input=move |ev| set_setup_confirm.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" { do_setup.dispatch(()); }
                                    }
                                />
                            </div>
                            <button
                                class="btn btn-success w-100"
                                on:click=move |_| { do_setup.dispatch(()); }
                                disabled=submitting
                            >
                                <i class="fa-solid fa-shield-halved"></i>
                                {move || if submitting.get() { " Setting up..." } else { " Create Admin Account" }}
                            </button>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AuthMode {
    Loading,
    Login,
    Setup,
}
