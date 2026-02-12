use leptos::prelude::*;

use crate::components::execution_node::{ExecutionNode, NodeStatus};

#[cfg(feature = "hydrate")]
use gloo_net::eventsource::futures::EventSource;
#[cfg(feature = "hydrate")]
use serde::Deserialize;

#[cfg(feature = "hydrate")]
#[derive(Deserialize, Clone, Debug)]
struct PipelineEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    node_id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    cost_usd: f64,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    message: String,
}

#[derive(Clone, Debug)]
struct NodeData {
    node_id: String,
    label: String,
    status: NodeStatus,
    content: String,
    cost: f64,
}

/// Pipeline execution progress panel.
/// Subscribes to SSE at `/api/stream/{session_id}` and shows live node progress.
#[component]
pub fn ExecutionPanel<F>(session_id: F) -> impl IntoView
where
    F: Fn() -> String + Send + Sync + Copy + 'static,
{
    #[allow(unused_variables)]
    let (nodes, set_nodes) = signal(Vec::<NodeData>::new());
    #[allow(unused_variables)]
    let (total_cost, set_total_cost) = signal(0.0_f64);
    let (is_running, set_is_running) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);

    // Connect to SSE stream
    #[cfg(feature = "hydrate")]
    {
        Effect::new(move || {
            let sid = session_id();
            if sid.is_empty() {
                return;
            }

            leptos::task::spawn_local(async move {
                use futures::StreamExt as _;

                let url = format!("/api/stream/{}", sid);
                match EventSource::new(&url) {
                    Ok(mut es) => {
                        let mut stream = es.subscribe("message").unwrap();

                        while let Some(Ok((_, msg))) = stream.next().await {
                            if let Some(data_str) = msg.data().as_string() {
                                if let Ok(event) =
                                    serde_json::from_str::<PipelineEvent>(&data_str)
                                {
                                    process_event(
                                        event,
                                        set_nodes,
                                        set_total_cost,
                                        set_is_running,
                                        set_error,
                                    );
                                }
                            }
                        }

                        es.close();
                    }
                    Err(e) => {
                        tracing::error!("Failed to connect to pipeline SSE: {:?}", e);
                        set_error.set(Some(format!("Connection failed: {:?}", e)));
                        set_is_running.set(false);
                    }
                }
            });
        });
    }

    view! {
        <div class="execution-panel">
            <div class="execution-header">
                <h2>"Execution Progress"</h2>
                <div class="execution-stats">
                    {move || if is_running.get() {
                        view! { <span class="badge badge-running">"Running"</span> }.into_any()
                    } else {
                        view! { <span class="badge badge-done">"Done"</span> }.into_any()
                    }}
                    <span class="cost-display">
                        "Cost: "
                        {move || format!("${:.2}", total_cost.get())}
                    </span>
                </div>
            </div>

            {move || error.get().map(|err| view! {
                <div class="execution-error">
                    <p>{err}</p>
                </div>
            })}

            <div class="execution-nodes">
                <For
                    each=move || nodes.get()
                    key=|node| node.node_id.clone()
                    children=move |node: NodeData| {
                        view! {
                            <ExecutionNode
                                _node_id=node.node_id.clone()
                                label=node.label.clone()
                                status=node.status.clone()
                                content=node.content.clone()
                                cost=node.cost
                            />
                        }
                    }
                />
            </div>
        </div>
    }
}

#[cfg(feature = "hydrate")]
fn process_event(
    event: PipelineEvent,
    set_nodes: WriteSignal<Vec<NodeData>>,
    set_total_cost: WriteSignal<f64>,
    set_is_running: WriteSignal<bool>,
    set_error: WriteSignal<Option<String>>,
) {
    match event.event_type.as_str() {
        "node_start" => {
            set_nodes.update(|nodes| {
                if !nodes.iter().any(|n| n.node_id == event.node_id) {
                    nodes.push(NodeData {
                        node_id: event.node_id.clone(),
                        label: event.label.clone(),
                        status: NodeStatus::InProgress,
                        content: String::new(),
                        cost: 0.0,
                    });
                }
            });
        }
        "node_complete" => {
            set_nodes.update(|nodes| {
                if let Some(node) = nodes.iter_mut().find(|n| n.node_id == event.node_id) {
                    node.status = match event.status.as_str() {
                        "Success" => NodeStatus::Success,
                        "Failed" | "Fail" => NodeStatus::Failed,
                        "Skipped" => NodeStatus::Skipped,
                        _ => NodeStatus::Success,
                    };
                    node.cost = event.cost_usd;
                    node.content = event.notes.clone();
                }
            });
            set_total_cost.set(event.cost_usd);
        }
        "pipeline_complete" => {
            set_is_running.set(false);
        }
        "error" => {
            set_error.set(Some(event.message.clone()));
            set_is_running.set(false);
        }
        _ => {}
    }
}
