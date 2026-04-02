/// gRPC service layer for engram.
///
/// With `--features grpc`: real protobuf binary gRPC via tonic, generated from
/// `proto/engram.proto`. Supports standard gRPC clients (grpcurl, BloomRPC, etc).
///
/// Without `--features grpc`: JSON-over-HTTP/2 fallback using the same path
/// conventions (`/engram.EngramService/Method`), compatible with REST tooling.

// ── Feature: grpc (real tonic service) ──

#[cfg(feature = "grpc")]
pub mod proto {
    tonic::include_proto!("engram");
}

#[cfg(feature = "grpc")]
mod service {
    use super::proto;
    use super::proto::engram_service_server::EngramService;
    use crate::state::AppState;
    use engram_core::graph::Provenance;
    use tonic::{Request, Response, Status};

    pub struct EngramGrpc {
        pub state: AppState,
    }

    fn provenance(source: &str) -> Provenance {
        if source.is_empty() {
            Provenance::user("grpc")
        } else {
            Provenance::user(source)
        }
    }

    fn internal(e: impl std::fmt::Display) -> Status {
        Status::internal(e.to_string())
    }

    fn not_found(msg: impl Into<String>) -> Status {
        Status::not_found(msg)
    }

    #[tonic::async_trait]
    impl EngramService for EngramGrpc {
        async fn store(
            &self,
            request: Request<proto::StoreRequest>,
        ) -> Result<Response<proto::StoreResponse>, Status> {
            let req = request.into_inner();
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
            let prov = provenance(&req.source);

            let slot = if req.confidence > 0.0 {
                g.store_with_confidence(&req.entity, req.confidence, &prov)
            } else {
                g.store(&req.entity, &prov)
            }
            .map_err(internal)?;

            if !req.entity_type.is_empty() {
                let _ = g.set_node_type(&req.entity, &req.entity_type);
            }
            for (k, v) in &req.properties {
                let _ = g.set_property(&req.entity, k, v);
            }

            let confidence = g.get_node(&req.entity).ok().flatten()
                .map(|n| n.confidence).unwrap_or(0.0);

            drop(g);
            self.state.mark_dirty();
            self.state.fire_rules_async();

            Ok(Response::new(proto::StoreResponse {
                node_id: slot,
                label: req.entity,
                confidence,
            }))
        }

        async fn relate(
            &self,
            request: Request<proto::RelateRequest>,
        ) -> Result<Response<proto::RelateResponse>, Status> {
            let req = request.into_inner();
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
            let prov = provenance("");

            let edge_slot = if req.confidence > 0.0 {
                g.relate_with_confidence(&req.from, &req.to, &req.relationship, req.confidence, &prov)
            } else {
                g.relate(&req.from, &req.to, &req.relationship, &prov)
            }
            .map_err(internal)?;

            drop(g);
            self.state.mark_dirty();
            self.state.fire_rules_async();

            Ok(Response::new(proto::RelateResponse {
                from: req.from,
                to: req.to,
                relationship: req.relationship,
                edge_slot,
            }))
        }

        async fn query(
            &self,
            request: Request<proto::QueryRequest>,
        ) -> Result<Response<proto::QueryResponse>, Status> {
            let req = request.into_inner();
            let g = self.state.graph.read().map_err(|_| internal("lock"))?;

            let depth = if req.depth > 0 { req.depth } else { 2 };
            let min_conf = req.min_confidence;
            let direction = if req.direction.is_empty() { "both" } else { &req.direction };

            let result = g.traverse_directed(&req.start, depth, min_conf, direction).map_err(|e| not_found(e.to_string()))?;

            let mut nodes = Vec::new();
            for &nid in &result.nodes {
                if let Ok(Some(node)) = g.get_node_by_id(nid) {
                    nodes.push(proto::NodeHit {
                        node_id: nid,
                        label: g.label_for_id(nid).unwrap_or_else(|_| node.label().to_string()),
                        confidence: node.confidence,
                        score: 0.0,
                        depth: result.depths.get(&nid).copied().unwrap_or(0),
                    });
                }
            }

            let edges = result.edges.iter()
                .filter_map(|&(_from_id, _to_id, edge_slot)| {
                    let ev = g.read_edge_view(edge_slot).ok()?;
                    Some(proto::EdgeHit {
                        from: ev.from,
                        to: ev.to,
                        relationship: ev.relationship,
                        confidence: ev.confidence,
                    })
                })
                .collect();

            Ok(Response::new(proto::QueryResponse { nodes, edges }))
        }

