use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::api::ApiClient;
use crate::components::nav::Nav;
use crate::pages;

#[component]
pub fn App() -> impl IntoView {
    // Provide the API client to all child components
    let base_url = ApiClient::load_base_url();
    provide_context(ApiClient::new(&base_url));

    view! {
        <Router>
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
                </Routes>
            </main>
        </Router>
    }
}
