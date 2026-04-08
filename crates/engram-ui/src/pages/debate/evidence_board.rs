/// Evidence board: facts discovered during the debate, grouped by source type.

use leptos::prelude::*;
use super::war_room::EvidenceItem;

#[component]
pub fn EvidenceBoard(
    items: ReadSignal<Vec<EvidenceItem>>,
) -> impl IntoView {
    view! {
        <div>
            <h4 style="margin: 0 0 0.5rem; font-size: 0.85rem; color: var(--text-secondary);">
                <i class="fa-solid fa-layer-group" style="margin-right: 0.3rem;"></i>"Evidence"
            </h4>

            // Running totals
            {move || {
                let ev = items.get();
                let total = ev.len();
                let graph = ev.iter().filter(|e| e.source_type == "graph").count();
                let web = ev.iter().filter(|e| e.source_type == "web").count();
                let wiki = ev.iter().filter(|e| e.source_type == "wikipedia").count();
                let sparql = ev.iter().filter(|e| e.source_type == "sparql").count();

                view! {
                    <div style="display: flex; flex-wrap: wrap; gap: 0.3rem; margin-bottom: 0.5rem; font-size: 0.75rem;">
                        <span class="badge" style="font-size: 0.7rem;">
                            {format!("{} total", total)}
                        </span>
                        {(graph > 0).then(|| view! {
                            <span class="badge badge-core" style="font-size: 0.7rem;">
                                <i class="fa-solid fa-database" style="font-size: 0.6rem;"></i>
                                {format!(" {} graph", graph)}
                            </span>
                        })}
                        {(web > 0).then(|| view! {
                            <span class="badge badge-active" style="font-size: 0.7rem;">
                                <i class="fa-solid fa-globe" style="font-size: 0.6rem;"></i>
                                {format!(" {} web", web)}
                            </span>
                        })}
                        {(wiki > 0).then(|| view! {
                            <span class="badge" style="font-size: 0.7rem; background: var(--bg-secondary);">
                                <i class="fa-brands fa-wikipedia-w" style="font-size: 0.6rem;"></i>
                                {format!(" {} wiki", wiki)}
                            </span>
                        })}
                        {(sparql > 0).then(|| view! {
                            <span class="badge" style="font-size: 0.7rem; background: var(--bg-secondary);">
                                <i class="fa-solid fa-project-diagram" style="font-size: 0.6rem;"></i>
                                {format!(" {} sparql", sparql)}
                            </span>
                        })}
                    </div>

                    // Evidence list
                    <div style="display: flex; flex-direction: column; gap: 2px;">
                        {ev.iter().take(50).map(|item| {
                            let conf_pct = (item.confidence * 100.0) as u32;
                            let color = item.agent_color.clone().unwrap_or_default();
                            let source_icon = match item.source_type.as_str() {
                                "graph" => "fa-solid fa-database",
                                "web" => "fa-solid fa-globe",
                                "wikipedia" => "fa-brands fa-wikipedia-w",
                                "sparql" => "fa-solid fa-project-diagram",
                                _ => "fa-solid fa-circle",
                            };
                            view! {
                                <div style="padding: 0.2rem 0.4rem; font-size: 0.75rem; background: var(--bg-secondary); border-radius: 3px; display: flex; align-items: center; gap: 0.3rem;">
                                    {(!color.is_empty()).then(|| view! {
                                        <span style={format!("width: 6px; height: 6px; border-radius: 50%; background: {}; flex-shrink: 0;", color)}></span>
                                    })}
                                    <i class=source_icon style="font-size: 0.65rem; opacity: 0.6; flex-shrink: 0;"></i>
                                    <span style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                        {item.entity.clone()}
                                    </span>
                                    <span class="text-muted" style="font-size: 0.65rem; flex-shrink: 0;">
                                        {format!("{}%", conf_pct)}
                                    </span>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }
            }}
        </div>
    }
}
