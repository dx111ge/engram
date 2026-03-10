/// SSE streaming for A2A tasks.
///
/// Large result sets are streamed as Server-Sent Events (SSE) via
/// POST /tasks/sendSubscribe. Each event is a partial result that
/// the client assembles into the full response.
///
/// Format follows the SSE spec:
///   event: <event-type>
///   data: <json-payload>
///
///   (blank line separates events)

/// SSE event types in the A2A protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// Task status changed
    StatusUpdate,
    /// Partial artifact available
    ArtifactChunk,
    /// Task completed with final artifacts
    TaskComplete,
    /// Task failed
    TaskFailed,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::StatusUpdate => "status-update",
            EventType::ArtifactChunk => "artifact-chunk",
            EventType::TaskComplete => "task-complete",
            EventType::TaskFailed => "task-failed",
        }
    }
}

/// A single SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: EventType,
    pub data: serde_json::Value,
    /// Optional event ID for reconnection
    pub id: Option<String>,
}

impl SseEvent {
    pub fn status_update(data: serde_json::Value) -> Self {
        SseEvent {
            event_type: EventType::StatusUpdate,
            data,
            id: None,
        }
    }

    pub fn artifact_chunk(data: serde_json::Value) -> Self {
        SseEvent {
            event_type: EventType::ArtifactChunk,
            data,
            id: None,
        }
    }

    pub fn task_complete(data: serde_json::Value) -> Self {
        SseEvent {
            event_type: EventType::TaskComplete,
            data,
            id: None,
        }
    }

    pub fn task_failed(error: &str) -> Self {
        SseEvent {
            event_type: EventType::TaskFailed,
            data: serde_json::json!({"error": error}),
            id: None,
        }
    }

    /// Format as SSE wire format.
    pub fn to_sse_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("event: {}\n", self.event_type.as_str()));
        if let Some(ref id) = self.id {
            s.push_str(&format!("id: {id}\n"));
        }
        let data_str = serde_json::to_string(&self.data).unwrap_or_default();
        // SSE data lines: split on newlines
        for line in data_str.lines() {
            s.push_str(&format!("data: {line}\n"));
        }
        s.push('\n'); // blank line terminates event
        s
    }
}

/// Build an SSE stream from a list of results, chunked into events.
pub fn stream_results(
    task_id: &str,
    results: &[serde_json::Value],
    chunk_size: usize,
) -> Vec<SseEvent> {
    let mut events = Vec::new();

    // Status: working
    events.push(SseEvent::status_update(serde_json::json!({
        "taskId": task_id,
        "state": "working",
        "total": results.len(),
    })));

    // Artifact chunks
    for (i, chunk) in results.chunks(chunk_size).enumerate() {
        events.push(SseEvent {
            event_type: EventType::ArtifactChunk,
            data: serde_json::json!({
                "taskId": task_id,
                "chunkIndex": i,
                "items": chunk,
            }),
            id: Some(format!("{task_id}-{i}")),
        });
    }

    // Complete
    events.push(SseEvent::task_complete(serde_json::json!({
        "taskId": task_id,
        "state": "completed",
        "totalChunks": (results.len() + chunk_size - 1) / chunk_size.max(1),
    })));

    events
}

/// Execute a task with streaming SSE output for long-running operations.
///
/// Returns a stream of SSE events: status-update (working), artifact-chunk(s),
/// then task-complete or task-failed. This wraps the synchronous skill handler
/// with streaming event framing.
pub fn stream_task(
    request: &crate::task::TaskRequest,
    graph: &std::sync::Arc<std::sync::RwLock<engram_core::graph::Graph>>,
) -> Vec<SseEvent> {
    let task_id = request.id.as_deref().unwrap_or("stream-task");

    // Emit "working" status
    let mut events = vec![SseEvent::status_update(serde_json::json!({
        "taskId": task_id,
        "state": "working",
        "skill": request.skill_id,
    }))];

    // Execute the skill synchronously
    let response = crate::skill::route_task(request, graph);

    match response.status.state {
        crate::task::TaskState::Completed => {
            // Stream artifacts as chunks
            if let Some(artifacts) = &response.artifacts {
                for (i, artifact) in artifacts.iter().enumerate() {
                    events.push(SseEvent {
                        event_type: EventType::ArtifactChunk,
                        data: serde_json::json!({
                            "taskId": task_id,
                            "chunkIndex": i,
                            "artifact": artifact,
                        }),
                        id: Some(format!("{task_id}-artifact-{i}")),
                    });
                }
            }
            events.push(SseEvent::task_complete(serde_json::json!({
                "taskId": task_id,
                "state": "completed",
                "artifactCount": response.artifacts.as_ref().map(|a| a.len()).unwrap_or(0),
            })));
        }
        crate::task::TaskState::Failed => {
            let error_msg = response.status.message
                .as_ref()
                .map(|m| m.text())
                .unwrap_or_else(|| "unknown error".to_string());
            events.push(SseEvent::task_failed(&error_msg));
        }
        _ => {
            // For other states (working, etc.), just emit status
            events.push(SseEvent::status_update(serde_json::json!({
                "taskId": task_id,
                "state": format!("{:?}", response.status.state).to_lowercase(),
            })));
        }
    }

    events
}

