use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::PeerInfo;

#[component]
pub fn MeshPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (peers, set_peers) = signal(Vec::<PeerInfo>::new());
    let (status_msg, set_status_msg) = signal(String::new());
    let (identity, set_identity) = signal(String::new());

    let api1 = api.clone();
    let load_peers = Action::new_local(move |_: &()| {
        let api = api1.clone();
        async move {
            match api.get::<Vec<PeerInfo>>("/mesh/peers").await {
                Ok(p) => set_peers.set(p),
                Err(e) => set_status_msg.set(format!("Error: {e}")),
            }
        }
    });

    load_peers.dispatch(());

    let api2 = api.clone();
    let load_identity = Action::new_local(move |_: &()| {
        let api = api2.clone();
        async move {
            match api.get_text("/mesh/identity").await {
                Ok(id) => set_identity.set(id),
                Err(_) => set_identity.set("unavailable".into()),
            }
        }
    });

    load_identity.dispatch(());

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-network-wired"></i>" Knowledge Mesh"</h2>
            <div class="page-actions">
                <button class="btn btn-primary" on:click=move |_| { load_peers.dispatch(()); }>
                    <i class="fa-solid fa-refresh"></i>" Refresh"
                </button>
            </div>
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        <div class="mesh-grid">
            <div class="card">
                <h3><i class="fa-solid fa-fingerprint"></i>" Identity"</h3>
                <pre class="code-area">{identity}</pre>
            </div>

            <div class="card">
                <h3><i class="fa-solid fa-users"></i>" Peers"</h3>
                <div class="table-wrapper">
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Key"</th>
                                <th>"Endpoint"</th>
                                <th>"Trust"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each={move || peers.get()}
                                key={|p| p.key.clone()}
                                children={move |peer| {
                                    let short_key = if peer.key.len() > 16 {
                                        format!("{}...", &peer.key[..16])
                                    } else {
                                        peer.key.clone()
                                    };
                                    view! {
                                        <tr>
                                            <td><code>{short_key}</code></td>
                                            <td>{peer.endpoint.clone().unwrap_or_default()}</td>
                                            <td>{peer.trust_level.clone().unwrap_or_default()}</td>
                                        </tr>
                                    }
                                }}
                            />
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    }
}