        async fn search(
            &self,
            request: Request<proto::SearchRequest>,
        ) -> Result<Response<proto::SearchResponse>, Status> {
            let req = request.into_inner();
            let g = self.state.graph.read().map_err(|_| internal("lock"))?;
            let limit = if req.limit > 0 { req.limit as usize } else { 10 };

            let results = g.search(&req.query, limit).map_err(|e| internal(e))?;
            let total = results.len() as u32;

            let hits = results.into_iter().map(|r| proto::NodeHit {
                node_id: r.node_id,
                label: r.label,
                confidence: r.confidence,
                score: r.score,
                depth: 0,
            }).collect();

            Ok(Response::new(proto::SearchResponse { results: hits, total }))
        }

        async fn get_node(
            &self,
            request: Request<proto::GetNodeRequest>,
        ) -> Result<Response<proto::NodeResponse>, Status> {
            let req = request.into_inner();
            let g = self.state.graph.read().map_err(|_| internal("lock"))?;

            let node = g.get_node(&req.label).map_err(internal)?
                .ok_or_else(|| not_found(format!("node not found: {}", req.label)))?;

            let node_id = node.id;
            let confidence = node.confidence;

            let properties = g.get_properties(&req.label).map_err(internal)?
                .unwrap_or_default();

            let edges_from = g.edges_from(&req.label).unwrap_or_default()
                .into_iter().map(|e| proto::EdgeHit {
                    from: e.from, to: e.to,
                    relationship: e.relationship,
                    confidence: e.confidence,
                }).collect();

            let edges_to = g.edges_to(&req.label).unwrap_or_default()
                .into_iter().map(|e| proto::EdgeHit {
                    from: e.from, to: e.to,
                    relationship: e.relationship,
                    confidence: e.confidence,
                }).collect();

            Ok(Response::new(proto::NodeResponse {
                node_id,
                label: req.label,
                confidence,
                properties,
                edges_from,
                edges_to,
            }))
        }

        async fn delete_node(
            &self,
            request: Request<proto::DeleteNodeRequest>,
        ) -> Result<Response<proto::DeleteResponse>, Status> {
            let req = request.into_inner();
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
            let prov = Provenance::user("grpc");

            let deleted = g.delete(&req.label, &prov).map_err(internal)?;

            if deleted {
                drop(g);
                self.state.mark_dirty();
            }

            Ok(Response::new(proto::DeleteResponse {
                deleted,
                entity: req.label,
            }))
        }

        async fn reinforce(
            &self,
            request: Request<proto::ReinforceRequest>,
        ) -> Result<Response<proto::ReinforceResponse>, Status> {
            let req = request.into_inner();
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;

            if !req.source.is_empty() {
                let prov = provenance(&req.source);
                g.reinforce_confirm(&req.entity, &prov).map_err(internal)?;
            } else {
                g.reinforce_access(&req.entity).map_err(internal)?;
            }

            let new_confidence = g.get_node(&req.entity).ok().flatten()
                .map(|n| n.confidence).unwrap_or(0.0);

            drop(g);
            self.state.mark_dirty();

            Ok(Response::new(proto::ReinforceResponse {
                entity: req.entity,
                new_confidence,
            }))
        }