/// Format a complete stream_task result as an SSE byte string.
pub fn format_sse_stream(events: &[SseEvent]) -> String {
    events.iter().map(|e| e.to_sse_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_format() {
        let event = SseEvent::status_update(serde_json::json!({"state": "working"}));
        let sse = event.to_sse_string();
        assert!(sse.starts_with("event: status-update\n"));
        assert!(sse.contains("data: "));
        assert!(sse.ends_with("\n\n"));
    }

    #[test]
    fn sse_with_id() {
        let event = SseEvent {
            event_type: EventType::ArtifactChunk,
            data: serde_json::json!({"chunk": 1}),
            id: Some("evt-1".to_string()),
        };
        let sse = event.to_sse_string();
        assert!(sse.contains("id: evt-1\n"));
    }

    #[test]
    fn stream_results_chunked() {
        let results: Vec<serde_json::Value> = (0..10)
            .map(|i| serde_json::json!({"item": i}))
            .collect();
        let events = stream_results("t1", &results, 3);
        // 1 status + 4 chunks (ceil(10/3)) + 1 complete = 6
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].event_type, EventType::StatusUpdate);
        assert_eq!(events[1].event_type, EventType::ArtifactChunk);
        assert_eq!(events[5].event_type, EventType::TaskComplete);
    }

    #[test]
    fn stream_empty_results() {
        let events = stream_results("t2", &[], 5);
        // 1 status + 0 chunks + 1 complete = 2
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn task_failed_event() {
        let event = SseEvent::task_failed("something went wrong");
        assert_eq!(event.event_type, EventType::TaskFailed);
        assert_eq!(event.data["error"], "something went wrong");
    }

    #[test]
    fn stream_task_completed() {
        use std::sync::{Arc, RwLock};
        use engram_core::graph::{Graph, Provenance};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain");
        let mut g = Graph::create(&path).unwrap();
        g.store("Rust", &Provenance::user("test")).unwrap();
        let graph = Arc::new(RwLock::new(g));

        let request = crate::task::TaskRequest {
            id: Some("stream-1".to_string()),
            skill_id: "query-knowledge".to_string(),
            message: crate::task::TaskMessage::user_text("What is Rust?"),
            metadata: None,
            push_notification_url: None,
        };

        let events = stream_task(&request, &graph);
        // Should have: working status, artifact chunk(s), task-complete
        assert!(events.len() >= 2);
        assert_eq!(events[0].event_type, EventType::StatusUpdate);
        assert_eq!(events.last().unwrap().event_type, EventType::TaskComplete);
    }

    #[test]
    fn stream_task_failed() {
        use std::sync::{Arc, RwLock};
        use engram_core::graph::Graph;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain");
        let g = Graph::create(&path).unwrap();
        let graph = Arc::new(RwLock::new(g));

        let request = crate::task::TaskRequest {
            id: Some("stream-2".to_string()),
            skill_id: "nonexistent-skill".to_string(),
            message: crate::task::TaskMessage::user_text("test"),
            metadata: None,
            push_notification_url: None,
        };

        let events = stream_task(&request, &graph);
        assert!(events.len() >= 2);
        assert_eq!(events[0].event_type, EventType::StatusUpdate);
        assert_eq!(events.last().unwrap().event_type, EventType::TaskFailed);
    }

    #[test]
    fn format_sse_stream_output() {
        let events = vec![
            SseEvent::status_update(serde_json::json!({"state": "working"})),
            SseEvent::task_complete(serde_json::json!({"state": "done"})),
        ];
        let output = format_sse_stream(&events);
        assert!(output.contains("event: status-update"));
        assert!(output.contains("event: task-complete"));
    }
}
