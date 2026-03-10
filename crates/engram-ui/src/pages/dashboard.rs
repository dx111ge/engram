use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{HealthResponse, StatsResponse, ComputeResponse};
use crate::components::stat_card::StatCard;

#[component]
pub fn Dashboard() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let api1 = api.clone();
    let api2 = api.clone();
    let api3 = api.clone();

    let health = LocalResource::new(move || {
        let api = api1.clone();
        async move { api.get::<HealthResponse>("/health").await.ok() }
    });

    let stats = LocalResource::new(move || {
        let api = api2.clone();
        async move { api.get::<StatsResponse>("/stats").await.ok() }
    });

    let compute = LocalResource::new(move || {
        let api = api3.clone();
        async move { api.get::<ComputeResponse>("/compute").await.ok() }
    });

    let node_count = Signal::derive(move || {
        stats.get()
            .flatten()
            .map(|s| s.nodes.to_string())
            .unwrap_or_else(|| "--".into())
    });
    let edge_count = Signal::derive(move || {
        stats.get()
            .flatten()
            .map(|s| s.edges.to_string())
            .unwrap_or_else(|| "--".into())
    });
    let prop_count = Signal::derive(move || {
        stats.get()
            .flatten()
            .map(|s| s.properties.to_string())
            .unwrap_or_else(|| "--".into())
    });
    let health_status = Signal::derive(move || {
        health.get()
            .flatten()
            .map(|h| h.status)
            .unwrap_or_else(|| "offline".into())
    });
    let backend = Signal::derive(move || {
        compute.get()
            .flatten()
            .map(|c| c.backend)
            .unwrap_or_else(|| "--".into())
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-gauge"></i>" Dashboard"</h2>
        </div>

        <div class="stat-grid">
            <StatCard icon="fa-solid fa-circle-nodes" label="Nodes" value=node_count />
            <StatCard icon="fa-solid fa-arrows-left-right" label="Edges" value=edge_count />
            <StatCard icon="fa-solid fa-tags" label="Properties" value=prop_count />
            <StatCard icon="fa-solid fa-heart-pulse" label="Health" value=health_status />
            <StatCard icon="fa-solid fa-microchip" label="Compute" value=backend />
        </div>
    }
}
