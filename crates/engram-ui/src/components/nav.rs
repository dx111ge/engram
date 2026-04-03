use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::ApiClient;
use crate::api::types::HealthResponse;
use crate::auth;

#[component]
pub fn Nav() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let auth_state = auth::use_auth();

    let (mobile_open, set_mobile_open) = signal(false);
    let (online, set_online) = signal(false);

    let toggle_nav = move |_| set_mobile_open.update(|v| *v = !*v);
    let close_nav = move |_| set_mobile_open.set(false);

    let nav_class = move || {
        if mobile_open.get() { "nav-links open" } else { "nav-links" }
    };

    // Health polling (30s interval)
    let api_health = api.clone();
    let poll = move || {
        let api = api_health.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api.get::<HealthResponse>("/health").await {
                Ok(_) => set_online.set(true),
                Err(_) => set_online.set(false),
            }
        });
    };
    // Initial check
    poll();
    // Set interval
    let poll_clone = poll.clone();
    gloo_timers::callback::Interval::new(30_000, move || poll_clone()).forget();

    let health_class = move || {
        if online.get() { "health-dot online" } else { "health-dot offline" }
    };

    let health_title = move || {
        if online.get() { "Connected" } else { "Disconnected" }
    };

    let username = move || {
        auth_state.get().map(|a| a.username).unwrap_or_default()
    };
    let role = move || {
        auth_state.get().map(|a| a.role).unwrap_or_default()
    };

    let do_logout = move |_| {
        auth::clear_storage_pub();
        if let Some(w) = web_sys::window() {
            let _ = w.location().reload();
        }
    };

    view! {
        <nav id="main-nav">
            <div class="nav-brand">
                <A href="/">
                    <i class="fa-solid fa-brain"></i>
                    <span>"engram"</span>
                </A>
            </div>
            <button class="nav-toggle" on:click=toggle_nav aria-label="Toggle navigation">
                <i class="fa-solid fa-bars"></i>
            </button>
            <ul class=nav_class>
                <li><A href="/" on:click=close_nav><i class="fa-solid fa-house"></i>" Home"</A></li>
                <li><A href="/graph" on:click=close_nav><i class="fa-solid fa-compass"></i>" Explore"</A></li>
                <li><A href="/insights" on:click=close_nav><i class="fa-solid fa-chart-line"></i>" Insights"</A></li>
                <li><A href="/facts" on:click=close_nav><i class="fa-solid fa-check-double"></i>" Facts"</A></li>
                <li><A href="/debate" on:click=close_nav><i class="fa-solid fa-comments"></i>" Debate"</A></li>
                <li><A href="/security" on:click=close_nav><i class="fa-solid fa-shield-halved"></i>" Security"</A></li>
                <li><A href="/system" on:click=close_nav><i class="fa-solid fa-sliders"></i>" System"</A></li>
            </ul>
            <div class="nav-status">
                <div class="nav-user-badge">
                    <span class="nav-username">{username}</span>
                    <span class="badge badge-active" style="font-size: 0.65rem;">{role}</span>
                </div>
                <span class=health_class title=health_title></span>
                <button class="btn-icon" on:click=do_logout title="Logout">
                    <i class="fa-solid fa-right-from-bracket"></i>
                </button>
            </div>
        </nav>
    }
}
