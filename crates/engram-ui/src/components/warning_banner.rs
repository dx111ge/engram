use leptos::prelude::*;

/// Permanent warning banner (e.g., for LLM-generated suggestions).
#[component]
pub fn WarningBanner(
    #[prop(into)] message: String,
) -> impl IntoView {
    view! {
        <div class="warning-banner">
            <i class="fa-solid fa-triangle-exclamation"></i>
            <span>{message}</span>
        </div>
    }
}
