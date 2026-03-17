use leptos::prelude::*;

use crate::api::types::NodeResponse;

pub(super) fn render_info_tab(detail: NodeResponse) -> leptos::prelude::AnyView {
    let confidence = detail.confidence;
    let conf_pct = confidence * 100.0;
    let conf_color = if confidence >= 0.7 {
        "#66bb6a"
    } else if confidence >= 0.4 {
        "#ffa726"
    } else {
        "#ef5350"
    };

    // Extract properties
    let all_props: Vec<(String, String)> = detail
        .properties
        .as_ref()
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let val = if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    };
                    (k.clone(), val)
                })
                .collect()
        })
        .unwrap_or_default();

    // Helper to get a property value
    let get_prop = |key: &str| -> Option<String> {
        all_props.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    };

    // Extract KB ID if present
    let kb_id = get_prop("kb_id");

    // Extract ingest source
    let ingest_source = get_prop("ingest_source");

    // Check if this is a Fact node
    let is_fact = detail
        .node_type
        .as_deref()
        .map(|t| t.eq_ignore_ascii_case("fact"))
        .unwrap_or(false);

    if is_fact {
        // Fact-specific layout
        let claim = get_prop("claim");
        let event_date = get_prop("event_date");
        let source_url = get_prop("source_url");
        let extraction_method = get_prop("extraction_method");
        let status = get_prop("status");

        // User-visible props: exclude internal keys
        let internal_keys = ["claim", "event_date", "source_url", "extraction_method", "status",
                             "kb_id", "ingest_source"];
        let user_props: Vec<(String, String)> = all_props
            .iter()
            .filter(|(k, _)| !k.starts_with('_') && !internal_keys.contains(&k.as_str()))
            .cloned()
            .collect();

        view! {
            <div>
                // Confidence bar
                <div style="margin-bottom: 1rem;">
                    <div style="display: flex; justify-content: space-between; margin-bottom: 0.25rem;">
                        <span class="text-secondary" style="font-size: 0.8rem;">"Confidence"</span>
                        <span style="font-size: 0.8rem; font-weight: 600;">{format!("{:.0}%", conf_pct)}</span>
                    </div>
                    <div style="height: 6px; background: var(--bg-tertiary); border-radius: 3px; overflow: hidden;">
                        <div style=format!("height: 100%; width: {:.0}%; background: {}; border-radius: 3px; transition: width 0.3s;", conf_pct, conf_color)></div>
                    </div>
                </div>

                // Status badge
                {status.map(|s| {
                    let (badge_color, badge_bg) = match s.to_lowercase().as_str() {
                        "active" | "confirmed" => ("#66bb6a", "rgba(102,187,106,0.15)"),
                        "disputed" | "contested" => ("#ffa726", "rgba(255,167,38,0.15)"),
                        "debunked" | "retracted" => ("#ef5350", "rgba(239,83,80,0.15)"),
                        _ => ("#78909c", "rgba(120,144,156,0.15)"),
                    };
                    view! {
                        <div style="margin-bottom: 0.75rem;">
                            <span style=format!("display: inline-block; padding: 2px 10px; border-radius: 12px; font-size: 0.75rem; font-weight: 600; color: {}; background: {};", badge_color, badge_bg)>
                                {s}
                            </span>
                        </div>
                    }
                })}

                // Claim blockquote
                {claim.map(|c| view! {
                    <div style="margin-bottom: 0.75rem;">
                        <div style="font-size: 0.75rem; color: rgba(255,255,255,0.5); text-transform: uppercase; margin-bottom: 0.25rem;">
                            <i class="fa-solid fa-quote-left" style="margin-right: 0.25rem;"></i>"Claim"
                        </div>
                        <blockquote style="margin: 0; padding: 0.5rem 0.75rem; border-left: 3px solid var(--accent-bright, #4fc3f7); background: rgba(255,255,255,0.03); border-radius: 0 4px 4px 0; font-size: 0.9rem; line-height: 1.4;">
                            {c}
                        </blockquote>
                    </div>
                })}

                // Event date
                {event_date.map(|d| view! {
                    <div class="prop-row" style="margin-bottom: 0.5rem;">
                        <span class="prop-key"><i class="fa-solid fa-calendar" style="margin-right: 0.25rem;"></i>"Event date:"</span>
                        <span style="font-size: 0.85rem;">{d}</span>
                    </div>
                })}

                // Source URL
                {source_url.map(|url| view! {
                    <div class="prop-row" style="margin-bottom: 0.5rem;">
                        <span class="prop-key"><i class="fa-solid fa-globe" style="margin-right: 0.25rem;"></i>"Source:"</span>
                        <a href=url.clone() target="_blank" rel="noopener" style="font-size: 0.85rem; color: var(--accent-bright); word-break: break-all;">
                            {if url.len() > 60 { format!("{}...", &url[..57]) } else { url.clone() }}
                            " " <i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 0.7rem;"></i>
                        </a>
                    </div>
                })}

                // Extraction method
                {extraction_method.map(|m| view! {
                    <div class="prop-row" style="margin-bottom: 0.5rem;">
                        <span class="prop-key"><i class="fa-solid fa-microchip" style="margin-right: 0.25rem;"></i>"Extraction:"</span>
                        <span style="font-size: 0.8rem; padding: 1px 8px; background: rgba(255,255,255,0.06); border-radius: 10px;">{m}</span>
                    </div>
                })}

                // Provenance
                {ingest_source.map(|src| view! {
                    <div class="prop-row" style="margin-bottom: 0.5rem;">
                        <span class="prop-key"><i class="fa-solid fa-file-import" style="margin-right: 0.25rem;"></i>"Ingested from:"</span>
                        <span style="font-size: 0.85rem;">{src}</span>
                    </div>
                })}

                // KB Link
                {kb_id.map(|id| {
                    let url = if id.starts_with('Q') {
                        format!("https://www.wikidata.org/wiki/{}", id)
                    } else {
                        id.clone()
                    };
                    view! {
                        <div class="prop-row" style="margin-bottom: 0.5rem;">
                            <span class="prop-key"><i class="fa-solid fa-link" style="margin-right: 0.25rem;"></i>"KB Link:"</span>
                            <a href=url.clone() target="_blank" rel="noopener" style="font-size: 0.85rem; color: var(--accent-bright);">
                                {id}
                                " " <i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 0.7rem;"></i>
                            </a>
                        </div>
                    }
                })}

                // User-added properties (non-internal)
                {if !user_props.is_empty() {
                    Some(view! {
                        <div style="margin-top: 0.5rem;">
                            <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                                <i class="fa-solid fa-table-list" style="margin-right: 0.25rem;"></i>"Properties"
                            </h4>
                            <div style="display: grid; gap: 0.25rem;">
                                {user_props.iter().map(|(k, v)| view! {
                                    <div class="prop-row">
                                        <span class="prop-key">{format!("{}:", k)}</span>
                                        <span style="font-size: 0.85rem; word-break: break-word;">{v.clone()}</span>
                                    </div>
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                } else {
                    None
                }}
            </div>
        }
        .into_any()
    } else {
        // Generic entity layout (non-Fact)

        // Filter out internal properties
        let display_props: Vec<(String, String)> = all_props
            .into_iter()
            .filter(|(k, _)| !k.starts_with('_'))
            .collect();

        view! {
            <div>
                // Confidence bar
                <div style="margin-bottom: 1rem;">
                    <div style="display: flex; justify-content: space-between; margin-bottom: 0.25rem;">
                        <span class="text-secondary" style="font-size: 0.8rem;">"Confidence"</span>
                        <span style="font-size: 0.8rem; font-weight: 600;">{format!("{:.0}%", conf_pct)}</span>
                    </div>
                    <div style="height: 6px; background: var(--bg-tertiary); border-radius: 3px; overflow: hidden;">
                        <div style=format!("height: 100%; width: {:.0}%; background: {}; border-radius: 3px; transition: width 0.3s;", conf_pct, conf_color)></div>
                    </div>
                </div>

                // Provenance
                {ingest_source.map(|src| view! {
                    <div class="prop-row" style="margin-bottom: 0.5rem;">
                        <span class="prop-key"><i class="fa-solid fa-file-import" style="margin-right: 0.25rem;"></i>"Source:"</span>
                        <span style="font-size: 0.85rem;">{src}</span>
                    </div>
                })}

                // KB Link
                {kb_id.map(|id| {
                    let url = if id.starts_with('Q') {
                        format!("https://www.wikidata.org/wiki/{}", id)
                    } else {
                        id.clone()
                    };
                    view! {
                        <div class="prop-row" style="margin-bottom: 0.5rem;">
                            <span class="prop-key"><i class="fa-solid fa-link" style="margin-right: 0.25rem;"></i>"KB Link:"</span>
                            <a href=url.clone() target="_blank" rel="noopener" style="font-size: 0.85rem; color: var(--accent-bright);">
                                {id}
                                " " <i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 0.7rem;"></i>
                            </a>
                        </div>
                    }
                })}

                // Properties table
                {if display_props.is_empty() {
                    view! {
                        <p class="text-secondary" style="font-size: 0.85rem;">"No properties stored for this entity."</p>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                                <i class="fa-solid fa-table-list" style="margin-right: 0.25rem;"></i>"Properties"
                            </h4>
                            <div style="display: grid; gap: 0.25rem;">
                                {display_props.iter().map(|(k, v)| view! {
                                    <div class="prop-row">
                                        <span class="prop-key">{format!("{}:", k)}</span>
                                        <span style="font-size: 0.85rem; word-break: break-word;">{v.clone()}</span>
                                    </div>
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    }.into_any()
                }}
            </div>
        }
        .into_any()
    }
}
