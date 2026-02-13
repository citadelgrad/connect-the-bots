use leptos::prelude::*;

use crate::server::execute::{start_execution, ExecutionPhase};

/// "Approve & Execute" button in the header bar.
/// Triggers decompose → scaffold → pipeline chain.
#[component]
pub fn ApprovalBar<F>(
    project_id: i64,
    enabled: impl Fn() -> bool + Send + Sync + Copy + 'static,
    on_approve: F,
) -> impl IntoView
where
    F: Fn(String) + Copy + Send + Sync + 'static,
{
    let (phase, set_phase) = signal(Option::<ExecutionPhase>::None);

    let execute_action = Action::new(move |_: &()| async move { start_execution(project_id).await });

    // Watch for action completion
    Effect::new(move || {
        if let Some(result) = execute_action.value().get() {
            match result {
                Ok(response) => {
                    tracing::info!("Execution started: session_id={}", response.session_id);
                    on_approve(response.session_id);
                }
                Err(e) => {
                    tracing::error!("Execution failed: {:?}", e);
                    set_phase.set(None);
                }
            }
        }
    });

    let is_loading = move || execute_action.pending().get();

    let on_click = move |_| {
        set_phase.set(Some(ExecutionPhase::Decomposing));
        execute_action.dispatch(());
    };

    view! {
        {move || {
            if let Some(p) = phase.get() {
                view! {
                    <div class="approval-progress">
                        <span class="spinner-sm"></span>
                        <span>{match p {
                            ExecutionPhase::Decomposing => "Decomposing...",
                            ExecutionPhase::Scaffolding => "Scaffolding...",
                            ExecutionPhase::Running => "Starting pipeline...",
                        }}</span>
                    </div>
                }.into_any()
            } else {
                view! {
                    <button
                        class="btn btn-approve"
                        disabled=move || !enabled() || is_loading()
                        on:click=on_click
                    >
                        "Approve & Execute"
                    </button>
                }.into_any()
            }
        }}
    }
}
