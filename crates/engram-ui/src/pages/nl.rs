use leptos::prelude::*;

use crate::api::ApiClient;

#[derive(Clone, Debug)]
struct Message {
    role: String,
    text: String,
}

#[component]
pub fn NlPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (messages, set_messages) = signal(Vec::<Message>::new());
    let (input, set_input) = signal(String::new());
    let (loading, set_loading) = signal(false);

    let api_c = api.clone();
    let send_message = Action::new_local(move |_: &()| {
        let api = api_c.clone();
        let text = input.get_untracked();
        async move {
            if text.is_empty() {
                return;
            }

            set_messages.update(|msgs| {
                msgs.push(Message { role: "user".into(), text: text.clone() });
            });
            set_input.set(String::new());
            set_loading.set(true);

            // Determine if this is a tell or ask
            let lower = text.to_lowercase();
            let is_tell = lower.starts_with("tell") || lower.starts_with("remember")
                || lower.starts_with("store") || lower.starts_with("add")
                || lower.contains(" is ") || lower.contains(" are ");

            let result = if is_tell {
                let body = serde_json::json!({"text": text, "source": "ui"});
                api.post_text("/tell", &body).await
            } else {
                let body = serde_json::json!({"text": text});
                api.post_text("/ask", &body).await
            };

            let response = match result {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            };

            set_messages.update(|msgs| {
                msgs.push(Message { role: "assistant".into(), text: response });
            });
            set_loading.set(false);
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        send_message.dispatch(());
    };

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-comments"></i>" Natural Language"</h2>
        </div>

        <div class="chat-container">
            <div class="chat-messages">
                <For
                    each={move || messages.get().into_iter().enumerate().collect::<Vec<_>>()}
                    key={|(i, _)| *i}
                    children={move |(_, msg)| {
                        let role_label = if msg.role == "user" { "You" } else { "engram" };
                        let cls = format!("chat-message {}", msg.role);
                        view! {
                            <div class=cls>
                                <div class="message-role">{role_label}</div>
                                <div class="message-text">{msg.text.clone()}</div>
                            </div>
                        }
                    }}
                />
                {move || loading.get().then(|| view! {
                    <div class="chat-message assistant">
                        <div class="message-role">"engram"</div>
                        <div class="message-text typing">"..."</div>
                    </div>
                })}
            </div>

            <form class="chat-input-form" on:submit=on_submit>
                <input
                    type="text"
                    class="chat-input"
                    placeholder="Tell me something or ask a question..."
                    prop:value=input
                    on:input=move |ev| set_input.set(event_target_value(&ev))
                    disabled=move || loading.get()
                />
                <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                    <i class="fa-solid fa-paper-plane"></i>
                </button>
            </form>
        </div>
    }
}
