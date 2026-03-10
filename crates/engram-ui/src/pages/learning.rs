use leptos::prelude::*;

use crate::api::ApiClient;

#[component]
pub fn LearningPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (entity, set_entity) = signal(String::new());
    let (result_msg, set_result_msg) = signal(String::new());

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
    }
}
