use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use crate::components::execution_node::{ExecutionNode, NodeStatus};

#[cfg(feature = "hydrate")]
use gloo_net::eventsource::futures::EventSource;
#[cfg(feature = "hydrate")]
use serde::{Deserialize, Serialize};

/// Pipeline event from SSE endpoint
#[cfg(feature = "hydrate")]
#[derive(Serialize, Deserialize, Clone, Debug)]
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
    #[serde(default)]
    events: Vec<String>, // For state_sync event
}

#[derive(Clone, Debug)]
struct ExecutionNodeData {
    node_id: String,
    label: String,
    status: NodeStatus,
    content: String,
    cost: f64,
}

#[component]
pub fn ExecutionPage() -> impl IntoView {
    let query = use_query_map();

    // Get session_id from query params
    let session_id = move || query.get().get("session_id").map(|s| s.to_string());

    // Reactive signals for execution state
    #[allow(unused_variables)]
    let (execution_nodes, set_execution_nodes) = signal(Vec::<ExecutionNodeData>::new());
    #[allow(unused_variables)]
    let (total_cost, set_total_cost) = signal(0.0);
    let (is_running, set_is_running) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (has_started, set_has_started) = signal(false);
    #[allow(unused_variables)]
    let (reconnected, set_reconnected) = signal(false);

    // Connect to SSE stream when session_id is available
    Effect::new(move || {
        if let Some(sid) = session_id() {
            tracing::info!("ExecutionPage: Connecting to SSE for session {}", sid);

            #[cfg(feature = "hydrate")]
            {
                leptos::task::spawn_local(async move {
                    use futures::StreamExt as _;

                    let url = format!("/api/stream/{}", sid);
                    tracing::info!("Connecting to SSE: {}", url);

                    match EventSource::new(&url) {
                        Ok(mut es) => {
                            let mut stream = es.subscribe("message").unwrap();

                            while let Some(Ok((_, msg))) = stream.next().await {
                                if let Some(data_str) = msg.data().as_string() {
                                    if let Ok(event) =
                                        serde_json::from_str::<PipelineEvent>(&data_str)
                                    {
                                        match event.event_type.as_str() {
                                            "state_sync" => {
                                                // Restore state from buffered events
                                                tracing::info!(
                                                    "Restoring state from {} events",
                                                    event.events.len()
                                                );
                                                set_reconnected.set(true);

                                                // Process each buffered event
                                                for event_str in &event.events {
                                                    if let Ok(evt) =
                                                        serde_json::from_str::<PipelineEvent>(
                                                            event_str,
                                                        )
                                                    {
                                                        process_pipeline_event(
                                                            evt,
                                                            set_execution_nodes,
                                                            set_total_cost,
                                                            set_is_running,
                                                            set_error,
                                                        );
                                                    }
                                                }

                                                // Clear reconnected notification after 3 seconds
                                                let reconnected_handle = set_reconnected;
                                                leptos::task::spawn_local(async move {
                                                    gloo_timers::future::TimeoutFuture::new(3000)
                                                        .await;
                                                    reconnected_handle.set(false);
                                                });
                                            }
                                            _ => {
                                                process_pipeline_event(
                                                    event,
                                                    set_execution_nodes,
                                                    set_total_cost,
                                                    set_is_running,
                                                    set_error,
                                                );
                                            }
                                        }
                                    }
                                }
                            }

                            es.close();
                        }
                        Err(e) => {
                            tracing::error!("Failed to create EventSource: {:?}", e);
                            set_error.set(Some(format!("Failed to connect to SSE: {:?}", e)));
                        }
                    }
                });
            }
        }
    });

    // Handler for Execute button
    let on_execute = move |_| {
        #[allow(unused_variables)]
        if let Some(sid) = session_id() {
            set_is_running.set(true);
            set_has_started.set(true);
            set_error.set(None);

            // Call start_pipeline server function
            #[cfg(feature = "hydrate")]
            {
                leptos::task::spawn_local(async move {
                    // Import server function from generated code
                    use crate::server::{PipelineExecutionConfig, start_pipeline};

                    // TODO: Get actual DOT graph from somewhere (for now, placeholder)
                    let config = PipelineExecutionConfig {
                        dot_graph: "digraph { start -> end; }".to_string(),
                        workdir: None,
                    };

                    match start_pipeline(sid.clone(), config).await {
                        Ok(response) => {
                            tracing::info!("Pipeline started: {:?}", response);
                        }
                        Err(e) => {
                            tracing::error!("Failed to start pipeline: {:?}", e);
                            set_error.set(Some(format!("Failed to start pipeline: {:?}", e)));
                            set_is_running.set(false);
                        }
                    }
                });
            }
        }
    };

    view! {
        <div class="execution-page">
            <h1>"Pipeline Execution"</h1>

            {move || {
                if reconnected.get() {
                    view! {
                        <div class="execution-reconnected">
                            <p>"✓ Connection restored - state recovered"</p>
                        </div>
                    }
                    .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            {move || {
                if let Some(sid) = session_id() {
                    view! {
                        <div class="execution-info">
                            <p>
                                <strong>"Session ID: "</strong>
                                {sid}
                            </p>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <div class="execution-error">
                            <p>"No session ID provided. Navigate from the editor page."</p>
                        </div>
                    }
                    .into_any()
                }
            }}

            {move || {
                if let Some(err) = error.get() {
                    view! {
                        <div class="execution-error">
                            <p>
                                <strong>"Error: "</strong>
                                {err}
                            </p>
                        </div>
                    }
                    .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            <div class="execution-controls">
                <button
                    class="execute-button"
                    on:click=on_execute
                    disabled=move || has_started.get()
                >
                    {move || {
                        if is_running.get() {
                            "Running..."
                        } else if has_started.get() {
                            "Completed"
                        } else {
                            "Execute Pipeline"
                        }
                    }}

                </button>

                <div class="cost-summary">
                    <strong>"Total Cost: "</strong>
                    {move || format!("${:.2}", total_cost.get())}
                </div>
            </div>

            <div class="execution-nodes">
                <For
                    each=move || execution_nodes.get()
                    key=|node| node.node_id.clone()
                    children=move |node: ExecutionNodeData| {
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

/// Parse status string to NodeStatus enum
#[allow(dead_code)]
fn parse_status(status: &str) -> NodeStatus {
    match status {
        "Success" => NodeStatus::Success,
        "Failed" => NodeStatus::Failed,
        "Skipped" => NodeStatus::Skipped,
        _ => NodeStatus::Success, // Default to success for unknown statuses
    }
}

/// Process a pipeline event and update signals
#[cfg(feature = "hydrate")]
fn process_pipeline_event(
    event: PipelineEvent,
    set_execution_nodes: WriteSignal<Vec<ExecutionNodeData>>,
    set_total_cost: WriteSignal<f64>,
    set_is_running: WriteSignal<bool>,
    set_error: WriteSignal<Option<String>>,
) {
    match event.event_type.as_str() {
        "node_start" => {
            tracing::info!("Node started: {}", event.node_id);
            set_execution_nodes.update(|nodes| {
                // Check if node already exists (from state restore)
                if !nodes.iter().any(|n| n.node_id == event.node_id) {
                    nodes.push(ExecutionNodeData {
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
            tracing::info!("Node completed: {}", event.node_id);
            set_execution_nodes.update(|nodes| {
                if let Some(node) = nodes.iter_mut().find(|n| n.node_id == event.node_id) {
                    node.status = parse_status(&event.status);
                    node.cost = event.cost_usd;
                    node.content = event.notes.clone();
                }
            });
            set_total_cost.set(event.cost_usd);
        }
        "pipeline_complete" => {
            tracing::info!("Pipeline complete");
            set_is_running.set(false);
        }
        "error" => {
            tracing::error!("Pipeline error: {}", event.message);
            set_error.set(Some(event.message.clone()));
            set_is_running.set(false);
        }
        _ => {}
    }
}
