use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::api::ApiClient;
use crate::api::types::ConfigStatusResponse;
use crate::auth;
use crate::components::auth_screen::AuthScreen;
use crate::components::chat_panel::ChatPanel;
use crate::components::chat_types::{ChatSelectedNode, ChatCurrentAssessment};
use crate::components::nav::Nav;
use crate::components::onboarding_wizard::OnboardingWizard;
use crate::components::toast::{Toast, ToastContainer};
use crate::pages;

/// Global debate state -- survives route changes, accessible from Nav and any page.
#[derive(Clone, Debug)]
pub struct ActiveDebate {
    pub session_id: RwSignal<Option<String>>,
    pub status: RwSignal<String>,
    pub topic: RwSignal<String>,
    pub mode: RwSignal<String>,
    pub current_round: RwSignal<usize>,
    pub max_rounds: RwSignal<usize>,
}

impl ActiveDebate {
    pub fn is_running(&self) -> bool {
        matches!(self.status.get_untracked().as_str(),
            "running" | "researching" | "gap_closing" | "synthesizing")
    }
    pub fn is_active(&self) -> bool {
        self.session_id.get_untracked().is_some() &&
        !matches!(self.status.get_untracked().as_str(), "" | "complete" | "error")
    }
}

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

    // Provide global debate state (survives route changes)
    let active_debate = ActiveDebate {
        session_id: RwSignal::new(None),
        status: RwSignal::new(String::new()),
        topic: RwSignal::new(String::new()),
        mode: RwSignal::new(String::new()),
        current_round: RwSignal::new(0),
        max_rounds: RwSignal::new(0),
    };
    provide_context(active_debate);

    // Provide page context signals for chat panel
    provide_context(ChatSelectedNode(RwSignal::new(None)));
    provide_context(ChatCurrentAssessment(RwSignal::new(None)));

    let is_authed = move || auth_state.get().is_some();

    // Wizard visibility signal -- provide as context so System page can trigger it
    let (wizard_open, set_wizard_open) = signal(false);
    provide_context(set_wizard_open);

    // Check config status when authenticated — auto-show wizard only if empty graph & not dismissed
    let api_for_check = use_context::<ApiClient>().unwrap_or_else(|| ApiClient::new(&base_url));
    let check_status = Action::new_local(move |_: &()| {
        let api = api_for_check.clone();
        let set_open = set_wizard_open;
        async move {
            if let Ok(status) = api.get::<ConfigStatusResponse>("/config/status").await {
                if status.is_empty_graph && !status.wizard_dismissed {
                    set_open.set(true);
                }
            }
        }
    });

    // Trigger the check once auth state becomes Some
    Effect::new(move || {
        if auth_state.get().is_some() {
            check_status.dispatch(());
        }
    });

    let on_wizard_complete = Callback::new(move |()| {
        set_wizard_open.set(false);
    });

    view! {
        <Router>
            {move || {
                if is_authed() {
                    view! {
                        <Nav />
                        <main id="content">
                            <Routes fallback=|| view! { <pages::not_found::NotFound /> }>
                                // ── Primary nav pages ──
                                <Route path=path!("/") view=pages::graph::GraphPage />
                                <Route path=path!("/insights") view=pages::insights::InsightsPage />
                                <Route path=path!("/debate") view=pages::debate::DebatePage />
                                <Route path=path!("/system") view=pages::system::SystemPage />
                                // ── Sub-pages (accessed from within main pages) ──
                                <Route path=path!("/knowledge") view=pages::graph::GraphPage />
                                <Route path=path!("/graph") view=pages::graph::GraphPage />
                                <Route path=path!("/search") view=pages::search::SearchPage />
                                <Route path=path!("/nl") view=pages::nl::NlPage />
                                <Route path=path!("/import") view=pages::import::ImportPage />
                                <Route path=path!("/learning") view=pages::learning::LearningPage />
                                <Route path=path!("/ingest") view=pages::ingest::IngestPage />
                                // /sources removed -- merged into System page Ingestion Sources section
                                <Route path=path!("/actions") view=pages::actions::ActionsPage />
                                <Route path=path!("/gaps") view=pages::gaps::GapsPage />
                                <Route path=path!("/mesh") view=pages::mesh::MeshPage />
                                <Route path=path!("/node/:label") view=pages::node::NodePage />
                                // ── Backward compat (absorbed into other pages) ──
                                <Route path=path!("/facts") view=pages::facts::FactReviewPage />
                                <Route path=path!("/security") view=pages::security::SecurityPage />
                            </Routes>
                        </main>
                        <OnboardingWizard open=wizard_open on_complete=on_wizard_complete />
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
