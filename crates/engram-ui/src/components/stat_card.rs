use leptos::prelude::*;

#[component]
pub fn StatCard(
    #[prop(into)] icon: String,
    #[prop(into)] label: String,
    #[prop(into)] value: Signal<String>,
) -> impl IntoView {
    view! {
        <div class="stat-card">
            <div class="stat-icon">
                <i class=icon></i>
            </div>
            <div class="stat-content">
                <div class="stat-value">{value}</div>
                <div class="stat-label">{label}</div>
            </div>
        </div>
    }
}
