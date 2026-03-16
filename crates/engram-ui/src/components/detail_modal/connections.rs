use leptos::prelude::*;

use crate::api::types::NodeResponse;

pub(super) fn render_connections_tab(
    detail: NodeResponse,
    set_current_node_id: WriteSignal<Option<String>>,
) -> leptos::prelude::AnyView {
    let edges_from = detail.edges_from.clone();
    let edges_to = detail.edges_to.clone();

    view! {
        <div>
            // Outgoing edges
            <div style="margin-bottom: 1.5rem;">
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-arrow-right" style="margin-right: 0.25rem;"></i>
                    "Outgoing (" {edges_from.len().to_string()} ")"
                </h4>
                {if edges_from.is_empty() {
                    view! { <p class="text-secondary" style="font-size: 0.85rem;">"No outgoing connections."</p> }.into_any()
                } else {
                    view! {
                        <div style="display: grid; gap: 0.25rem;">
                            {edges_from.iter().map(|e| {
                                let to = e.to.clone();
                                let rel = e.relationship.clone();
                                let conf = e.confidence;
                                let to_click = to.clone();
                                view! {
                                    <div class="prop-row" style="cursor: pointer; padding: 0.35rem 0.5rem; border-radius: 4px;"
                                        title="Click to navigate"
                                        on:click=move |_| {
                                            set_current_node_id.set(Some(to_click.clone()));
                                        }>
                                        <span style="font-size: 0.8rem; color: var(--accent-bright);">
                                            {rel}
                                        </span>
                                        <span style="font-size: 0.85rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <i class="fa-solid fa-arrow-right" style="font-size: 0.65rem; opacity: 0.5;"></i>
                                            <strong>{to}</strong>
                                            <span class="text-secondary" style="font-size: 0.75rem;">{format!("{:.0}%", conf * 100.0)}</span>
                                        </span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>

            // Incoming edges
            <div>
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-arrow-left" style="margin-right: 0.25rem;"></i>
                    "Incoming (" {edges_to.len().to_string()} ")"
                </h4>
                {if edges_to.is_empty() {
                    view! { <p class="text-secondary" style="font-size: 0.85rem;">"No incoming connections."</p> }.into_any()
                } else {
                    view! {
                        <div style="display: grid; gap: 0.25rem;">
                            {edges_to.iter().map(|e| {
                                let from = e.from.clone();
                                let rel = e.relationship.clone();
                                let conf = e.confidence;
                                let from_click = from.clone();
                                view! {
                                    <div class="prop-row" style="cursor: pointer; padding: 0.35rem 0.5rem; border-radius: 4px;"
                                        title="Click to navigate"
                                        on:click=move |_| {
                                            set_current_node_id.set(Some(from_click.clone()));
                                        }>
                                        <span style="font-size: 0.85rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <strong>{from}</strong>
                                            <i class="fa-solid fa-arrow-right" style="font-size: 0.65rem; opacity: 0.5;"></i>
                                        </span>
                                        <span style="font-size: 0.8rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <span style="color: var(--accent-bright);">{rel}</span>
                                            <span class="text-secondary" style="font-size: 0.75rem;">{format!("{:.0}%", conf * 100.0)}</span>
                                        </span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
    .into_any()
}
