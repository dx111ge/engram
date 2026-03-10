use leptos::prelude::*;

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-triangle-exclamation"></i>" 404 - Page Not Found"</h2>
        </div>
        <p>"The page you're looking for doesn't exist."</p>
        <a href="/" class="btn btn-primary">
            <i class="fa-solid fa-home"></i>" Back to Dashboard"
        </a>
    }
}
