//! Pipeline execution server function with SSE streaming.
//!
//! Provides the `start_pipeline` server function to launch background pipeline execution
//! and stream real-time progress events via SSE.

use leptos::prelude::*;
use leptos::server_fn::error::NoCustomError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PipelineExecutionConfig {
    pub dot_graph: String,
    pub workdir: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StartPipelineResponse {
    pub session_id: String,
    pub status: String,
}

/// Start pipeline execution in the background with SSE streaming.
///
/// This function:
/// 1. Validates the DOT graph
/// 2. Spawns a background task to execute the pipeline
/// 3. Streams progress events to SSE endpoint
/// 4. Returns immediately with session_id
#[server]
pub async fn start_pipeline(
    session_id: String,
    config: PipelineExecutionConfig,
) -> Result<StartPipelineResponse, ServerFnError<NoCustomError>> {
    use attractor_dot::parse;
    use attractor_pipeline::PipelineGraph;

    tracing::info!(
        "Starting pipeline execution for session: {}",
        session_id
    );

    // Parse and validate DOT graph
    let parsed = parse(&config.dot_graph).map_err(|e| {
        ServerFnError::<NoCustomError>::ServerError(format!("Failed to parse DOT graph: {}", e))
    })?;

    let graph = PipelineGraph::from_dot(parsed).map_err(|e| {
        ServerFnError::<NoCustomError>::ServerError(format!("Failed to create pipeline graph: {}", e))
    })?;

    // Validate graph structure
    attractor_pipeline::validate_or_raise(&graph).map_err(|e| {
        ServerFnError::<NoCustomError>::ServerError(format!("Invalid pipeline graph: {}", e))
    })?;

    // Clone data for background task
    let session_id_clone = session_id.clone();
    let workdir = config.workdir.clone();

    // Spawn background execution task
    tokio::spawn(async move {
        if let Err(e) = execute_pipeline_with_streaming(&graph, &session_id_clone, workdir).await {
            tracing::error!("Pipeline execution failed: {:?}", e);

            // Publish error event
            let error_event = serde_json::json!({
                "type": "error",
                "message": format!("Pipeline execution failed: {}", e),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            crate::server::stream::publish_event(
                &session_id_clone,
                serde_json::to_string(&error_event).unwrap_or_default(),
            );
        }
    });

    Ok(StartPipelineResponse {
        session_id,
        status: "started".to_string(),
    })
}

/// Execute pipeline with streaming progress events.
///
/// This is a custom execution loop that wraps the existing handler infrastructure
/// and emits SSE events at node boundaries.
#[cfg(feature = "ssr")]
async fn execute_pipeline_with_streaming(
    graph: &attractor_pipeline::PipelineGraph,
    session_id: &str,
    workdir: Option<String>,
) -> Result<attractor_pipeline::PipelineResult, attractor_types::AttractorError> {
    use attractor_pipeline::{default_registry, select_edge};
    use attractor_types::{AttractorError, Context};
    use std::collections::HashMap;

    let registry = default_registry();
    let context = Context::new();

    // Set workdir if provided
    if let Some(dir) = workdir {
        context.set("workdir", serde_json::Value::String(dir)).await;
    }

    // Initialize context from graph attrs
    for (key, val) in &graph.attrs {
        context.set(key, attr_to_json(val)).await;
    }

    let mut completed_nodes = Vec::new();
    let mut node_outcomes = HashMap::new();
    let mut current_node = graph
        .start_node()
        .ok_or_else(|| AttractorError::Other("No start node found".into()))?;
    let mut total_cost = 0.0;

    loop {
        // Emit node_start event
        let event = serde_json::json!({
            "type": "node_start",
            "node_id": current_node.id,
            "label": current_node.label,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        crate::server::stream::publish_event(
            session_id,
            serde_json::to_string(&event).unwrap_or_default(),
        );

        // Execute node via handler
        let handler_type = registry.resolve_type(current_node);
        let handler = registry.get(&handler_type).ok_or_else(|| {
            AttractorError::HandlerError {
                handler: handler_type.clone(),
                node: current_node.id.clone(),
                message: format!("No handler registered for type '{}'", handler_type),
            }
        })?;

        let outcome = handler.execute(current_node, &context, graph).await?;

        // Track cost
        if let Some(cost) = outcome
            .context_updates
            .get(&format!("{}.cost_usd", current_node.id))
        {
            if let Some(c) = cost.as_f64() {
                total_cost += c;
            }
        }

        // Emit node_complete event
        let event = serde_json::json!({
            "type": "node_complete",
            "node_id": current_node.id,
            "status": format!("{:?}", outcome.status),
            "cost_usd": total_cost,
            "notes": outcome.notes,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        crate::server::stream::publish_event(
            session_id,
            serde_json::to_string(&event).unwrap_or_default(),
        );

        // Record outcome
        completed_nodes.push(current_node.id.clone());
        node_outcomes.insert(current_node.id.clone(), outcome.clone());

        // Apply context updates
        context.apply_updates(outcome.context_updates.clone()).await;
        context
            .set(
                "outcome",
                serde_json::Value::String(status_to_string(outcome.status)),
            )
            .await;
        if let Some(ref label) = outcome.preferred_label {
            context
                .set(
                    "preferred_label",
                    serde_json::Value::String(label.clone()),
                )
                .await;
        }

        // Terminal check (exit node)
        if current_node.shape == "Msquare" {
            break;
        }

        // Select next edge
        let resolve = |key: &str| -> String {
            match key {
                "outcome" => format!("{:?}", outcome.status),
                "preferred_label" => outcome.preferred_label.clone().unwrap_or_default(),
                _ => String::new(),
            }
        };
        let next_edge = select_edge(&current_node.id, &outcome, &resolve, graph);

        match next_edge {
            Some(edge) => {
                let next_id = edge.to.clone();
                current_node = graph.node(&next_id).ok_or_else(|| {
                    AttractorError::Other(format!("Edge target '{}' not found", next_id))
                })?;
            }
            None => {
                return Err(AttractorError::Other(
                    "No outgoing edge from non-terminal node".into(),
                ));
            }
        }
    }

    // Emit pipeline_complete event
    let event = serde_json::json!({
        "type": "pipeline_complete",
        "total_cost_usd": total_cost,
        "completed_nodes": completed_nodes,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    crate::server::stream::publish_event(
        session_id,
        serde_json::to_string(&event).unwrap_or_default(),
    );

    Ok(attractor_pipeline::PipelineResult {
        completed_nodes,
        node_outcomes,
        final_context: context.snapshot().await,
    })
}

/// Convert DOT attribute to JSON value.
#[cfg(feature = "ssr")]
fn attr_to_json(attr: &attractor_dot::AttributeValue) -> serde_json::Value {
    use attractor_dot::AttributeValue;
    match attr {
        AttributeValue::String(s) => serde_json::Value::String(s.clone()),
        AttributeValue::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        AttributeValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        AttributeValue::Boolean(b) => serde_json::Value::Bool(*b),
        AttributeValue::Duration(d) => {
            serde_json::Value::Number(serde_json::Number::from(d.as_secs()))
        }
    }
}

/// Map a `StageStatus` to the lowercase string used in edge conditions.
#[cfg(feature = "ssr")]
fn status_to_string(status: attractor_types::StageStatus) -> String {
    use attractor_types::StageStatus;
    match status {
        StageStatus::Success => "success".to_string(),
        StageStatus::PartialSuccess => "partial_success".to_string(),
        StageStatus::Retry => "retry".to_string(),
        StageStatus::Fail => "fail".to_string(),
        StageStatus::Skipped => "skipped".to_string(),
    }
}
