use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::KgeTrainResponse;

#[component]
pub fn LearningPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (entity, set_entity) = signal(String::new());
    let (result_msg, set_result_msg) = signal(String::new());
    let (kge_epochs, set_kge_epochs) = signal("100".to_string());
    let (kge_result, set_kge_result) = signal(Option::<KgeTrainResponse>::None);
    let (kge_loading, set_kge_loading) = signal(false);

    let api1 = api.clone();
    let reinforce = Action::new_local(move |_: &()| {
        let api = api1.clone();
        let e = entity.get_untracked();
        async move {
            let body = serde_json::json!({"entity": e});
            match api.post_text("/learn/reinforce", &body).await {
                Ok(r) => set_result_msg.set(format!("Reinforced: {r}")),
                Err(e) => set_result_msg.set(format!("Error: {e}")),
            }
        }
    });

    let api2 = api.clone();
    let correct = Action::new_local(move |_: &()| {
        let api = api2.clone();
        let e = entity.get_untracked();
        async move {
            let body = serde_json::json!({"entity": e});
            match api.post_text("/learn/correct", &body).await {
                Ok(r) => set_result_msg.set(format!("Corrected: {r}")),
                Err(e) => set_result_msg.set(format!("Error: {e}")),
            }
        }
    });

    let api3 = api.clone();
    let decay = Action::new_local(move |_: &()| {
        let api = api3.clone();
        async move {
            let body = serde_json::json!({});
            match api.post_text("/learn/decay", &body).await {
                Ok(r) => set_result_msg.set(format!("Decay applied: {r}")),
                Err(e) => set_result_msg.set(format!("Error: {e}")),
            }
        }
    });

    let api4 = api.clone();
    let train_kge = Action::new_local(move |_: &()| {
        let api = api4.clone();
        let epochs_str = kge_epochs.get_untracked();
        async move {
            set_kge_loading.set(true);
            let epochs: u64 = epochs_str.parse().unwrap_or(100);
            let body = serde_json::json!({"epochs": epochs});
            match api.post::<_, KgeTrainResponse>("/kge/train", &body).await {
                Ok(r) => {
                    set_result_msg.set(format!(
                        "KGE trained: {} epochs, loss {:.4}, {} entities, {} relation types",
                        r.epochs_completed, r.final_loss, r.entity_count, r.relation_type_count
                    ));
                    set_kge_result.set(Some(r));
                }
                Err(e) => set_result_msg.set(format!("KGE error: {e}")),
            }
            set_kge_loading.set(false);
        }
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-graduation-cap"></i>" Learning"</h2>
        </div>

        {move || {
            let msg = result_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        <div class="card">
            <h3>"Confidence Management"</h3>
            <div class="form-row">
                <input
                    type="text"
                    placeholder="Entity name..."
                    prop:value=entity
                    on:input=move |ev| set_entity.set(event_target_value(&ev))
                />
            </div>
            <div class="button-group">
                <button class="btn btn-success" on:click=move |_| { reinforce.dispatch(()); }>
                    <i class="fa-solid fa-arrow-up"></i>" Reinforce"
                </button>
                <button class="btn btn-warning" on:click=move |_| { correct.dispatch(()); }>
                    <i class="fa-solid fa-arrow-down"></i>" Correct"
                </button>
                <button class="btn btn-secondary" on:click=move |_| { decay.dispatch(()); }>
                    <i class="fa-solid fa-clock-rotate-left"></i>" Apply Decay"
                </button>
            </div>
        </div>

        <div class="card">
            <h3><i class="fa-solid fa-brain"></i>" Relation Learning (KGE)"</h3>
            <p class="card-description">
                "Train RotatE knowledge graph embeddings to predict relations between entities. "
                "The model learns structural patterns from existing edges."
            </p>

            {move || kge_result.get().map(|r| view! {
                <div class="kge-stats">
                    <div class="stat-row">
                        <span class="stat-label"><i class="fa-solid fa-database"></i>" Entities"</span>
                        <span class="stat-value">{r.entity_count}</span>
                    </div>
                    <div class="stat-row">
                        <span class="stat-label"><i class="fa-solid fa-link"></i>" Relation types"</span>
                        <span class="stat-value">{r.relation_type_count}</span>
                    </div>
                    <div class="stat-row">
                        <span class="stat-label"><i class="fa-solid fa-chart-line"></i>" Final loss"</span>
                        <span class="stat-value">{format!("{:.4}", r.final_loss)}</span>
                    </div>
                    <div class="stat-row">
                        <span class="stat-label"><i class="fa-solid fa-repeat"></i>" Epochs"</span>
                        <span class="stat-value">{r.epochs_completed}</span>
                    </div>
                </div>
            })}

            <div class="form-row">
                <label>"Epochs"</label>
                <input
                    type="number"
                    min="10"
                    max="1000"
                    prop:value=kge_epochs
                    on:input=move |ev| set_kge_epochs.set(event_target_value(&ev))
                    style="width: 100px"
                />
                <button
                    class="btn btn-primary"
                    on:click=move |_| { train_kge.dispatch(()); }
                    disabled=move || kge_loading.get()
                >
                    {move || if kge_loading.get() {
                        "Training..."
                    } else {
                        "Train KGE"
                    }}
                </button>
            </div>
        </div>
    }
}
