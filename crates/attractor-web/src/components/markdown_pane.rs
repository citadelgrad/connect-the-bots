use leptos::prelude::*;

#[component]
pub fn MarkdownPane(
    #[prop(into)] title: String,
    #[prop(into)] content: String,
) -> impl IntoView {
    view! {
        <div class="markdown-pane">
            <h2 class="pane-title">{title}</h2>
            <div class="markdown-content">
                <pre>{content}</pre>
            </div>
        </div>
    }
}
