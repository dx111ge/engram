/// Effect executor: applies rule effects to the graph and external systems.

use std::sync::{Arc, RwLock};

use engram_core::events::GraphEvent;
use engram_core::graph::Graph;

use crate::error::ActionError;
use crate::types::{Effect, EffectPreview};

/// Execute a single effect.
pub fn execute_effect(
    effect: &Effect,
    event: &GraphEvent,
    graph: &Arc<RwLock<Graph>>,
) -> Result<(), ActionError> {
    match effect {
        Effect::ConfidenceCascade {
            rel_types,
            depth,
            decay_factor,
        } => cascade_confidence(event, graph, rel_types, *depth, *decay_factor),

        Effect::CreateEdge {
            from,
            to,
            rel_type,
            confidence,
        } => create_edge(graph, from, to, rel_type, *confidence),

        Effect::TierChange { tier } => change_tier(event, graph, tier),

        Effect::SetProperty { key, value } => set_property(event, graph, key, value),

        Effect::Flag { message } => flag_node(event, graph, message),

        Effect::Webhook { url, headers } => {
            #[cfg(feature = "webhooks")]
            {
                send_webhook(url, headers, event)
            }
            #[cfg(not(feature = "webhooks"))]
            {
                let _ = (url, headers);
                tracing::warn!("webhook effect skipped: webhooks feature not enabled");
                Ok(())
            }
        }

        Effect::Notify { channel, message } => {
            tracing::info!(
                channel = %channel,
                message = %substitute_event_vars(message, event),
                "action notification"
            );
            Ok(())
        }

        Effect::CreateIngestJob {
            query_template,
            source,
            reconcile,
        } => {
            let query = substitute_event_vars(query_template, event);
            tracing::info!(
                query = %query,
                source = %source,
                reconcile = ?reconcile,
                "ingest job created (will be picked up by scheduler)"
            );
            // The actual ingest job is created by the ingest pipeline scheduler.
            // Here we just log the intent. Full integration happens when
            // engram-action is wired to engram-ingest's SourceRegistry.
            Ok(())
        }

        Effect::Log { level, message } => {
            let msg = substitute_event_vars(message, event);
            match level.as_deref().unwrap_or("info") {
                "debug" => tracing::debug!("{}", msg),
                "warn" => tracing::warn!("{}", msg),
                "error" => tracing::error!("{}", msg),
                _ => tracing::info!("{}", msg),
            }
            Ok(())
        }

        Effect::AssessEvaluate { categories } => {
            tracing::info!("assess_evaluate effect: categories={:?}", categories);
            // Actual evaluation handled by the assessment engine subscriber
            Ok(())
        }
    }
}

/// Preview an effect (dry run).
pub fn preview_effect(effect: &Effect, event: &GraphEvent) -> EffectPreview {
    let (effect_type, description) = match effect {
        Effect::ConfidenceCascade { rel_types, depth, .. } => (
            "confidence_cascade",
            format!("cascade via {:?} depth={}", rel_types, depth.unwrap_or(3)),
        ),
        Effect::CreateEdge { from, to, rel_type, .. } => (
            "create_edge",
            format!("{} -[{}]-> {}", substitute_event_vars(from, event), rel_type, substitute_event_vars(to, event)),
        ),
        Effect::TierChange { tier } => (
            "tier_change",
            format!("change tier to {}", tier),
        ),
        Effect::SetProperty { key, value } => (
            "set_property",
            format!("set {} = {}", key, substitute_event_vars(value, event)),
        ),
        Effect::Flag { message } => (
            "flag",
            format!("flag: {}", substitute_event_vars(message, event)),
        ),
        Effect::Webhook { url, .. } => (
            "webhook",
            format!("POST {}", url),
        ),
        Effect::Notify { channel, message } => (
            "notify",
            format!("[{}] {}", channel, substitute_event_vars(message, event)),
        ),
        Effect::CreateIngestJob { query_template, source, .. } => (
            "create_ingest_job",
            format!("query '{}' from {}", substitute_event_vars(query_template, event), source),
        ),
        Effect::Log { message, .. } => (
            "log",
            substitute_event_vars(message, event),
        ),
        Effect::AssessEvaluate { categories } => (
            "assess_evaluate",
            format!("evaluate assessments: {:?}", categories),
        ),
    };

    EffectPreview {
        effect_type: effect_type.to_string(),
        description,
        would_execute: true,
    }
}

