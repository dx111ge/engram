use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::api::ApiClient;
use crate::auth;
use crate::components::auth_screen::AuthScreen;
use crate::components::chat_panel::ChatPanel;
use crate::components::nav::Nav;
use crate::components::toast::{Toast, ToastContainer};
use crate::pages;

#[component]
pub fn App() -> impl IntoView {
    // Provide API client
    let base_url = ApiClient::load_base_url();
    provide_context(ApiClient::new(&base_url));

    // Provide auth state
    let auth_state = auth::provide_auth();

    // Provide toast context
    let (toasts, set_toasts) = signal(Vec::<Toast>::new());
    provide_context(toasts);
    provide_context(set_toasts);

    // Provide chat open state
    let chat_open = RwSignal::new(false);
    provide_context(chat_open);

    let is_authed = move || auth_state.get().is_some();

    view! {
        <Router>
            {move || {
                if is_authed() {
                    view! {
                        <Nav />
                        <main id="content">
                            <Routes fallback=|| view! { <pages::not_found::NotFound /> }>
                                <Route path=path!("/") view=pages::dashboard::Dashboard />
                                <Route path=path!("/graph") view=pages::graph::GraphPage />
                                <Route path=path!("/search") view=pages::search::SearchPage />
                                <Route path=path!("/nl") view=pages::nl::NlPage />
                                <Route path=path!("/import") view=pages::import::ImportPage />
                                <Route path=path!("/learning") view=pages::learning::LearningPage />
                                <Route path=path!("/ingest") view=pages::ingest::IngestPage />
                                <Route path=path!("/sources") view=pages::sources::SourcesPage />
                                <Route path=path!("/actions") view=pages::actions::ActionsPage />
                                <Route path=path!("/gaps") view=pages::gaps::GapsPage />
                                <Route path=path!("/mesh") view=pages::mesh::MeshPage />
                                <Route path=path!("/system") view=pages::system::SystemPage />
                                <Route path=path!("/security") view=pages::security::SecurityPage />
                                <Route path=path!("/node/:label") view=pages::node::NodePage />
                                <Route path=path!("/insights") view=pages::insights::InsightsPage />
                            </Routes>
                        </main>
                        <ChatPanel />
                        <ToastContainer />
                    }.into_any()
                } else {
                    view! {
                        <AuthScreen />
                    }.into_any()
                }
            }}
        </Router>
    }
}
