use leptos::prelude::*;

#[component]
pub fn Modal(
    #[prop(into)] title: String,
    #[prop(into)] open: ReadSignal<bool>,
    #[prop(into)] on_close: Callback<()>,
    children: Children,
) -> impl IntoView {
    let overlay_class = move || {
        if open.get() {
            "modal-overlay active"
        } else {
            "modal-overlay"
        }
    };

    let close = move |_| {
        on_close.run(());
    };

    view! {
        <div class=overlay_class on:click=close>
            <div class="modal" on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h3>{title.clone()}</h3>
                    <button class="btn-icon modal-close" on:click=close>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="modal-body">
                    {children()}
                </div>
            </div>
        </div>
    }
}
