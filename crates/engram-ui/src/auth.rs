use leptos::prelude::*;
use serde::{Deserialize, Serialize};

const AUTH_STORAGE_KEY: &str = "engram_auth";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthInfo {
    pub username: String,
    pub role: String,
    pub trust_level: f32,
    pub token: String,
}

pub type AuthState = RwSignal<Option<AuthInfo>>;

/// Provide auth state at the App root. Reads from sessionStorage on init.
pub fn provide_auth() -> AuthState {
    let stored = load_from_storage();
    let auth = RwSignal::new(stored);
    provide_context(auth);
    auth
}

/// Retrieve auth state from context.
pub fn use_auth() -> AuthState {
    use_context::<AuthState>().expect("AuthState context not provided")
}

/// Get the current auth token, if any.
pub fn auth_token() -> Option<String> {
    load_from_storage().map(|a| a.token)
}

/// Save auth info to sessionStorage.
pub fn login(info: AuthInfo) {
    let auth = use_auth();
    save_to_storage(&info);
    auth.set(Some(info));
}

/// Clear auth from sessionStorage and signal.
pub fn logout() {
    let auth = use_auth();
    clear_storage();
    auth.set(None);
}

fn load_from_storage() -> Option<AuthInfo> {
    web_sys::window()
        .and_then(|w| w.session_storage().ok().flatten())
        .and_then(|s| s.get_item(AUTH_STORAGE_KEY).ok().flatten())
        .and_then(|json| serde_json::from_str(&json).ok())
}

pub fn save_to_storage_pub(info: &AuthInfo) {
    save_to_storage(info);
}

fn save_to_storage(info: &AuthInfo) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.session_storage().ok().flatten())
    {
        if let Ok(json) = serde_json::to_string(info) {
            let _ = storage.set_item(AUTH_STORAGE_KEY, &json);
        }
    }
}

pub fn clear_storage_pub() {
    clear_storage();
}

fn clear_storage() {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.session_storage().ok().flatten())
    {
        let _ = storage.remove_item(AUTH_STORAGE_KEY);
    }
}