        async fn correct(
            &self,
            request: Request<proto::CorrectRequest>,
        ) -> Result<Response<proto::CorrectResponse>, Status> {
            let req = request.into_inner();
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
            let prov = provenance(&req.source);

            let result = g.correct(&req.entity, &prov, 3).map_err(internal)?;

            let propagated_to: Vec<String> = match &result {
                Some(cr) => cr.propagated.iter()
                    .filter_map(|&(slot, _, _)| g.get_node_label_by_slot(slot))
                    .collect(),
                None => Vec::new(),
            };

            drop(g);
            self.state.mark_dirty();

            Ok(Response::new(proto::CorrectResponse {
                corrected: req.entity,
                propagated_to,
            }))
        }

        async fn decay(
            &self,
            _request: Request<proto::DecayRequest>,
        ) -> Result<Response<proto::DecayResponse>, Status> {
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
            let nodes_decayed = g.apply_decay().map_err(internal)?;

            drop(g);
            if nodes_decayed > 0 {
                self.state.mark_dirty();
            }

            Ok(Response::new(proto::DecayResponse { nodes_decayed }))
        }

        async fn health(
            &self,
            _request: Request<proto::HealthRequest>,
        ) -> Result<Response<proto::HealthResponse>, Status> {
            Ok(Response::new(proto::HealthResponse {
                status: "ok".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }))
        }

        async fn stats(
            &self,
            _request: Request<proto::StatsRequest>,
        ) -> Result<Response<proto::StatsResponse>, Status> {
            let g = self.state.graph.read().map_err(|_| internal("lock"))?;
            let (nodes, edges) = g.stats();
            Ok(Response::new(proto::StatsResponse { nodes, edges }))
        }

        async fn ask(
            &self,
            request: Request<proto::AskRequest>,
        ) -> Result<Response<proto::AskResponse>, Status> {
            let req = request.into_inner();
            let g = self.state.graph.read().map_err(|_| internal("lock"))?;
            let resp = crate::natural::handle_ask(&g, &req.question);

            Ok(Response::new(proto::AskResponse {
                interpretation: resp.interpretation,
                results: resp.results.into_iter().map(|r| proto::AskResult {
                    label: r.label,
                    confidence: r.confidence,
                    relationship: r.relationship.unwrap_or_default(),
                    detail: r.detail.unwrap_or_default(),
                }).collect(),
            }))
        }

        async fn tell(
            &self,
            request: Request<proto::TellRequest>,
        ) -> Result<Response<proto::TellResponse>, Status> {
            let req = request.into_inner();
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
            let source = if req.source.is_empty() { None } else { Some(req.source.as_str()) };
            let resp = crate::natural::handle_tell(&mut g, &req.statement, source);

            drop(g);
            self.state.mark_dirty();
            self.state.fire_rules_async();

            Ok(Response::new(proto::TellResponse {
                interpretation: resp.interpretation,
                actions: resp.actions,
            }))
        }
    }
}

// ── Streaming gRPC service (v1.1.0) ──

#[cfg(feature = "grpc")]
mod stream_service {
    use super::proto;
    use super::proto::engram_stream_service_server::EngramStreamService;
    use crate::state::AppState;
    use engram_core::graph::Provenance;
    use tonic::{Request, Response, Status, Streaming};
    use tokio_stream::wrappers::BroadcastStream;

    pub struct EngramStreamGrpc {
        pub state: AppState,
    }

    fn internal(e: impl std::fmt::Display) -> Status {
        Status::internal(e.to_string())
    }

    type GrpcStream<T> = std::pin::Pin<Box<dyn futures::Stream<Item = Result<T, Status>> + Send>>;

    #[tonic::async_trait]
    impl EngramStreamService for EngramStreamGrpc {
        type EventStreamStream = GrpcStream<proto::GraphEventMessage>;

