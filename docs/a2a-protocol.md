# Engram A2A Protocol

Google's Agent-to-Agent (A2A) protocol implementation. Any A2A-compatible agent (ChatGPT, Claude, Gemini, custom agents) can discover engram and use it as a knowledge service -- no custom integration needed.

A2A is to AI agents what HTTP is to web servers: a standard protocol for interoperability.

## Concepts

| Concept | Description |
|---------|-------------|
| **Agent Card** | JSON at `/.well-known/agent.json` describing what engram can do |
| **Task** | A unit of work one agent asks another to perform |
| **Message** | Communication within a task (text, data, files) |
| **Artifact** | Structured output from a completed task |
| **Streaming** | Server-Sent Events for long-running tasks |
| **Push Notify** | Webhook callbacks for async completion |

## Agent Card

Served at `GET /.well-known/agent.json`. This is how other agents discover engram.

```rust
use engram_a2a::card::AgentCard;

let card = AgentCard::engram_default("http://localhost:3030");
println!("{}", card.to_json());
```

```json
{
  "name": "engram",
  "description": "High-performance AI memory engine...",
  "url": "http://localhost:3030",
  "version": "1.1.0",
  "protocolVersion": "0.2",
  "capabilities": {
    "streaming": true,
    "pushNotifications": true,
    "stateTransitionHistory": true
  },
  "skills": [ ... ],
  "authentication": {
    "schemes": ["bearer"]
  },
  "defaultInputModes": ["text/plain", "application/json"],
  "defaultOutputModes": ["application/json"]
}
```

## Skills

Engram exposes 9 skills via A2A:

### store-knowledge

Store facts, entities, and relationships with confidence scoring and provenance tracking.

```json
{
  "skillId": "store-knowledge",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "Remember that server-01 runs PostgreSQL 15" }]
  }
}
```

Structured input also supported:

```json
{
  "skillId": "store-knowledge",
  "message": {
    "role": "user",
    "parts": [{ "type": "data", "data": { "entity": "PostgreSQL", "confidence": 0.9 } }]
  }
}
```

### query-knowledge

Query the knowledge graph with natural language.

```json
{
  "skillId": "query-knowledge",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "What do we know about server-01?" }]
  }
}
```

### reason

Logical inference, proof, and relationship discovery.

```json
{
  "skillId": "reason",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "Is PostgreSQL related to server-01?" }]
  }
}
```

Recognizes patterns: "X related to Y", "X connected to Y", "X linked to Y". Falls back to search for other queries.

### learn

Reinforce, correct, or decay knowledge.

| Prefix | Action | Example |
|--------|--------|---------|
| `Confirm ...` / `Reinforce ...` | Boost confidence | "Confirm server-01 is healthy" |
| `Correct ...` / `... was wrong` | Penalize + propagate | "Correct the server IP" |
| `Forget ...` / `Decay ...` | Apply decay | "Forget old API endpoints" |
| (other) | Store via NL parser | "The deploy succeeded" |

### explain

Explain provenance, confidence, edges, and co-occurrences for an entity.

```json
{
  "skillId": "explain",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "How do we know about server-01?" }]
  }
}
```

Returns confidence, properties, incoming/outgoing edges, and co-occurrence data.

### analyze-gaps

Detect knowledge gaps and black areas in the graph. Returns gaps ranked by severity with affected node counts and descriptions.

```json
{
  "skillId": "analyze-gaps",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "What knowledge gaps exist in the security domain?" }]
  }
}
```

Supports the keyword "critical" to filter for higher severity thresholds only:

```json
{
  "skillId": "analyze-gaps",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "Show critical gaps in our infrastructure knowledge" }]
  }
}
```

Returns gaps with severity, affected node count, and suggested remediation queries.

### federated-search

Search across the local graph with mesh-aware ACL filtering. Results include peer attribution showing which mesh node contributed each fact.

```json
{
  "skillId": "federated-search",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "What do mesh peers know about CVE-2026-1234?" }]
  }
}
```

Structured input for fine-grained control:

```json
{
  "skillId": "federated-search",
  "message": {
    "role": "user",
    "parts": [{
      "type": "data",
      "data": {
        "query": "CVE-2026-1234",
        "min_confidence": 0.3,
        "max_hops": 2,
        "clearance": "internal"
      }
    }]
  }
}
```

