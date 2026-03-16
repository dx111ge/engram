use leptos::prelude::*;

/// NL page -- redirects users to the Knowledge Chat panel (floating, available on Explore/Insights).
/// The old /tell + /ask pattern-matching interface is superseded by LLM-powered chat with tool calling.
#[component]
pub fn NlPage() -> impl IntoView {
    // Auto-open chat panel if context signal is available
    let chat_open = use_context::<RwSignal<bool>>();
    if let Some(open) = chat_open {
        open.set(true);
    }

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-comments"></i>" Natural Language"</h2>
        </div>

        <div class="card" style="max-width:600px;margin:2rem auto;text-align:center;padding:2rem;">
            <i class="fa-solid fa-arrow-right-to-bracket" style="font-size:2rem;color:var(--accent, #4a9eff);margin-bottom:1rem;display:block;"></i>
            <h3 style="margin:0 0 0.5rem;">"Use Knowledge Chat"</h3>
            <p style="color:var(--text-muted, #8b8fa3);margin:0 0 1.5rem;">
                "The natural language interface has been replaced by the Knowledge Chat panel. "
                "Open it with the "<i class="fa-solid fa-comments"></i>" button in the bottom-right corner on the Explore or Insights pages."
            </p>
            <p style="color:var(--text-muted, #8b8fa3);font-size:0.85rem;">
                "Knowledge Chat provides 40+ tools, auto-context retrieval, write confirmation, "
                "temporal awareness, and follow-up suggestions."
            </p>
        </div>
    }
}