// ── Internal effect implementations ──

fn cascade_confidence(
    event: &GraphEvent,
    graph: &Arc<RwLock<Graph>>,
    rel_types: &[String],
    max_depth: Option<u32>,
    decay: Option<f32>,
) -> Result<(), ActionError> {
    let (label, new_confidence) = match event {
        GraphEvent::FactUpdated { label, new_confidence, .. } => (label.to_string(), *new_confidence),
        GraphEvent::FactStored { label, confidence, .. } => (label.to_string(), *confidence),
        _ => return Ok(()), // not applicable
    };

    let decay_factor = decay.unwrap_or(0.8);
    let depth_limit = max_depth.unwrap_or(3);

    let g = graph.read().map_err(|_| ActionError::Graph("lock poisoned".into()))?;
    let mut to_visit: Vec<(String, f32, u32)> = vec![(label, new_confidence, 0)];
    let mut visited = std::collections::HashSet::new();
    let mut updates: Vec<(String, f32)> = Vec::new();

    while let Some((current_label, conf, depth)) = to_visit.pop() {
        if depth >= depth_limit || !visited.insert(current_label.clone()) {
            continue;
        }

        if let Ok(edges) = g.edges_from(&current_label) {
            for edge in edges {
                if rel_types.is_empty() || rel_types.iter().any(|rt| rt == &edge.relationship) {
                    let cascaded = conf * decay_factor;
                    updates.push((edge.to.clone(), cascaded));
                    to_visit.push((edge.to, cascaded, depth + 1));
                }
            }
        }
    }

    drop(g);

    // Apply updates under write lock
    if !updates.is_empty() {
        let mut g = graph.write().map_err(|_| ActionError::Graph("lock poisoned".into()))?;
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "action-cascade".to_string(),
        };
        for (label, confidence) in &updates {
            let _ = g.store_with_confidence(label, *confidence, &prov);
        }
        tracing::debug!(cascaded = updates.len(), "confidence cascade complete");
    }

    Ok(())
}

fn create_edge(
    graph: &Arc<RwLock<Graph>>,
    from: &str,
    to: &str,
    rel_type: &str,
    confidence: Option<f32>,
) -> Result<(), ActionError> {
    let mut g = graph.write().map_err(|_| ActionError::Graph("lock poisoned".into()))?;
    let prov = engram_core::graph::Provenance {
        source_type: engram_core::graph::SourceType::Derived,
        source_id: "action-engine".to_string(),
    };

    if let Some(conf) = confidence {
        g.relate_with_confidence(from, to, rel_type, conf, &prov)
            .map_err(|e| ActionError::Effect(e.to_string()))?;
    } else {
        g.relate(from, to, rel_type, &prov)
            .map_err(|e| ActionError::Effect(e.to_string()))?;
    }

    Ok(())
}

fn change_tier(
    event: &GraphEvent,
    graph: &Arc<RwLock<Graph>>,
    tier: &str,
) -> Result<(), ActionError> {
    let label = match event {
        GraphEvent::FactStored { label, .. } |
        GraphEvent::FactUpdated { label, .. } |
        GraphEvent::ThresholdCrossed { label, .. } => label.to_string(),
        _ => return Ok(()),
    };

    let tier_value = match tier {
        "core" => 0u8,
        "active" => 1,
        "archival" => 2,
        _ => return Err(ActionError::Effect(format!("unknown tier: {}", tier))),
    };

    let mut g = graph.write().map_err(|_| ActionError::Graph("lock poisoned".into()))?;
    g.set_tier(&label, tier_value)
        .map_err(|e| ActionError::Effect(format!("{}", e)))?;

    Ok(())
}

fn set_property(
    event: &GraphEvent,
    graph: &Arc<RwLock<Graph>>,
    key: &str,
    value: &str,
) -> Result<(), ActionError> {
    let label = match event {
        GraphEvent::FactStored { label, .. } |
        GraphEvent::FactUpdated { label, .. } => label.to_string(),
        _ => return Ok(()),
    };

    let resolved_value = substitute_event_vars(value, event);
    let mut g = graph.write().map_err(|_| ActionError::Graph("lock poisoned".into()))?;
    g.set_property(&label, key, &resolved_value)
        .map_err(|e| ActionError::Effect(e.to_string()))?;

    Ok(())
}