Returns facts with confidence, source peer ID, and hop count.

### suggest-investigations

Analyze knowledge gaps and generate investigation suggestions. Returns mechanical query suggestions per gap (always available) and indicates whether LLM-powered suggestions are available (requires configured LLM endpoint).

```json
{
  "skillId": "suggest-investigations",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "How should we investigate the networking gaps?" }]
  }
}
```

Response includes:
- `mechanical_suggestions` -- concrete queries that can be run against sources (GDELT, RSS, mesh peers)
- `llm_available` -- boolean indicating if LLM-powered suggestions can be generated
- `llm_suggestions` -- deeper investigation plans (only present when LLM is configured and reachable)

### assess-knowledge

Create, evaluate, and track hypotheses with evidence-based probability scoring. Assessments watch graph entities and auto-update when new information arrives.

```json
{
  "skillId": "assess-knowledge",
  "message": {
    "role": "user",
    "parts": [{ "type": "text", "text": "Create assessment: NVIDIA stock > $200 by Q3 2026, watch NVIDIA and GPU market" }]
  }
}
```

Structured input for creating assessments:

```json
{
  "skillId": "assess-knowledge",
  "message": {
    "role": "user",
    "parts": [{
      "type": "data",
      "data": {
        "action": "create",
        "title": "NVIDIA stock > $200 by Q3 2026",
        "category": "financial",
        "watches": ["NVIDIA", "GPU market"],
        "initial_probability": 0.50
      }
    }]
  }
}
```

Supported actions via text patterns:
| Pattern | Action |
|---------|--------|
| `Create assessment: ...` | Create new assessment with watches |
| `Evaluate ...` / `Re-evaluate ...` | Trigger manual re-evaluation |
| `Add evidence ...` | Add supporting/contradicting evidence |
| `List assessments` | List all active assessments |
| `Assessment status ...` | Get detail for specific assessment |

Returns probability, shift delta, evidence counts, and score history for evaluation requests.

## Streaming Task Support

Long-running tasks (large graph scans, federated queries, enrichment) can be streamed via Server-Sent Events using `stream_task` and `format_sse_stream`.

```rust
use engram_a2a::skill::stream_task;
use engram_a2a::streaming::format_sse_stream;

// Stream a long-running task
let events = stream_task(&request, &graph);
for event in &events {
    print!("{}", format_sse_stream(event));
}
```

The streaming protocol sends incremental `artifact-chunk` events as results become available, followed by a `task-complete` event. Clients can process partial results immediately rather than waiting for the full response. This is particularly useful for:

- `analyze-gaps` on large graphs (thousands of nodes)
- `federated-search` where peer responses arrive at different times
- `suggest-investigations` where mechanical suggestions arrive before LLM suggestions

## Task Lifecycle

Tasks go through state transitions:

```
submitted -> working -> completed
                    \-> failed
                    \-> canceled
                    \-> inputRequired
```

### Sending a Task

```rust
use engram_a2a::task::{TaskRequest, TaskMessage};

let request = TaskRequest {
    id: Some("task-001".to_string()),
    skill_id: "query-knowledge".to_string(),
    message: TaskMessage::user_text("What caused the outage?"),
    metadata: None,
    push_notification_url: None,
};
```

### Task Response

```json
{
  "id": "task-001",
  "status": {
    "state": "completed",
    "timestamp": 1741340400000
  },
  "artifacts": [{
    "type": "application/json",
    "data": {
      "action": "query",
      "query": "What caused the outage?",
      "result": {
        "interpretation": "search for: outage",
        "results": [
          { "label": "outage-march-5", "confidence": 0.87 }
        ]
      }
    }
  }]
}
```

### Skill Routing

```rust
use engram_a2a::skill::route_task;

let response = route_task(&request, &graph);
// response.status.state == TaskState::Completed
// response.artifacts contains the results
```

## SSE Streaming

For large result sets, use `POST /tasks/sendSubscribe`. Results are streamed as Server-Sent Events.

### Event Types

| Event | Description |
|-------|-------------|
| `status-update` | Task state changed (e.g., working) |
| `artifact-chunk` | Partial results available |
| `task-complete` | Task finished, final status |
| `task-failed` | Task errored |

### Wire Format