        async fn event_stream(
            &self,
            request: Request<proto::EventStreamRequest>,
        ) -> Result<Response<Self::EventStreamStream>, Status> {
            use tokio_stream::StreamExt;

            let topics = request.into_inner().topics;
            let rx = self.state.event_bus.subscribe();

            let stream = BroadcastStream::new(rx)
                .filter_map(move |result| {
                    match result {
                        Ok(event) => {
                            let event_type = format!("{:?}", event);
                            let event_type_short = event_type.split('{').next().unwrap_or(&event_type).trim().to_string();

                            // Topic filtering
                            if !topics.is_empty() && !topics.iter().any(|t| event_type_short.to_lowercase().contains(&t.to_lowercase())) {
                                return None;
                            }

                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as i64)
                                .unwrap_or(0);

                            Some(Ok(proto::GraphEventMessage {
                                event_type: event_type_short,
                                data: serde_json::to_string(&format!("{:?}", event)).unwrap_or_default(),
                                timestamp,
                            }))
                        }
                        Err(_) => None,
                    }
                });

            Ok(Response::new(Box::pin(stream)))
        }

        type IngestProgressStream = GrpcStream<proto::IngestProgressMessage>;

        async fn ingest_progress(
            &self,
            request: Request<proto::IngestProgressRequest>,
        ) -> Result<Response<Self::IngestProgressStream>, Status> {
            let job_id = request.into_inner().job_id;

            // Stub: return a single "complete" message
            let msg = proto::IngestProgressMessage {
                job_id: job_id.clone(),
                phase: "complete".to_string(),
                processed: 0,
                total: 0,
                progress: 1.0,
                error: String::new(),
            };
            let stream = tokio_stream::once(Ok(msg));
            Ok(Response::new(Box::pin(stream)))
        }

        type EnrichStreamStream = GrpcStream<proto::EnrichmentMessage>;

        async fn enrich_stream(
            &self,
            request: Request<proto::EnrichStreamRequest>,
        ) -> Result<Response<Self::EnrichStreamStream>, Status> {
            let req = request.into_inner();
            let g = self.state.graph.read().map_err(|_| internal("lock"))?;

            // Local search phase
            let results = g.search(&req.query, 20).unwrap_or_default();
            let local_data = serde_json::json!({
                "results": results.iter().map(|r| serde_json::json!({
                    "label": r.label,
                    "confidence": r.confidence,
                    "score": r.score,
                })).collect::<Vec<_>>()
            });
            drop(g);

            let events = vec![
                proto::EnrichmentMessage {
                    phase: "local".to_string(),
                    status: "complete".to_string(),
                    data: serde_json::to_string(&local_data).unwrap_or_default(),
                },
                proto::EnrichmentMessage {
                    phase: "mesh".to_string(),
                    status: "skipped".to_string(),
                    data: "{}".to_string(),
                },
                proto::EnrichmentMessage {
                    phase: "external".to_string(),
                    status: "skipped".to_string(),
                    data: "{}".to_string(),
                },
            ];

            let stream = tokio_stream::iter(events.into_iter().map(Ok));
            Ok(Response::new(Box::pin(stream)))
        }

        async fn bulk_ingest(
            &self,
            request: Request<Streaming<proto::IngestItem>>,
        ) -> Result<Response<proto::BulkIngestResponse>, Status> {
            use tokio_stream::StreamExt;

            let start = std::time::Instant::now();
            let mut stream = request.into_inner();
            let mut ingested = 0u64;
            let mut errors = 0u64;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(item) => {
                        let source = if item.source.is_empty() { "grpc" } else { &item.source };
                        let prov = Provenance::user(source);
                        let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
                        match g.store(&item.text, &prov) {
                            Ok(_) => {
                                ingested += 1;
                                self.state.mark_dirty();
                            }
                            Err(_) => errors += 1,
                        }
                    }
                    Err(_) => errors += 1,
                }
            }

            Ok(Response::new(proto::BulkIngestResponse {
                ingested,
                relations_created: 0, // bulk ingest doesn't run relation extraction yet
                errors,
                duration_ms: start.elapsed().as_millis() as u64,
            }))
        }
    }
}

// ── Assessment gRPC service (v1.1.0) ──

#[cfg(all(feature = "grpc", feature = "assess"))]
mod assess_service {
    use super::proto;
    use super::proto::engram_assess_service_server::EngramAssessService;
    use crate::state::AppState;
    use engram_core::graph::Provenance;
    use tonic::{Request, Response, Status};

