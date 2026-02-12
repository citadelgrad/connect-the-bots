use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeStatus {
    Pending,
    InProgress,
    Success,
    Failed,
    Skipped,
}

#[component]
pub fn ExecutionNode(
    _node_id: String,
    label: String,
    status: NodeStatus,
    content: String,
    cost: f64,
) -> impl IntoView {
    let (expanded, set_expanded) = signal(false);

    let status_class = match status {
        NodeStatus::Pending => "pending",
        NodeStatus::InProgress => "in-progress",
        NodeStatus::Success => "success",
        NodeStatus::Failed => "failed",
        NodeStatus::Skipped => "skipped",
    };

    let status_icon = match status {
        NodeStatus::Pending => "○",
        NodeStatus::InProgress => "●",
        NodeStatus::Success => "✓",
        NodeStatus::Failed => "✗",
        NodeStatus::Skipped => "○",
    };

    view! {
        <div class=format!("execution-node {}", status_class)>
            <div
                class="execution-node-header"
                on:click=move |_| set_expanded.update(|e| *e = !*e)
            >
                <span class="execution-node-status">{status_icon}</span>
                <span class="execution-node-label">{label.clone()}</span>
                <span class="execution-node-cost">{format!("${:.2}", cost)}</span>
            </div>

            {move || {
                if expanded.get() && !content.is_empty() {
                    view! {
                        <div class="execution-node-content">
                            <pre>{content.clone()}</pre>
                        </div>
                    }
                    .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
        </div>
    }
}
