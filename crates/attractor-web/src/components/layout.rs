use leptos::prelude::*;

use crate::components::approval_bar::ApprovalBar;
use crate::components::document_viewer::DocumentViewer;
use crate::components::execution_panel::ExecutionPanel;
use crate::components::terminal::Terminal;

/// View mode for the right panel
#[derive(Clone, Copy, PartialEq)]
pub enum RightPanel {
    Documents,
    Execution,
}

/// Single-page two-column layout: Terminal (left) + Document/Execution viewer (right)
#[component]
pub fn MainLayout() -> impl IntoView {
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
                <h1 class="app-title">"Attractor"</h1>
                <div class="app-header-actions">
                    {move || match panel.get() {
                        RightPanel::Documents => {
                            view! {
                                <ApprovalBar
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
                    <Terminal />
                </div>

                {move || match panel.get() {
                    RightPanel::Documents => {
                        view! {
                            <DocumentViewer
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