    pub struct EngramAssessGrpc {
        pub state: AppState,
    }

    fn internal(e: impl std::fmt::Display) -> Status {
        Status::internal(e.to_string())
    }

    #[tonic::async_trait]
    impl EngramAssessService for EngramAssessGrpc {
        async fn create_assessment(
            &self,
            request: Request<proto::CreateAssessmentRequest>,
        ) -> Result<Response<proto::AssessmentResponse>, Status> {
            let req = request.into_inner();
            let label = format!("Assessment:{}", req.title.to_lowercase().replace(' ', "-"));
            let initial_prob = if req.initial_probability > 0.0 { req.initial_probability } else { 0.50 };

            // Create graph node
            {
                let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
                let prov = Provenance::user("grpc");
                let _ = g.store_with_confidence(&label, initial_prob, &prov).map_err(internal)?;
                let _ = g.set_node_type(&label, "assessment");
                let _ = g.set_property(&label, "title", &req.title);
                let _ = g.set_property(&label, "category", &req.category);
                let _ = g.set_property(&label, "status", "active");
                let _ = g.set_property(&label, "current_probability", &initial_prob.to_string());
                if !req.description.is_empty() {
                    let _ = g.set_property(&label, "description", &req.description);
                }
                if !req.timeframe.is_empty() {
                    let _ = g.set_property(&label, "timeframe", &req.timeframe);
                }

                // Create watch edges
                for entity in &req.watches {
                    let _ = g.relate_with_confidence(&label, entity, "watches", 1.0, &prov);
                }
            }

            // Create sidecar record
            {
                let mut store = self.state.assessments.write().map_err(|_| internal("lock"))?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                let record = engram_assess::AssessmentRecord {
                    label: label.clone(),
                    node_id: 0,
                    history: vec![engram_assess::ScorePoint {
                        timestamp: now,
                        probability: initial_prob,
                        shift: 0.0,
                        trigger: engram_assess::ScoreTrigger::Created,
                        reason: "Assessment created".to_string(),
                        path: None,
                    }],
                    evidence: vec![],
                    success_criteria: None,
                    tags: vec![],
                    resolution: "active".to_string(),
                    pending_count: 0,
                    evidence_for: vec![],
                    evidence_against: vec![],
                };
                store.insert(record);
            }

            self.state.mark_dirty();

            Ok(Response::new(proto::AssessmentResponse {
                label,
                probability: initial_prob,
                status: "active".to_string(),
            }))
        }

        async fn get_assessment(
            &self,
            request: Request<proto::GetAssessmentRequest>,
        ) -> Result<Response<proto::AssessmentDetailResponse>, Status> {
            let label = request.into_inner().label;

            let g = self.state.graph.read().map_err(|_| internal("lock"))?;
            let props = g.get_properties(&label).map_err(internal)?.unwrap_or_default();

            // Get watches
            let watches: Vec<String> = g.edges_from(&label).unwrap_or_default()
                .into_iter()
                .filter(|e| e.relationship == "watches")
                .map(|e| e.to)
                .collect();
            drop(g);

            let store = self.state.assessments.read().map_err(|_| internal("lock"))?;
            let record = store.get(&label)
                .ok_or_else(|| Status::not_found(format!("assessment not found: {}", label)))?;

            let history: Vec<proto::ScorePointMessage> = record.history.iter().map(|p| {
                proto::ScorePointMessage {
                    timestamp: p.timestamp,
                    probability: p.probability,
                    shift: p.shift,
                    trigger: format!("{:?}", p.trigger),
                    reason: p.reason.clone(),
                }
            }).collect();

            Ok(Response::new(proto::AssessmentDetailResponse {
                label: label.clone(),
                title: props.get("title").cloned().unwrap_or_default(),
                category: props.get("category").cloned().unwrap_or_default(),
                status: props.get("status").cloned().unwrap_or_else(|| "active".to_string()),
                description: props.get("description").cloned().unwrap_or_default(),
                timeframe: props.get("timeframe").cloned().unwrap_or_default(),
                current_probability: record.history.last().map(|p| p.probability).unwrap_or(0.5),
                last_evaluated: record.history.last().map(|p| p.timestamp).unwrap_or(0),
                history,
                watches,
            }))
        }

