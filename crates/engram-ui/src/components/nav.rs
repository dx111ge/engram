use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn Nav() -> impl IntoView {
    let (mobile_open, set_mobile_open) = signal(false);

    let toggle_nav = move |_| {
        set_mobile_open.update(|v| *v = !*v);
    };

    let nav_class = move || {
        if mobile_open.get() {
            "nav-links open"
        } else {
            "nav-links"
        }
    };

    // Close mobile nav on link click
    let close_nav = move |_| {
        set_mobile_open.set(false);
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
                <li><A href="/" on:click=close_nav><i class="fa-solid fa-gauge"></i>" Dashboard"</A></li>
                <li><A href="/graph" on:click=close_nav><i class="fa-solid fa-diagram-project"></i>" Graph"</A></li>
                <li><A href="/search" on:click=close_nav><i class="fa-solid fa-magnifying-glass"></i>" Search"</A></li>
                <li><A href="/nl" on:click=close_nav><i class="fa-solid fa-comments"></i>" Natural Language"</A></li>
                <li><A href="/import" on:click=close_nav><i class="fa-solid fa-file-import"></i>" Import"</A></li>
                <li><A href="/learning" on:click=close_nav><i class="fa-solid fa-graduation-cap"></i>" Learning"</A></li>
                <li class="nav-separator"></li>
                <li><A href="/ingest" on:click=close_nav><i class="fa-solid fa-gears"></i>" Ingest"</A></li>
                <li><A href="/sources" on:click=close_nav><i class="fa-solid fa-plug"></i>" Sources"</A></li>
                <li><A href="/actions" on:click=close_nav><i class="fa-solid fa-bolt"></i>" Actions"</A></li>
                <li><A href="/gaps" on:click=close_nav><i class="fa-solid fa-map"></i>" Gaps"</A></li>
                <li><A href="/mesh" on:click=close_nav><i class="fa-solid fa-network-wired"></i>" Mesh"</A></li>
            </ul>
            <div class="nav-status">
                <span id="health-indicator" class="health-dot offline" title="Checking..."></span>
                <button class="btn-icon" id="settings-btn" title="API Settings">
                    <i class="fa-solid fa-gear"></i>
                </button>
            </div>
        </nav>
    }
}
