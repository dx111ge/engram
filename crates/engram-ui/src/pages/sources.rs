use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::SourceInfo;

#[component]
pub fn SourcesPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let api_c = api.clone();
    let sources = LocalResource::new(move || {
        let api = api_c.clone();
        async move { api.get::<Vec<SourceInfo>>("/sources").await.ok().unwrap_or_default() }
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-plug"></i>" Sources"</h2>
        </div>

        <div class="table-wrapper">
            <table class="data-table">
                <thead>
                    <tr>
                        <th>"Name"</th>
                        <th>"Type"</th>
                        <th>"Ingested"</th>
                        <th>"Actions"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        sources.get()
                            .map(|list: Vec<SourceInfo>| {
                                list.into_iter().map(|src| {
                                    let name = src.name.clone();
                                    let usage_href = format!("#/sources/{}/usage", name);
                                    view! {
                                        <tr>
                                            <td>{src.name.clone()}</td>
                                            <td>{src.source_type.clone().unwrap_or_default()}</td>
                                            <td>{src.total_ingested.map(|n| n.to_string()).unwrap_or_default()}</td>
                                            <td>
                                                <a href=usage_href class="btn btn-small">
                                                    <i class="fa-solid fa-chart-line"></i>" Usage"
                                                </a>
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()
                            })
                    }}
                </tbody>
            </table>
        </div>
    }
}