        async fn list_assessments(
            &self,
            request: Request<proto::ListAssessmentsRequest>,
        ) -> Result<Response<proto::ListAssessmentsResponse>, Status> {
            let req = request.into_inner();
            let g = self.state.graph.read().map_err(|_| internal("lock"))?;
            let store = self.state.assessments.read().map_err(|_| internal("lock"))?;

            let mut assessments = Vec::new();
            for record in store.all() {
                let props = g.get_properties(&record.label).ok().flatten().unwrap_or_default();
                let category = props.get("category").cloned().unwrap_or_default();
                let status = props.get("status").cloned().unwrap_or_else(|| "active".to_string());

                if !req.category.is_empty() && category != req.category {
                    continue;
                }
                if !req.status.is_empty() && status != req.status {
                    continue;
                }

                let watch_count = g.edges_from(&record.label).unwrap_or_default()
                    .iter().filter(|e| e.relationship == "watches").count() as u32;

                assessments.push(proto::AssessmentSummaryMessage {
                    label: record.label.clone(),
                    title: props.get("title").cloned().unwrap_or_default(),
                    category,
                    status,
                    current_probability: record.history.last().map(|p| p.probability).unwrap_or(0.5),
                    last_evaluated: record.history.last().map(|p| p.timestamp).unwrap_or(0),
                    evidence_count: (record.evidence_for.len() + record.evidence_against.len()) as u32,
                    watch_count,
                    last_shift: record.history.last().map(|p| p.shift).unwrap_or(0.0),
                });
            }

            Ok(Response::new(proto::ListAssessmentsResponse { assessments }))
        }

        async fn evaluate_assessment(
            &self,
            request: Request<proto::EvaluateAssessmentRequest>,
        ) -> Result<Response<proto::EvaluationResponse>, Status> {
            let label = request.into_inner().label;

            let mut store = self.state.assessments.write().map_err(|_| internal("lock"))?;
            let record = store.get_mut(&label)
                .ok_or_else(|| Status::not_found(format!("assessment not found: {}", label)))?;

            let old_prob = record.history.last().map(|p| p.probability).unwrap_or(0.5);
            let point = engram_assess::evaluate(record);

            Ok(Response::new(proto::EvaluationResponse {
                label,
                old_probability: old_prob,
                new_probability: point.probability,
                shift: point.shift,
            }))
        }

        async fn add_evidence(
            &self,
            request: Request<proto::AddEvidenceRequest>,
        ) -> Result<Response<proto::EvidenceResponse>, Status> {
            let req = request.into_inner();
            let supports = req.direction != "contradicts";

            let mut store = self.state.assessments.write().map_err(|_| internal("lock"))?;
            let record = store.get_mut(&req.assessment_label)
                .ok_or_else(|| Status::not_found(format!("assessment not found: {}", req.assessment_label)))?;

            let point = engram_assess::add_evidence(
                record,
                req.confidence,
                supports,
                engram_assess::ScoreTrigger::Manual,
                format!("gRPC evidence: {}", req.node_label),
                None,
                &req.node_label,
                "grpc",
            );

            Ok(Response::new(proto::EvidenceResponse {
                added: true,
                new_probability: point.probability,
            }))
        }

        async fn add_watch(
            &self,
            request: Request<proto::AddWatchRequest>,
        ) -> Result<Response<proto::WatchResponse>, Status> {
            let req = request.into_inner();
            let mut g = self.state.graph.write().map_err(|_| internal("lock"))?;
            let prov = Provenance::user("grpc");

            g.relate_with_confidence(&req.assessment_label, &req.entity_label, "watches", 1.0, &prov)
                .map_err(internal)?;

            drop(g);
            self.state.mark_dirty();

            Ok(Response::new(proto::WatchResponse { added: true }))
        }

