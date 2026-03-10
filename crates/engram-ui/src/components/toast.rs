use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
}

#[derive(Clone, Debug)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

impl ToastKind {
    pub fn class(&self) -> &'static str {
        match self {
            ToastKind::Success => "toast toast-success",
            ToastKind::Error => "toast toast-error",
            ToastKind::Info => "toast toast-info",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ToastKind::Success => "fa-solid fa-check-circle",
            ToastKind::Error => "fa-solid fa-exclamation-circle",
            ToastKind::Info => "fa-solid fa-info-circle",
        }
    }
}

#[component]
pub fn ToastContainer() -> impl IntoView {
    let toasts = use_context::<ReadSignal<Vec<Toast>>>()
        .expect("ToastContainer requires toast context");

    view! {
        <div id="toast-container">
            <For
                each={move || toasts.get().into_iter().enumerate().collect::<Vec<_>>()}
                key={|(i, _)| *i}
                children={move |(_, toast)| {
                    let cls = toast.kind.class();
                    let icon = toast.kind.icon();
                    view! {
                        <div class=cls>
                            <i class=icon></i>
                            <span>{toast.message.clone()}</span>
                        </div>
                    }
                }}
            />
        </div>
    }
}
