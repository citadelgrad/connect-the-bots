use leptos::prelude::*;

#[component]
pub fn MarkdownPane<F>(
    #[prop(into)] title: String,
    content: F,
) -> impl IntoView
where
    F: Fn() -> String + Send + 'static,
{
    view! {
        <div class="markdown-pane">
            <h2 class="pane-title">{title}</h2>
            <div class="markdown-content">
                <pre>{move || content()}</pre>
            </div>
        </div>
    }
}
