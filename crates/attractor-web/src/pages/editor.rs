use leptos::prelude::*;
use crate::components::markdown_pane::MarkdownPane;

#[component]
pub fn EditorPage() -> impl IntoView {
    view! {
        <div class="editor-page">
            <h1>"PRD / Spec Editor"</h1>
            <div class="editor-container">
                <MarkdownPane title="Original PRD" content="# Original PRD\n\nThis is the original PRD content.\n\n## Section 1\n\nPlaceholder text for the original document."/>
                <MarkdownPane title="Edited Version" content="# Edited Version\n\nThis is the edited version.\n\n## Section 1\n\nModified content appears here."/>
            </div>
        </div>
    }
}
