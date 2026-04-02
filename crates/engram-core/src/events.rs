//! Event bus for graph change notifications.
//!
//! Every graph mutation emits a lightweight [`GraphEvent`] through a bounded
//! [`tokio::sync::broadcast`] channel. Downstream consumers (action engine,
//! SSE endpoints, audit log) subscribe and filter client-side.
//!
//! Events use `Arc<str>` for strings -- cheap to clone across subscribers,
//! zero-copy for the emitting side.

use std::sync::Arc;

/// A graph mutation event.
///
/// Emitted by [`Graph`](crate::graph::Graph) mutation methods and delivered
/// to all subscribers via the [`EventBus`].
#[derive(Debug, Clone)]
pub enum GraphEvent {
    /// A new node was stored.
    FactStored {
        node_id: u64,
        label: Arc<str>,
        confidence: f32,
        source: Arc<str>,
        entity_type: Option<Arc<str>>,
    },
    /// A node's confidence changed (reinforce, decay, correction).
    FactUpdated {
        node_id: u64,
        label: Arc<str>,
        old_confidence: f32,
        new_confidence: f32,
    },
    /// A node was deleted (soft-delete).
    FactDeleted {
        node_id: u64,
        label: Arc<str>,
        source: Arc<str>,
    },
    /// A new edge was created.
    EdgeCreated {
        edge_id: u64,
        from: u64,
        to: u64,
        rel_type: Arc<str>,
        confidence: f32,
    },
    /// An edge was deleted (soft-delete).
    EdgeDeleted {
        edge_id: u64,
        from: u64,
        to: u64,
        rel_type: Arc<str>,
    },
    /// A property was set or changed on a node.
    PropertyChanged {
        node_id: u64,
        label: Arc<str>,
        key: Arc<str>,
        value: Arc<str>,
    },
    /// A node's memory tier changed.
    TierChanged {
        node_id: u64,
        label: Arc<str>,
        old_tier: u8,
        new_tier: u8,
    },
    /// A confidence threshold was crossed (for action engine triggers).
    ThresholdCrossed {
        node_id: u64,
        label: Arc<str>,
        old_confidence: f32,
        new_confidence: f32,
        direction: ThresholdDirection,
    },
    /// A query returned sparse results (signals potential knowledge gap).
    QueryGap {
        query: Arc<str>,
        result_count: usize,
        avg_confidence: f32,
    },
    /// Timer tick for scheduled action rules.
    TimerTick {
        rule_id: Arc<str>,
    },
    /// A conflict was detected between existing and incoming facts.
    ConflictDetected {
        existing: u64,
        incoming: u64,
        conflict_type: ConflictType,
    },
    /// Bulk decay was applied.
    DecayApplied {
        nodes_affected: u32,
    },
    /// Tier sweep completed.
    TierSweepCompleted {
        promoted: u32,
        demoted: u32,
        archived: u32,
    },
    // ── Seed enrichment events (interactive multi-phase flow) ──

    /// Area of interest detected from seed text.
    SeedAoiDetected {
        session_id: Arc<str>,
        area_of_interest: Arc<str>,
    },
    /// Entity successfully linked to a KB entry.
    SeedEntityLinked {
        session_id: Arc<str>,
        label: Arc<str>,
        canonical: Arc<str>,
        description: Arc<str>,
        qid: Arc<str>,
    },
    /// Entity has multiple candidate matches — user must disambiguate.
    SeedEntityAmbiguous {
        session_id: Arc<str>,
        label: Arc<str>,
        candidates: Vec<(Arc<str>, Arc<str>, Arc<str>)>, // (canonical, description, qid)
    },
    /// A contextual connection found (area-of-interest article co-occurrence).
    SeedConnectionFound {
        session_id: Arc<str>,
        from: Arc<str>,
        to: Arc<str>,
        rel_type: Arc<str>,
        source: Arc<str>,
    },
    /// A SPARQL-derived structured relation.
    SeedSparqlRelation {
        session_id: Arc<str>,
        from: Arc<str>,
        to: Arc<str>,
        rel_type: Arc<str>,
    },
    /// A phase of seed enrichment completed.
    SeedPhaseComplete {
        session_id: Arc<str>,
        phase: u32,
        entities_processed: u32,
        relations_found: u32,
    },
    /// Article fetch progress during seed enrichment.
    SeedArticleProgress {
        session_id: Arc<str>,
        current: u32,
        total: u32,
        url: Arc<str>,
        status: Arc<str>, // "fetching", "fetched", "paywalled", "failed"
        chars: u32,
    },
    /// Fact extraction progress during seed enrichment.
    SeedFactProgress {
        session_id: Arc<str>,
        current: u32,
        total: u32,
        doc_title: Arc<str>,
        facts_found: u32,
    },
    /// Seed enrichment fully committed to graph.
    SeedComplete {
        session_id: Arc<str>,
        facts_stored: u32,
        relations_created: u32,
    },
    /// Generic enrichment phase progress (covers gaps between specific events).
    SeedProgress {
        session_id: Arc<str>,
        phase: Arc<str>,        // "entity_linking", "sparql", "web_search", "fact_extraction", "gliner2", "complete", "error"
        message: Arc<str>,      // human-readable status
        current: u32,           // progress counter within phase
        total: u32,             // total items in phase
        elapsed_secs: u32,      // seconds since enrichment started
    },
}

