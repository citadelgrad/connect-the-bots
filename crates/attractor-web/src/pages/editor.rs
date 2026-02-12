use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use crate::components::markdown_pane::MarkdownPane;

#[component]
pub fn EditorPage() -> impl IntoView {
    let query = use_query_map();

    let prd_content = move || {
        query
            .get()
            .get("prd")
            .and_then(|s| urlencoding::decode(&s).ok().map(|cow| cow.into_owned()))
            .unwrap_or_else(|| {
                "# No PRD Content\n\nNavigate from the prompt page to generate PRD content."
                    .to_string()
            })
    };

    let spec_content = move || {
        query
            .get()
            .get("spec")
            .and_then(|s| urlencoding::decode(&s).ok().map(|cow| cow.into_owned()))
            .unwrap_or_else(|| {
                "# No Spec Content\n\nNavigate from the prompt page to generate spec content."
                    .to_string()
            })
    };

    view! {
        <div class="editor-page">
            <h1>"PRD / Spec Editor"</h1>
            <div class="editor-container">
                <MarkdownPane title="PRD (Product Requirements)" content=prd_content/>
                <MarkdownPane title="Technical Specification" content=spec_content/>
            </div>
        </div>
    }
}
