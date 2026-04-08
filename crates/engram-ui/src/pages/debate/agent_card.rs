/// Live agent dashboard card for the War Room.
/// Compact: gauges, stats, persona, bias. Positions go to the activity feed.

use leptos::prelude::*;
use crate::api::types::DebateAgent;
use super::war_room::AgentLiveState;

#[component]
pub fn AgentCard(
    agent: DebateAgent,
    state: AgentLiveState,
) -> impl IntoView {
    let conf = state.confidence_history.last().copied().unwrap_or(0.0);
    let conf_pct = (conf * 100.0) as u32;
    let color = agent.color.clone();
    let active_class = if state.is_active { " agent-card-active" } else { "" };

    // Gauge color based on confidence
    let gauge_color = if conf >= 0.7 { "var(--success)" }
        else if conf >= 0.4 { "var(--warning)" }
        else { "var(--danger)" };

    // Trust: supported vs contradicted
    let total_checks = state.supported_claims + state.contradicted_claims;
    let trust_icon = if total_checks == 0 { "fa-solid fa-circle-question" }
        else if state.contradicted_claims == 0 { "fa-solid fa-circle-check" }
        else if state.supported_claims > state.contradicted_claims { "fa-solid fa-circle-check" }
        else { "fa-solid fa-circle-xmark" };
    let trust_color = if total_checks == 0 { "var(--text-muted)" }
        else if state.contradicted_claims == 0 { "var(--success)" }
        else if state.supported_claims > state.contradicted_claims { "var(--warning)" }
        else { "var(--danger)" };

    // Sparkline data
    let sparkline_bars: Vec<_> = state.confidence_history.iter().map(|c| {
        let h = (*c * 100.0).max(4.0);
        format!("height: {}%;", h)
    }).collect();

    // Persona truncated
    let persona_short = agent.persona_description.char_indices().nth(80)
        .map(|(i, _)| format!("{}...", &agent.persona_description[..i]))
        .unwrap_or_else(|| agent.persona_description.clone());
    let bias_label = agent.bias.label.clone();
    let is_neutral = agent.bias.is_neutral;

    view! {
        <div class={format!("agent-card-war{}", active_class)}
             style={format!("border-left: 4px solid {};", color)}>
            // Header: icon + name
            <div style="display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.4rem;">
                <i class={format!("fa-solid {}", agent.icon)} style={format!("color: {};", color)}></i>
                <strong style="font-size: 0.85rem; flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">
                    {agent.name.clone()}
                </strong>
            </div>

            // Confidence gauge + sparkline row
            <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.4rem;">
                <div class="confidence-gauge"
                     style={format!("--confidence: {}; --gauge-color: {};", conf_pct, gauge_color)}>
                    <span class="confidence-gauge-value">{format!("{}%", conf_pct)}</span>
                </div>
                <div class="sparkline">
                    {sparkline_bars.iter().map(|style| {
                        let s = style.clone();
                        view! { <div class="sparkline-bar" style=s></div> }
                    }).collect::<Vec<_>>()}
                </div>
            </div>

            // Stats row: evidence + trust
            <div style="display: flex; gap: 0.5rem; font-size: 0.75rem; margin-bottom: 0.3rem;">
                <span title="Evidence found">
                    <i class="fa-solid fa-layer-group" style="opacity: 0.6;"></i>
                    " " {state.evidence_count.to_string()}
                </span>
                <span title="Fact-check trust" style={format!("color: {};", trust_color)}>
                    <i class=trust_icon></i>
                    {if total_checks > 0 {
                        format!(" {}/{}", state.supported_claims, total_checks)
                    } else {
                        String::new()
                    }}
                </span>
                // Agreement dots
                {if !state.agrees_with.is_empty() {
                    Some(view! {
                        <span title="Agrees with" style="color: var(--success);">
                            <i class="fa-solid fa-handshake" style="font-size: 0.7rem;"></i>
                            " " {state.agrees_with.len().to_string()}
                        </span>
                    })
                } else { None }}
                {if !state.disagrees_with.is_empty() {
                    Some(view! {
                        <span title="Disagrees with" style="color: var(--danger);">
                            <i class="fa-solid fa-hand-fist" style="font-size: 0.7rem;"></i>
                            " " {state.disagrees_with.len().to_string()}
                        </span>
                    })
                } else { None }}
            </div>

            // Persona description
            <div class="text-muted" style="font-size: 0.7rem; line-height: 1.3; margin-bottom: 0.3rem; overflow: hidden; max-height: 2.6em;">
                {persona_short}
            </div>

            // Bias badge
            {(!is_neutral).then(|| view! {
                <span class="badge" style="font-size: 0.65rem; padding: 0.1rem 0.4rem;">
                    <i class="fa-solid fa-flag" style="font-size: 0.55rem; margin-right: 0.2rem;"></i>
                    {bias_label}
                </span>
            })}
        </div>
    }
}
