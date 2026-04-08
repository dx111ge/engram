/// Activity feed: real-time rolling log of debate events.
/// The CENTER panel -- this is the main content area where positions are readable.

use leptos::prelude::*;
use super::war_room::FeedEntry;

#[component]
pub fn ActivityFeed(
    entries: ReadSignal<Vec<FeedEntry>>,
    progress_msg: ReadSignal<String>,
) -> impl IntoView {
    view! {
        <div>
            <h4 style="margin: 0 0 0.5rem; font-size: 0.85rem; color: var(--text-secondary);">
                <i class="fa-solid fa-stream" style="margin-right: 0.3rem;"></i>"Activity"
            </h4>

            // Progress bar
            {move || {
                let msg = progress_msg.get();
                if msg.is_empty() { None } else {
                    Some(view! {
                        <div style="padding: 0.3rem 0.5rem; font-size: 0.75rem; background: var(--bg-tertiary); border-radius: 4px; margin-bottom: 0.5rem; color: var(--accent-bright);">
                            <i class="fa-solid fa-spinner fa-spin" style="margin-right: 0.3rem;"></i>
                            {msg}
                        </div>
                    })
                }
            }}

            // Feed entries
            <div style="display: flex; flex-direction: column; gap: 2px;">
                {move || {
                    let items = entries.get();
                    if items.is_empty() {
                        view! {
                            <div class="text-muted" style="font-size: 0.8rem; padding: 1rem; text-align: center;">
                                <i class="fa-solid fa-spinner fa-spin" style="margin-right: 0.3rem;"></i>
                                "Waiting for debate events..."
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div>
                                {items.iter().take(50).map(|entry| {
                                    let border_color = entry.agent_color.clone().unwrap_or_else(|| "var(--border)".into());
                                    let border_color_inner = border_color.clone();
                                    let detail = entry.detail.clone();
                                    let has_detail = detail.is_some();
                                    // Auto-expand position entries (turn_complete) so users can READ them
                                    let is_position = entry.event_type == "turn_complete";
                                    let (expanded, set_expanded) = signal(is_position);
                                    view! {
                                        <div class="feed-entry"
                                             style={format!("border-left-color: {};{}", border_color,
                                                if is_position { " background: var(--bg-tertiary);" } else { "" })}
                                             on:click=move |_| { if has_detail { set_expanded.update(|v| *v = !*v); } }>
                                            <div style="display: flex; align-items: baseline; gap: 0.4rem;">
                                                <span class="text-muted" style="font-size: 0.65rem; min-width: 4.5em;">
                                                    {entry.timestamp.clone()}
                                                </span>
                                                <i class={entry.icon.clone()} style="font-size: 0.7rem; opacity: 0.7; min-width: 1em;"></i>
                                                <span style={format!("flex: 1;{}", if is_position { " font-weight: 600;" } else { "" })}>
                                                    {entry.summary.clone()}
                                                </span>
                                                {has_detail.then(|| view! {
                                                    <i class={move || if expanded.get() { "fa-solid fa-chevron-up" } else { "fa-solid fa-chevron-down" }}
                                                       style="font-size: 0.6rem; opacity: 0.4; cursor: pointer;"></i>
                                                })}
                                            </div>
                                            {move || {
                                                if expanded.get() {
                                                    detail.clone().map(|d| view! {
                                                        <div style={format!("margin-top: 0.3rem; padding: 0.4rem 0.5rem; background: var(--bg-primary); border-radius: 4px; font-size: 0.78rem; line-height: 1.5; white-space: pre-wrap; max-height: 300px; overflow-y: auto; border-left: 3px solid {};", border_color_inner)}>
                                                            {d}
                                                        </div>
                                                    })
                                                } else { None }
                                            }}
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