        type AssessmentStreamStream = std::pin::Pin<Box<dyn futures::Stream<Item = Result<proto::ScoreUpdate, Status>> + Send>>;

        async fn assessment_stream(
            &self,
            request: Request<proto::AssessmentStreamRequest>,
        ) -> Result<Response<Self::AssessmentStreamStream>, Status> {
            use tokio_stream::StreamExt;
            use tokio_stream::wrappers::BroadcastStream;

            let labels = request.into_inner().labels;
            let rx = self.state.event_bus.subscribe();

            let stream = BroadcastStream::new(rx)
                .filter_map(move |result| {
                    match result {
                        Ok(engram_core::events::GraphEvent::PropertyChanged { label, key, value, .. }) => {
                            if key.as_ref() != "current_probability" {
                                return None;
                            }
                            if !labels.is_empty() && !labels.iter().any(|l| l == label.as_ref()) {
                                return None;
                            }

                            let (prob, shift) = if let Some(shift_str) = value.split('|').nth(1) {
                                let prob = value.split('|').next().unwrap_or("0.5").parse().unwrap_or(0.5);
                                let shift = shift_str.parse().unwrap_or(0.0);
                                (prob, shift)
                            } else {
                                (value.parse().unwrap_or(0.5), 0.0)
                            };

                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as i64)
                                .unwrap_or(0);

                            Some(Ok(proto::ScoreUpdate {
                                label: label.to_string(),
                                probability: prob,
                                shift,
                                trigger: "graph_propagation".to_string(),
                                timestamp,
                            }))
                        }
                        _ => None,
                    }
                });

            Ok(Response::new(Box::pin(stream)))
        }
    }
}

/// Start the real gRPC server (protobuf binary, tonic).
#[cfg(feature = "grpc")]
pub async fn serve_grpc(state: crate::state::AppState, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    use proto::engram_service_server::EngramServiceServer;
    use proto::engram_stream_service_server::EngramStreamServiceServer;

    let svc = service::EngramGrpc { state: state.clone() };
    let stream_svc = stream_service::EngramStreamGrpc { state: state.clone() };
    let addr = addr.parse().map_err(|e| format!("invalid gRPC address: {e}"))?;

    tracing::info!("engram gRPC service (tonic) listening on {}", addr);
    let mut builder = tonic::transport::Server::builder()
        .add_service(EngramServiceServer::new(svc))
        .add_service(EngramStreamServiceServer::new(stream_svc));

    #[cfg(feature = "assess")]
    {
        use proto::engram_assess_service_server::EngramAssessServiceServer;
        let assess_svc = assess_service::EngramAssessGrpc { state };
        builder = builder.add_service(EngramAssessServiceServer::new(assess_svc));
    }

    builder.serve(addr).await?;
    Ok(())
}

// ── Fallback: JSON-over-HTTP/2 (no grpc feature) ──

#[cfg(not(feature = "grpc"))]
pub async fn serve_grpc(state: crate::state::AppState, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    use axum::routing::{get, post};
    use axum::Router;
    use crate::handlers;

    let app = Router::new()
        .route("/engram.EngramService/Store", post(handlers::store))
        .route("/engram.EngramService/Relate", post(handlers::relate))
        .route("/engram.EngramService/Query", post(handlers::query))
        .route("/engram.EngramService/Search", post(handlers::search))
        .route("/engram.EngramService/GetNode", post(handlers::get_node_by_body))
        .route("/engram.EngramService/DeleteNode", post(handlers::delete_node_by_body))
        .route("/engram.EngramService/Reinforce", post(handlers::reinforce))
        .route("/engram.EngramService/Correct", post(handlers::correct))
        .route("/engram.EngramService/Decay", post(handlers::decay))
        .route("/engram.EngramService/Health", get(handlers::health))
        .route("/engram.EngramService/Stats", post(handlers::stats_post))
        .route("/engram.EngramService/Ask", post(handlers::ask))
        .route("/engram.EngramService/Tell", post(handlers::tell))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("engram gRPC service (JSON fallback) listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
