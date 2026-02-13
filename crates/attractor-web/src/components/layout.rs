use leptos::prelude::*;

use crate::components::approval_bar::ApprovalBar;
use crate::components::document_viewer::DocumentViewer;
use crate::components::execution_panel::ExecutionPanel;
use crate::components::terminal::Terminal;
use crate::server::projects::Project;

/// View mode for the right panel
#[derive(Clone, Copy, PartialEq)]
pub enum RightPanel {
    Documents,
    Execution,
}

/// Project-scoped two-column layout: Terminal (left) + Document/Execution viewer (right)
/// Accepts a Project prop and renders the terminal + document panels for that project.
/// Multiple instances can coexist simultaneously (one per open project).
#[component]
pub fn ProjectView(
    #[prop(into)] project: Project,
) -> impl IntoView {
    // Extract project fields for use in component
    let project_id = project.id;
    let folder = project.folder_path.clone();
    let project_name = project.name.clone();
    let container_id = format!("terminal-{}", project_id);

    let (panel, set_panel) = signal(RightPanel::Documents);
    let (session_id, set_session_id) = signal(Option::<String>::None);
    let (prd_exists, set_prd_exists) = signal(false);
    let (spec_exists, set_spec_exists) = signal(false);

    let can_approve = move || prd_exists.get() && spec_exists.get();

    let on_approve = move |sid: String| {
        set_session_id.set(Some(sid));
        set_panel.set(RightPanel::Execution);
    };

    let on_back_to_docs = move |_| {
        set_panel.set(RightPanel::Documents);
    };

    view! {
        <div class="app-layout">
            <header class="app-header">
                <h1 class="app-title">{project_name.clone()}</h1>
                <div class="app-header-actions">
                    {move || match panel.get() {
                        RightPanel::Documents => {
                            view! {
                                <ApprovalBar
                                    project_id=project_id
                                    enabled=can_approve
                                    on_approve=on_approve
                                />
                            }.into_any()
                        }
                        RightPanel::Execution => {
                            view! {
                                <button class="btn btn-secondary" on:click=on_back_to_docs>
                                    "Back to Docs"
                                </button>
                            }.into_any()
                        }
                    }}
                </div>
            </header>

            <div class=move || {
                let base = "app-panels";
                if panel.get() == RightPanel::Documents { format!("{base} documents-mode") } else { base.to_string() }
            }>
                <div class="panel-left">
                    <Terminal
                        folder=folder.clone()
                        container_id=container_id.clone()
                    />
                </div>

                {move || match panel.get() {
                    RightPanel::Documents => {
                        view! {
                            <DocumentViewer
                                project_id=project_id
                                on_prd_change=move |exists| set_prd_exists.set(exists)
                                on_spec_change=move |exists| set_spec_exists.set(exists)
                            />
                        }.into_any()
                    }
                    RightPanel::Execution => {
                        view! {
                            <div class="panel-right">
                                <ExecutionPanel
                                    session_id=move || session_id.get().unwrap_or_default()
                                />
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
