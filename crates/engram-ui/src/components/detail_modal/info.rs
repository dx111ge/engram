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
    let props: Vec<(String, String)> = detail
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

    // Extract KB ID if present
    let kb_id = detail
        .properties
        .as_ref()
        .and_then(|p| p.get("kb_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract ingest source
    let ingest_source = detail
        .properties
        .as_ref()
        .and_then(|p| p.get("ingest_source"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

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
                    <span class="prop-key"><i class="fa-solid fa-file-import" style="margin-right: 0.25rem;"></i>"Source"</span>
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
                        <span class="prop-key"><i class="fa-solid fa-link" style="margin-right: 0.25rem;"></i>"KB Link"</span>
                        <a href=url.clone() target="_blank" rel="noopener" style="font-size: 0.85rem; color: var(--accent-bright);">
                            {id}
                            " " <i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 0.7rem;"></i>
                        </a>
                    </div>
                }
            })}

            // Properties table
            {if props.is_empty() {
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
                            {props.iter().map(|(k, v)| view! {
                                <div class="prop-row">
                                    <span class="prop-key">{k.clone()}</span>
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
