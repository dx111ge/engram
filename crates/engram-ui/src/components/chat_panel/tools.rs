//! Tool execution: map tool names to engram API calls.

use crate::api::ApiClient;

/// Execute a tool call by mapping tool name to the appropriate API endpoint.
pub async fn execute_tool(api: &ApiClient, name: &str, args: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(args).unwrap_or_default();

    let result = match name {
        "engram_store" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "node_type": parsed.get("type").and_then(|v| v.as_str()),
                "source": parsed.get("source").and_then(|v| v.as_str()),
                "confidence": parsed.get("confidence").and_then(|v| v.as_f64()),
                "properties": parsed.get("properties"),
            });
            api.post_text("/store", &body).await
        }
        "engram_relate" => {
            let body = serde_json::json!({
                "from": parsed.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                "to": parsed.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                "relationship": parsed.get("relationship").and_then(|v| v.as_str()).unwrap_or(""),
                "confidence": parsed.get("confidence").and_then(|v| v.as_f64()),
                "valid_from": parsed.get("valid_from").and_then(|v| v.as_str()),
                "valid_to": parsed.get("valid_to").and_then(|v| v.as_str()),
            });
            api.post_text("/relate", &body).await
        }
        "engram_query" => {
            let body = serde_json::json!({
                "start": parsed.get("start").and_then(|v| v.as_str()).unwrap_or(""),
                "depth": parsed.get("depth").and_then(|v| v.as_u64()),
                "direction": parsed.get("direction").and_then(|v| v.as_str()),
                "min_confidence": parsed.get("min_confidence").and_then(|v| v.as_f64()),
            });
            api.post_text("/query", &body).await
        }
        "engram_search" => {
            let body = serde_json::json!({
                "query": parsed.get("query").and_then(|v| v.as_str()).unwrap_or(""),
                "limit": parsed.get("limit").and_then(|v| v.as_u64()),
            });
            api.post_text("/search", &body).await
        }
        "engram_similar" => {
            let body = serde_json::json!({
                "text": parsed.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                "limit": parsed.get("limit").and_then(|v| v.as_u64()),
            });
            api.post_text("/similar", &body).await
        }
        "engram_explain" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(entity);
            api.get_text(&format!("/explain/{}", encoded.as_string().unwrap_or_default()))
                .await
        }
        "engram_gaps" => {
            let min_sev = parsed.get("min_severity").and_then(|v| v.as_f64());
            let limit = parsed.get("limit").and_then(|v| v.as_u64());
            let domain = parsed.get("domain").and_then(|v| v.as_str());
            let mut path = "/reason/gaps".to_string();
            let mut params = Vec::new();
            if let Some(s) = min_sev {
                params.push(format!("min_severity={}", s));
            }
            if let Some(l) = limit {
                params.push(format!("limit={}", l));
            }
            if let Some(d) = domain {
                params.push(format!("domain={}", d));
            }
            if !params.is_empty() {
                path.push('?');
                path.push_str(&params.join("&"));
            }
            api.get_text(&path).await
        }
        "engram_reinforce" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "source": parsed.get("source").and_then(|v| v.as_str()),
            });
            api.post_text("/reinforce", &body).await
        }
        "engram_correct" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "reason": parsed.get("reason").and_then(|v| v.as_str()).unwrap_or(""),
            });
            api.post_text("/learn/correct", &body).await
        }
        "engram_prove" => {
            let body = serde_json::json!({
                "from": parsed.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                "relationship": parsed.get("relationship").and_then(|v| v.as_str()).unwrap_or(""),
                "to": parsed.get("to").and_then(|v| v.as_str()).unwrap_or(""),
            });
            api.post_text("/learn/derive", &body).await
        }
        "engram_delete" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(entity);
            api.delete(&format!("/node/{}", encoded.as_string().unwrap_or_default()))
                .await
        }
        // -- Temporal tools --
        "engram_temporal_query" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "from_date": parsed.get("from_date").and_then(|v| v.as_str()),
                "to_date": parsed.get("to_date").and_then(|v| v.as_str()),
                "relationship": parsed.get("relationship").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/temporal_query", &body).await
        }
        "engram_timeline" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "limit": parsed.get("limit").and_then(|v| v.as_u64()),
            });
            api.post_text("/chat/timeline", &body).await
        }
        "engram_current_state" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "depth": parsed.get("depth").and_then(|v| v.as_u64()),
            });
            api.post_text("/chat/current_state", &body).await
        }
        // -- Compare & Analytics tools --
        "engram_compare" => {
            let body = serde_json::json!({
                "entity_a": parsed.get("entity_a").and_then(|v| v.as_str()).unwrap_or(""),
                "entity_b": parsed.get("entity_b").and_then(|v| v.as_str()).unwrap_or(""),
                "aspects": parsed.get("aspects"),
            });
            api.post_text("/chat/compare", &body).await
        }
        "engram_shortest_path" => {
            let body = serde_json::json!({
                "from": parsed.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                "to": parsed.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                "max_depth": parsed.get("max_depth").and_then(|v| v.as_u64()),
            });
            api.post_text("/chat/shortest_path", &body).await
        }
        "engram_most_connected" => {
            let body = serde_json::json!({
                "limit": parsed.get("limit").and_then(|v| v.as_u64()),
                "node_type": parsed.get("node_type").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/most_connected", &body).await
        }
        "engram_isolated" => {
            let body = serde_json::json!({
                "max_edges": parsed.get("max_edges").and_then(|v| v.as_u64()),
                "node_type": parsed.get("node_type").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/isolated", &body).await
        }
        // -- Ingest & Investigation tools --
        "engram_ingest_text" => {
            let body = serde_json::json!({
                "items": [parsed.get("text").and_then(|v| v.as_str()).unwrap_or("")],
                "source": parsed.get("source").and_then(|v| v.as_str()),
            });
            api.post_text("/ingest", &body).await
        }
        "engram_changes" => {
            let body = serde_json::json!({
                "since": parsed.get("since").and_then(|v| v.as_str()).unwrap_or(""),
                "entity": parsed.get("entity").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/changes", &body).await
        }
        // -- Assessment tools (existing endpoints) --
        "engram_assess_create" => {
            let body = serde_json::json!({
                "title": parsed.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                "category": parsed.get("category").and_then(|v| v.as_str()),
                "timeframe": parsed.get("timeframe").and_then(|v| v.as_str()),
                "initial_probability": parsed.get("probability").and_then(|v| v.as_f64()),
                "watches": parsed.get("watches"),
            });
            api.post_text("/assessments", &body).await
        }
        "engram_assess_query" => {
            let label = parsed.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(label);
            api.get_text(&format!("/assessments/{}", encoded.as_string().unwrap_or_default()))
                .await
        }
        "engram_assess_evidence" => {
            let label = parsed.get("assessment").and_then(|v| v.as_str())
                .or_else(|| parsed.get("label").and_then(|v| v.as_str()))
                .unwrap_or("");
            let encoded = js_sys::encode_uri_component(label);
            let body = serde_json::json!({
                "node_label": parsed.get("entity").and_then(|v| v.as_str())
                    .or_else(|| parsed.get("node_label").and_then(|v| v.as_str())).unwrap_or(""),
                "direction": parsed.get("direction").and_then(|v| v.as_str()).unwrap_or("supports"),
            });
            api.post_text(
                &format!("/assessments/{}/evidence", encoded.as_string().unwrap_or_default()),
                &body,
            ).await
        }
        // -- What-if & Influence --
        "engram_what_if" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "new_confidence": parsed.get("new_confidence").and_then(|v| v.as_f64()),
                "depth": parsed.get("depth").and_then(|v| v.as_u64()),
            });
            api.post_text("/chat/what_if", &body).await
        }
        "engram_influence_path" => {
            let body = serde_json::json!({
                "from": parsed.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                "to": parsed.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                "max_depth": parsed.get("max_depth").and_then(|v| v.as_u64()),
            });
            api.post_text("/chat/influence_path", &body).await
        }
        // -- Action Engine tools --
        "engram_rule_create" => {
            let body = serde_json::json!({
                "rules": [serde_json::json!({
                    "name": parsed.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "trigger": parsed.get("trigger"),
                    "conditions": parsed.get("conditions"),
                    "actions": parsed.get("actions"),
                }).to_string()],
            });
            api.post_text("/actions/rules", &body).await
        }
        "engram_rule_list" => {
            api.get_text("/actions/rules").await
        }
        "engram_run_inference" => {
            api.post_text("/learn/derive", &serde_json::json!({})).await
        }
        // -- Reporting tools --
        "engram_briefing" => {
            let body = serde_json::json!({
                "topic": parsed.get("topic").and_then(|v| v.as_str()).unwrap_or(""),
                "depth": parsed.get("depth").and_then(|v| v.as_str()),
                "format": parsed.get("format").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/briefing", &body).await
        }
        "engram_export_subgraph" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "depth": parsed.get("depth").and_then(|v| v.as_u64()),
                "format": parsed.get("format").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/export_subgraph", &body).await
        }
        "engram_entity_timeline" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "from_date": parsed.get("from_date").and_then(|v| v.as_str()),
                "to_date": parsed.get("to_date").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/entity_timeline", &body).await
        }
        // -- Source Management --
        "engram_sources_list" => {
            api.get_text("/sources").await
        }
        "engram_source_trigger" => {
            let name = parsed.get("source_name").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(name);
            api.post_text(
                &format!("/sources/{}/trigger", encoded.as_string().unwrap_or_default()),
                &serde_json::json!({}),
            ).await
        }
        // -- Existing assessment tools --
        "engram_assess_list" => {
            api.get_text("/assessments").await
        }
        "engram_assess_get" => {
            let label = parsed.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(label);
            api.get_text(&format!("/assessments/{}", encoded.as_string().unwrap_or_default()))
                .await
        }
        "engram_assess_evaluate" => {
            let label = parsed.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(label);
            api.post_text(
                &format!("/assessments/{}/evaluate", encoded.as_string().unwrap_or_default()),
                &serde_json::json!({}),
            ).await
        }
        "engram_assess_watch" => {
            let label = parsed.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(label);
            let body = serde_json::json!({
                "entity_label": parsed.get("entity_label").and_then(|v| v.as_str()).unwrap_or(""),
            });
            api.post_text(
                &format!("/assessments/{}/watch", encoded.as_string().unwrap_or_default()),
                &body,
            ).await
        }
        "engram_investigate" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("");
            let _depth = parsed.get("depth").and_then(|v| v.as_str()).unwrap_or("shallow");
            let encoded = js_sys::encode_uri_component(entity);
            // Step 1: web search
            let search_result = api.get_text(&format!(
                "/proxy/search?q={}", encoded.as_string().unwrap_or_default()
            )).await;
            let search_text = match search_result {
                Ok(t) => t,
                Err(e) => return format!("{{\"error\": \"web search failed: {e}\"}}"),
            };
            // Step 2: ingest search results through NER pipeline
            let body = serde_json::json!({
                "items": [search_text],
                "source": format!("investigation:{}", entity),
            });
            let ingest_part = match api.post_text("/ingest", &body).await {
                Ok(r) => format!("\"ingest\": {r}"),
                Err(e) => format!("\"ingest_error\": \"{e}\""),
            };
            Ok(format!("{{\"search_results\": {search_text}, {ingest_part}}}"))
        }
        "engram_watch" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
            });
            api.post_text("/chat/watch", &body).await
        }
        "engram_schedule" => {
            let body = serde_json::json!({
                "action": parsed.get("action").and_then(|v| v.as_str()).unwrap_or("list"),
                "entity": parsed.get("entity").and_then(|v| v.as_str()),
                "interval": parsed.get("interval").and_then(|v| v.as_str()),
            });
            api.post_text("/chat/schedule", &body).await
        }
        "engram_source_coverage" => {
            let source = parsed.get("source_name").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = js_sys::encode_uri_component(source);
            api.get_text(&format!("/sources/{}/usage", encoded.as_string().unwrap_or_default()))
                .await
        }
        "engram_analyze_relations" => {
            let body = serde_json::json!({
                "text": parsed.get("text").and_then(|v| v.as_str()).unwrap_or(""),
            });
            api.post_text("/ingest/analyze", &body).await
        }
        "engram_sources" | "engram_frontier" => {
            let endpoint = name.strip_prefix("engram_").unwrap_or(name);
            api.get_text(&format!("/reason/{endpoint}")).await
        }
        // Generic fallback
        other => {
            let endpoint = other.strip_prefix("engram_").unwrap_or(other);
            api.post_text(&format!("/{endpoint}"), &parsed).await
        }
    };

    match result {
        Ok(text) => text,
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    }
}
