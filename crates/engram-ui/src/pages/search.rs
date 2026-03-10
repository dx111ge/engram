use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::SearchResult;

#[component]
pub fn SearchPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (query, set_query) = signal(String::new());
    let (results, set_results) = signal(Vec::<SearchResult>::new());
    let (loading, set_loading) = signal(false);
    let (search_mode, set_search_mode) = signal("hybrid".to_string());

    let api_c = api.clone();
    let do_search = Action::new_local(move |_: &()| {
        let api = api_c.clone();
        let q = query.get_untracked();
        let mode = search_mode.get_untracked();
        async move {
            set_loading.set(true);
            let path = match mode.as_str() {
                "bm25" => "/search",
                "semantic" => "/similar",
                _ => "/query",
            };
            let body = serde_json::json!({"query": q, "limit": 50});
            match api.post::<_, Vec<SearchResult>>(path, &body).await {
                Ok(r) => set_results.set(r),
                Err(_) => set_results.set(vec![]),
            }
            set_loading.set(false);
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        do_search.dispatch(());
    };

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-magnifying-glass"></i>" Search"</h2>
        </div>

        <form class="search-form" on:submit=on_submit>
            <div class="search-bar">
                <input
                    type="text"
                    placeholder="Search the knowledge graph..."
                    class="search-input large"
                    prop:value=query
                    on:input=move |ev| set_query.set(event_target_value(&ev))
                />
                <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                    {move || if loading.get() {
                        "Searching..."
                    } else {
                        "Search"
                    }}
                </button>
            </div>
            <div class="search-options">
                <label>
                    <input type="radio" name="mode" value="hybrid"
                        checked=move || search_mode.get() == "hybrid"
                        on:change=move |_| set_search_mode.set("hybrid".into())
                    />
                    " Hybrid"
                </label>
                <label>
                    <input type="radio" name="mode" value="bm25"
                        checked=move || search_mode.get() == "bm25"
                        on:change=move |_| set_search_mode.set("bm25".into())
                    />
                    " BM25"
                </label>
                <label>
                    <input type="radio" name="mode" value="semantic"
                        checked=move || search_mode.get() == "semantic"
                        on:change=move |_| set_search_mode.set("semantic".into())
                    />
                    " Semantic"
                </label>
            </div>
        </form>

        <div class="results-list">
            <For
                each={move || results.get()}
                key={|r| r.label.clone()}
                children={move |result| {
                    let nt = result.node_type.clone();
                    view! {
                        <div class="result-card">
                            <div class="result-header">
                                <span class="result-label">{result.label.clone()}</span>
                                <span class="result-score">{format!("{:.3}", result.score)}</span>
                            </div>
                            {nt.map(|t| view! {
                                <span class="badge">{t}</span>
                            })}
                        </div>
                    }
                }}
            />
        </div>
    }
}
