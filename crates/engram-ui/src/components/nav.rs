use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::ApiClient;
use crate::api::types::HealthResponse;
use crate::app::ActiveDebate;
use crate::auth;

#[component]
pub fn Nav() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let auth_state = auth::use_auth();

    let (mobile_open, set_mobile_open) = signal(false);
    let (online, set_online) = signal(false);

    let toggle_nav = move |_| set_mobile_open.update(|v| *v = !*v);
    let close_nav = move |_: leptos::ev::MouseEvent| set_mobile_open.set(false);

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

    // Global debate state for navigation guard
    let debate = use_context::<ActiveDebate>();

    // Navigation guard: intercept clicks when debate is running
    let make_nav_handler = |href: &'static str| {
        let debate = debate.clone();
        move |ev: leptos::ev::MouseEvent| {
            ev.prevent_default();
            set_mobile_open.set(false);
            let nav = use_navigate();
            if href == "/debate" {
                nav(href, Default::default());
                return;
            }
            if let Some(ref d) = debate {
                if d.is_running() {
                    let msg = "A debate is currently running. Leaving will disconnect the live feed.\n\nAre you sure?";
                    if let Some(w) = web_sys::window() {
                        match w.confirm_with_message(msg) {
                            Ok(true) => nav(href, Default::default()),
                            _ => {}
                        }
                    }
                    return;
                }
            }
            nav(href, Default::default());
        }
    };
    let nav_home = make_nav_handler("/");
    let nav_insights = make_nav_handler("/insights");
    let nav_debate = make_nav_handler("/debate");
    let nav_system = make_nav_handler("/system");

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
                <li><a href="/" on:click=nav_home><i class="fa-solid fa-brain"></i>" Knowledge"</a></li>
                <li><a href="/insights" on:click=nav_insights><i class="fa-solid fa-chart-line"></i>" Insights"</a></li>
                <li><a href="/debate" on:click=nav_debate><i class="fa-solid fa-comments"></i>" Debate"</a></li>
                <li><a href="/system" on:click=nav_system><i class="fa-solid fa-sliders"></i>" System"</a></li>
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
        // Layer 3: Global debate banner (shown on all pages when debate is active, NOT on debate page)
        {move || {
            let debate_banner = use_context::<ActiveDebate>();
            if let Some(ref d) = debate_banner {
                let status = d.status.get();
                let sid = d.session_id.get();
                let is_active = sid.is_some() && !matches!(status.as_str(), "" | "complete" | "error");
                // Check if we're currently on the debate page
                let on_debate = web_sys::window()
                    .and_then(|w| w.location().pathname().ok())
                    .map(|p| p == "/debate")
                    .unwrap_or(false);
                if is_active && !on_debate {
                    let topic = d.topic.get();
                    let round = d.current_round.get();
                    let max = d.max_rounds.get();
                    let status_label = match status.as_str() {
                        "running" | "researching" => "running",
                        "gap_closing" => "researching gaps",
                        "awaiting_input" => "waiting for input",
                        "all_rounds_complete" => "ready to synthesize",
                        "synthesizing" => "synthesizing",
                        "panel_ready" => "panel ready",
                        _ => &status,
                    };
                    Some(view! {
                        <div class="debate-active-banner" on:click=move |_| { use_navigate()("/debate", Default::default()); }
                             style="position: fixed; bottom: 0; left: 0; right: 0; z-index: 1000; padding: 0.4rem 1rem; background: var(--accent-bright); color: var(--bg-primary); cursor: pointer; display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; font-weight: 600;">
                            <i class="fa-solid fa-comments"></i>
                            <span>{format!("Debate {} -- Round {}/{} -- {}", status_label, round.max(1), max, topic)}</span>
                            <span style="margin-left: auto; opacity: 0.8; font-weight: 400;">"Click to return"</span>
                            <i class="fa-solid fa-arrow-right"></i>
                        </div>
                    })
                } else { None }
            } else { None }
        }}
    }
}