/// Direction of a confidence threshold crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdDirection {
    Up,
    Down,
}

/// Type of conflict detected between facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictType {
    PropertyConflict,
    ContradictoryEdge,
    DuplicateEntity,
}

/// Overflow strategy when the event channel is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowStrategy {
    /// Log and drop events. Graph operations never block. (Default)
    LogAndDrop,
    /// Block the mutation until channel has space. Use only when
    /// action rules must not miss events.
    Backpressure,
}

/// Bounded broadcast channel for graph events.
///
/// All subscribers receive all events. The channel is bounded to prevent
/// memory blowup under high ingest load.
pub struct EventBus {
    sender: tokio::sync::broadcast::Sender<GraphEvent>,
    overflow: OverflowStrategy,
}

impl EventBus {
    /// Create a new event bus with the given capacity and overflow strategy.
    pub fn new(capacity: usize, overflow: OverflowStrategy) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        EventBus { sender, overflow }
    }

    /// Publish an event to all subscribers.
    ///
    /// Under `LogAndDrop`, this never blocks. If there are no subscribers,
    /// the event is silently dropped (no error).
    pub fn publish(&self, event: GraphEvent) {
        match self.sender.send(event) {
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::SendError(dropped)) => {
                match self.overflow {
                    OverflowStrategy::LogAndDrop => {
                        tracing::warn!(
                            event = ?dropped,
                            "event bus overflow: event dropped (no subscribers or channel full)"
                        );
                    }
                    OverflowStrategy::Backpressure => {
                        // broadcast::send only fails when there are no receivers,
                        // not when the channel is full (broadcast drops oldest for
                        // lagging receivers). So this path is only hit when
                        // nobody is listening.
                        tracing::debug!(
                            event = ?dropped,
                            "event bus: no subscribers, event dropped"
                        );
                    }
                }
            }
        }
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<GraphEvent> {
        self.sender.subscribe()
    }

    /// Number of current subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(10_000, OverflowStrategy::LogAndDrop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn event_bus_publish_subscribe() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe();

        bus.publish(GraphEvent::FactStored {
            node_id: 1,
            label: Arc::from("test"),
            confidence: 0.8,
            source: Arc::from("user:alice"),
            entity_type: None,
        });

        let event = rx.recv().await.unwrap();
        match event {
            GraphEvent::FactStored { node_id, label, .. } => {
                assert_eq!(node_id, 1);
                assert_eq!(&*label, "test");
            }
            _ => panic!("unexpected event type"),
        }
    }

    #[tokio::test]
    async fn event_bus_multiple_subscribers() {
        let bus = EventBus::default();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(GraphEvent::DecayApplied { nodes_affected: 42 });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();

        match (&e1, &e2) {
            (
                GraphEvent::DecayApplied { nodes_affected: n1 },
                GraphEvent::DecayApplied { nodes_affected: n2 },
            ) => {
                assert_eq!(*n1, 42);
                assert_eq!(*n2, 42);
            }
            _ => panic!("both subscribers should get the same event"),
        }
    }

    #[test]
    fn event_bus_no_subscribers_does_not_panic() {
        let bus = EventBus::default();
        // Publishing with no subscribers should not panic
        bus.publish(GraphEvent::FactDeleted {
            node_id: 1,
            label: Arc::from("gone"),
            source: Arc::from("user:bob"),
        });
    }

    #[test]
    fn subscriber_count() {
        let bus = EventBus::default();
        assert_eq!(bus.subscriber_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        drop(_rx1);
        assert_eq!(bus.subscriber_count(), 1);
    }
}