fn flag_node(
    event: &GraphEvent,
    graph: &Arc<RwLock<Graph>>,
    message: &str,
) -> Result<(), ActionError> {
    let label = match event {
        GraphEvent::FactStored { label, .. } |
        GraphEvent::FactUpdated { label, .. } |
        GraphEvent::ThresholdCrossed { label, .. } => label.to_string(),
        _ => return Ok(()),
    };

    let resolved = substitute_event_vars(message, event);
    let mut g = graph.write().map_err(|_| ActionError::Graph("lock poisoned".into()))?;
    g.set_property(&label, "flag", &resolved)
        .map_err(|e| ActionError::Effect(e.to_string()))?;

    Ok(())
}

#[cfg(feature = "webhooks")]
fn send_webhook(
    url: &str,
    headers: &std::collections::HashMap<String, String>,
    event: &GraphEvent,
) -> Result<(), ActionError> {
    let client = reqwest::blocking::Client::new();
    let mut req = client.post(url);

    for (key, value) in headers {
        req = req.header(key, value);
    }

    let payload = serde_json::json!({
        "event": format!("{:?}", event),
    });

    req.json(&payload)
        .send()
        .map_err(|e| ActionError::Webhook(e.to_string()))?;

    Ok(())
}

/// Substitute `{entity}`, `{confidence}`, `{node_id}` in a template string.
fn substitute_event_vars(template: &str, event: &GraphEvent) -> String {
    let mut result = template.to_string();

    match event {
        GraphEvent::FactStored {
            node_id,
            label,
            confidence,
            ..
        } => {
            result = result.replace("{entity}", label);
            result = result.replace("{node_id}", &node_id.to_string());
            result = result.replace("{confidence}", &confidence.to_string());
        }
        GraphEvent::FactUpdated {
            node_id,
            label,
            new_confidence,
            old_confidence,
            ..
        } => {
            result = result.replace("{entity}", label);
            result = result.replace("{node_id}", &node_id.to_string());
            result = result.replace("{confidence}", &new_confidence.to_string());
            result = result.replace("{old_confidence}", &old_confidence.to_string());
        }
        _ => {}
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event() -> GraphEvent {
        GraphEvent::FactStored {
            node_id: 42,
            label: Arc::from("Apple Inc"),
            confidence: 0.85,
            source: Arc::from("test"),
            entity_type: Some(Arc::from("ORG")),
        }
    }

    #[test]
    fn substitute_vars_in_template() {
        let event = make_event();
        let result = substitute_event_vars("Found {entity} with confidence {confidence}", &event);
        assert_eq!(result, "Found Apple Inc with confidence 0.85");
    }

    #[test]
    fn log_effect_works() {
        let graph = {
            let dir = tempfile::TempDir::new().unwrap();
            let path = dir.path().join("test.brain");
            let g = Graph::create(&path).unwrap();
            Arc::new(RwLock::new(g))
        };

        let effect = Effect::Log {
            level: Some("info".into()),
            message: "test message for {entity}".into(),
        };

        let event = make_event();
        assert!(execute_effect(&effect, &event, &graph).is_ok());
    }

    #[test]
    fn preview_log_effect() {
        let event = make_event();
        let effect = Effect::Log {
            level: None,
            message: "Stored {entity}".into(),
        };

        let preview = preview_effect(&effect, &event);
        assert_eq!(preview.effect_type, "log");
        assert_eq!(preview.description, "Stored Apple Inc");
        assert!(preview.would_execute);
    }

    #[test]
    fn preview_create_edge() {
        let event = make_event();
        let effect = Effect::CreateEdge {
            from: "{entity}".into(),
            to: "Tech Sector".into(),
            rel_type: "belongs_to".into(),
            confidence: Some(0.9),
        };

        let preview = preview_effect(&effect, &event);
        assert_eq!(preview.effect_type, "create_edge");
        assert!(preview.description.contains("Apple Inc"));
    }

    #[test]
    fn create_edge_effect() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        let prov = engram_core::graph::Provenance::user("test");
        g.store("A", &prov).unwrap();
        g.store("B", &prov).unwrap();
        let graph = Arc::new(RwLock::new(g));

        let effect = Effect::CreateEdge {
            from: "A".into(),
            to: "B".into(),
            rel_type: "related_to".into(),
            confidence: None,
        };

        let event = make_event();
        assert!(execute_effect(&effect, &event, &graph).is_ok());
    }
}
