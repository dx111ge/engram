use leptos::prelude::*;

#[component]
pub fn DataTable(
    #[prop(into)] headers: Vec<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="table-wrapper">
            <table class="data-table">
                <thead>
                    <tr>
                        {headers.into_iter().map(|h| view! {
                            <th>{h}</th>
                        }).collect::<Vec<_>>()}
                    </tr>
                </thead>
                <tbody>
                    {children()}
                </tbody>
            </table>
        </div>
    }
}