```
event: status-update
data: {"taskId":"t1","state":"working","total":50}

event: artifact-chunk
id: t1-0
data: {"taskId":"t1","chunkIndex":0,"items":[...]}

event: artifact-chunk
id: t1-1
data: {"taskId":"t1","chunkIndex":1,"items":[...]}

event: task-complete
data: {"taskId":"t1","state":"completed","totalChunks":2}
```

### Building a Stream

```rust
use engram_a2a::streaming::stream_results;

let results: Vec<serde_json::Value> = vec![/* ... */];
let events = stream_results("task-001", &results, 10); // chunk size = 10

for event in &events {
    print!("{}", event.to_sse_string());
}
```

## Agent Discovery

Find and track other A2A-compatible agents in the network.

```rust
use engram_a2a::discovery::{AgentRegistry, DiscoveredAgent};
use engram_a2a::card::AgentCard;

let mut registry = AgentRegistry::new();

// Register a discovered agent
registry.register(DiscoveredAgent {
    url: "http://monitoring-agent:3030".to_string(),
    card: AgentCard::engram_default("http://monitoring-agent:3030"),
    last_fetched: 1741340400000,
    reachable: true,
});

// Find agents with specific skills
let knowledge_agents = registry.find_by_skill("query-knowledge");
let memory_agents = registry.find_by_tag("memory");

// Build discovery URL
let url = AgentRegistry::well_known_url("http://some-agent:3030");
// -> "http://some-agent:3030/.well-known/agent.json"

// Persist registry
registry.save(Path::new("agents.json"))?;
```

## Push Notifications

For async tasks, the caller provides a webhook URL. Engram POSTs the result when the task completes.

```rust
use engram_a2a::notification::{NotificationQueue, should_notify};
use engram_a2a::task::TaskResponse;

let mut queue = NotificationQueue::new(3); // max 3 retries

// Enqueue a notification
queue.enqueue(
    "http://caller.example.com/webhook".to_string(),
    TaskResponse::completed("task-001", vec![/* artifacts */]),
);

// Deliver pending notifications
for notification in queue.drain_pending() {
    let payload = NotificationQueue::build_payload(&notification);
    // POST payload to notification.url
    // On failure:
    // queue.report_failure(notification);
}

// Retry failed deliveries
for retry in queue.drain_retries() {
    // Try again...
}
```

Only completed and failed tasks trigger notifications:

```rust
let working = TaskResponse::working("t1", "in progress");
assert!(!should_notify(&working));

let done = TaskResponse::completed("t1", vec![]);
assert!(should_notify(&done));
```

## Multi-Agent Collaboration

A typical multi-agent workflow using engram as a knowledge backend:

```
User -> Orchestrator: "Why is the payment service slow?"

Orchestrator:
  |
  +-> engram (A2A, query-knowledge):
  |   "What do we know about payment service?"
  |   -> architecture, dependencies, past incidents
  |
  +-> monitoring-agent (A2A):
  |   "Current metrics for payment service?"
  |   -> CPU 95%, DB latency 500ms
  |
  +-> engram (A2A, store-knowledge):
  |   "Payment service DB latency is 500ms"
  |   -> stored with confidence 0.95
  |
  +-> engram (A2A, reason):
  |   "What caused high DB latency in the past?"
  |   -> 3 previous incidents, all missing index after migration
  |
  +-> Orchestrator -> User:
      "DB latency (500ms) likely caused by missing index.
       Confidence: 82%. Based on 3 previous incidents."
```

## Module Reference

| Module | Description |
|--------|-------------|
| `engram_a2a::card` | Agent Card definition and default builder |
| `engram_a2a::task` | Task request/response, state machine, message parts |
| `engram_a2a::skill` | Skill routing to graph operations |
| `engram_a2a::streaming` | SSE event types and chunked streaming |
| `engram_a2a::discovery` | Agent registry and well-known URL builder |
| `engram_a2a::notification` | Push notification queue with retry |

## Endpoints Summary

| Method | Path | Description |
|--------|------|-------------|
| GET | `/.well-known/agent.json` | Agent Card discovery |
| POST | `/tasks/send` | Send a task (synchronous) |
| POST | `/tasks/sendSubscribe` | Send a task (SSE streaming) |
| POST | `/tasks/{id}/cancel` | Cancel a running task |
| GET | `/tasks/{id}` | Get task status |

## Protocol Version

Engram implements A2A protocol version **0.2**. All JSON payloads use camelCase field names per the spec.
