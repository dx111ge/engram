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

/// Start the real gRPC server (protobuf binary, tonic).
#[cfg(feature = "grpc")]
pub async fn serve_grpc(state: crate::state::AppState, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    use proto::engram_service_server::EngramServiceServer;

    let svc = service::EngramGrpc { state };
    let addr = addr.parse().map_err(|e| format!("invalid gRPC address: {e}"))?;

    tracing::info!("engram gRPC service (tonic) listening on {}", addr);
    tonic::transport::Server::builder()
        .add_service(EngramServiceServer::new(svc))
        .serve(addr)
        .await?;
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
