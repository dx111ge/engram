use leptos::prelude::*;

#[component]
pub fn CollapsibleSection(
    #[prop(into)] title: String,
    #[prop(into)] icon: String,
    #[prop(optional, into)] status: Option<Signal<String>>,
    #[prop(optional, default = false)] collapsed: bool,
    children: Children,
) -> impl IntoView {
    let (open, set_open) = signal(!collapsed);

    let toggle = move |_| set_open.update(|v| *v = !*v);

    let chevron = move || {
        if open.get() { "fa-solid fa-chevron-down" } else { "fa-solid fa-chevron-right" }
    };

    let body_style = move || {
        if open.get() { "display: block;" } else { "display: none;" }
    };

    view! {
        <div class="card collapsible-section">
            <div class="section-header clickable" on:click=toggle>
                <div class="flex" style="align-items: center; gap: 0.5rem;">
                    <i class=icon></i>
                    <h3>{title.clone()}</h3>
                </div>
                <div class="flex" style="align-items: center; gap: 0.5rem;">
                    {status.map(|sig| view! {
                        <span class=move || {
                            let val = sig.get();
                            let lower = val.to_lowercase();
                            if lower.contains("not enabled") || lower.contains("no secrets") || lower.contains("not configured") || lower.contains("disabled") || lower.contains("error") {
                                "section-status section-status-amber"
                            } else if lower.is_empty() || lower.contains("setup") {
                                "section-status"
                            } else {
                                "section-status section-status-green"
                            }
                        }>{move || sig.get()}</span>
                    })}
                    <i class=chevron></i>
                </div>
            </div>
            <div class="section-body" style=body_style>
                {children()}
            </div>
        </div>
    }
}
