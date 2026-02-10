//! Pipeline execution engine — the core traversal loop.
//!
//! Implements the 5-phase lifecycle: parse, validate, initialize, execute, finalize.

use std::collections::HashMap;
use std::path::PathBuf;

use attractor_types::{AttractorError, Context, Outcome, Result, StageStatus};

use crate::edge_selection::select_edge;
use crate::goal_gate::enforce_goal_gates;
use crate::graph::PipelineGraph;
use crate::handler::{default_registry, HandlerRegistry};
use crate::validation::validate_or_raise;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The core pipeline executor. Owns a handler registry and drives graph traversal.
pub struct PipelineExecutor {
    registry: HandlerRegistry,
}

/// Configuration for a pipeline run.
pub struct PipelineConfig {
    pub logs_root: PathBuf,
}

/// The result of a completed pipeline execution.
#[derive(Debug)]
pub struct PipelineResult {
    pub completed_nodes: Vec<String>,
    pub node_outcomes: HashMap<String, Outcome>,
    pub final_context: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert an `attractor_dot::AttributeValue` to a `serde_json::Value`.
fn attr_to_json(val: &attractor_dot::AttributeValue) -> serde_json::Value {
    match val {
        attractor_dot::AttributeValue::String(s) => serde_json::Value::String(s.clone()),
        attractor_dot::AttributeValue::Integer(i) => serde_json::json!(*i),
        attractor_dot::AttributeValue::Float(f) => serde_json::json!(*f),
        attractor_dot::AttributeValue::Boolean(b) => serde_json::Value::Bool(*b),
        attractor_dot::AttributeValue::Duration(d) => serde_json::json!(d.as_millis() as u64),
    }
}

/// Map a `StageStatus` to the lowercase string used in edge conditions.
fn status_to_string(status: StageStatus) -> String {
    match status {
        StageStatus::Success => "success".to_string(),
        StageStatus::PartialSuccess => "partial_success".to_string(),
        StageStatus::Retry => "retry".to_string(),
        StageStatus::Fail => "fail".to_string(),
        StageStatus::Skipped => "skipped".to_string(),
    }
}

// ---------------------------------------------------------------------------
// PipelineExecutor
// ---------------------------------------------------------------------------

impl PipelineExecutor {
    /// Create an executor with the given handler registry.
    pub fn new(registry: HandlerRegistry) -> Self {
        Self { registry }
    }

    /// Create an executor pre-loaded with the default built-in handlers.
    pub fn with_default_registry() -> Self {
        Self {
            registry: default_registry(),
        }
    }

    /// Run the full 5-phase pipeline lifecycle on the given graph.
    pub async fn run(&self, graph: &PipelineGraph) -> Result<PipelineResult> {
        // Phase 2: Validate
        validate_or_raise(graph)?;

        // Phase 3: Initialize
        let context = Context::new();
        for (key, val) in &graph.attrs {
            context.set(key, attr_to_json(val)).await;
        }
        let mut completed_nodes: Vec<String> = Vec::new();
        let mut node_outcomes: HashMap<String, Outcome> = HashMap::new();

        // Phase 4: Execute
        let start = graph.start_node().ok_or_else(|| {
            AttractorError::ValidationError("No start node found".into())
        })?;
        let mut current_node = start;

        loop {
            // Terminal check (exit node)
            if current_node.shape == "Msquare" {
                // Check goal gates
                let gate_result = enforce_goal_gates(graph, &node_outcomes)?;
                if !gate_result.all_satisfied {
                    if let Some(ref target) = gate_result.retry_target {
                        current_node = graph.node(target).ok_or_else(|| {
                            AttractorError::Other(format!(
                                "Retry target '{}' not found",
                                target
                            ))
                        })?;
                        continue;
                    }
                }

                // Execute the exit handler
                let handler_type = self.registry.resolve_type(current_node);
                let handler = self.registry.get(&handler_type).ok_or_else(|| {
                    AttractorError::HandlerError {
                        handler: handler_type.clone(),
                        node: current_node.id.clone(),
                        message: format!("No handler registered for type '{}'", handler_type),
                    }
                })?;
                let outcome = handler.execute(current_node, &context, graph).await?;
                completed_nodes.push(current_node.id.clone());
                node_outcomes.insert(current_node.id.clone(), outcome);
                break;
            }

            // Execute handler
            let handler_type = self.registry.resolve_type(current_node);
            let handler = self.registry.get(&handler_type).ok_or_else(|| {
                AttractorError::HandlerError {
                    handler: handler_type.clone(),
                    node: current_node.id.clone(),
                    message: format!("No handler registered for type '{}'", handler_type),
                }
            })?;
            let outcome = handler.execute(current_node, &context, graph).await?;

            // Record
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

            // Select next edge
            let resolve = |key: &str| -> String {
                match key {
                    "outcome" => status_to_string(outcome.status),
                    "preferred_label" => outcome.preferred_label.clone().unwrap_or_default(),
                    _ => String::new(),
                }
            };
            let next_edge = select_edge(&current_node.id, &outcome, &resolve, graph);

            match next_edge {
                Some(edge) => {
                    // Handle loop_restart
                    if edge.loop_restart {
                        completed_nodes.clear();
                        node_outcomes.clear();
                    }
                    let next_id = edge.to.clone();
                    current_node = graph.node(&next_id).ok_or_else(|| {
                        AttractorError::Other(format!("Edge target '{}' not found", next_id))
                    })?;
                }
                None => {
                    // No outgoing edge and not an exit node
                    if outcome.status == StageStatus::Fail {
                        return Err(AttractorError::HandlerError {
                            handler: handler_type,
                            node: current_node.id.clone(),
                            message: "Handler failed with no outgoing edge".into(),
                        });
                    }
                    break;
                }
            }
        }

        // Phase 5: Finalize
        let final_context = context.snapshot().await;
        Ok(PipelineResult {
            completed_nodes,
            node_outcomes,
            final_context,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::PipelineGraph;

    fn parse_graph(dot: &str) -> PipelineGraph {
        let parsed = attractor_dot::parse(dot).unwrap();
        PipelineGraph::from_dot(parsed).unwrap()
    }

    // Test 1: Linear pipeline (start -> A -> exit) completes successfully
    #[tokio::test]
    async fn linear_pipeline_completes() {
        let graph = parse_graph(
            r#"digraph G {
                start [shape="Mdiamond"]
                process [shape="box", label="Process", prompt="Do work"]
                done [shape="Msquare"]
                start -> process -> done
            }"#,
        );
        let executor = PipelineExecutor::with_default_registry();
        let result = executor.run(&graph).await.unwrap();

        assert_eq!(result.completed_nodes, vec!["start", "process", "done"]);
        assert!(result.node_outcomes.contains_key("start"));
        assert!(result.node_outcomes.contains_key("process"));
        assert!(result.node_outcomes.contains_key("done"));
        assert_eq!(
            result.node_outcomes["start"].status,
            StageStatus::Success
        );
        assert_eq!(
            result.node_outcomes["process"].status,
            StageStatus::Success
        );
        assert_eq!(
            result.node_outcomes["done"].status,
            StageStatus::Success
        );
    }

    // Test 2: Branching pipeline routes based on conditions
    #[tokio::test]
    async fn branching_pipeline_routes_on_condition() {
        // The codergen handler returns Success, so outcome=success.
        // Edge to "yes_path" has condition="outcome=success", so it should be taken.
        let graph = parse_graph(
            r#"digraph G {
                start [shape="Mdiamond"]
                check [shape="box", label="Check", prompt="Check something"]
                yes_path [shape="box", label="Yes Path", prompt="Yes"]
                no_path [shape="box", label="No Path", prompt="No"]
                done [shape="Msquare"]
                start -> check
                check -> yes_path [condition="outcome=success"]
                check -> no_path [condition="outcome=fail"]
                yes_path -> done
                no_path -> done
            }"#,
        );
        let executor = PipelineExecutor::with_default_registry();
        let result = executor.run(&graph).await.unwrap();

        assert!(result.completed_nodes.contains(&"yes_path".to_string()));
        assert!(!result.completed_nodes.contains(&"no_path".to_string()));
    }

    // Test 3: Pipeline with no start node returns error
    #[tokio::test]
    async fn no_start_node_returns_error() {
        let graph = parse_graph(
            r#"digraph G {
                process [shape="box", label="Do work"]
                done [shape="Msquare"]
                process -> done
            }"#,
        );
        let executor = PipelineExecutor::with_default_registry();
        let result = executor.run(&graph).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            AttractorError::ValidationError(msg) => {
                assert!(
                    msg.contains("start node"),
                    "Expected error about start node, got: {msg}"
                );
            }
            other => panic!("Expected ValidationError, got: {other:?}"),
        }
    }

    // Test 4: Context updates from one node visible to next (verify via final_context)
    #[tokio::test]
    async fn context_updates_propagate() {
        // The codergen handler sets "last_prompt" in context_updates.
        // We verify it shows up in final_context.
        let graph = parse_graph(
            r#"digraph G {
                start [shape="Mdiamond"]
                step [shape="box", label="Step", prompt="Generate code"]
                done [shape="Msquare"]
                start -> step -> done
            }"#,
        );
        let executor = PipelineExecutor::with_default_registry();
        let result = executor.run(&graph).await.unwrap();

        // The codergen handler puts the prompt into context_updates["<node_id>.prompt"]
        assert_eq!(
            result.final_context.get("step.prompt"),
            Some(&serde_json::Value::String("Generate code".into())),
        );
        // The engine also sets "outcome" in context
        assert_eq!(
            result.final_context.get("outcome"),
            Some(&serde_json::Value::String("success".into())),
        );
    }

    // Test 5: Goal gate failure with retry target loops back
    #[tokio::test]
    async fn goal_gate_failure_with_retry_loops_back() {
        // We cannot easily make a handler fail on first call and succeed on second
        // with the default registry. Instead, we test that goal gate checking works
        // by having a goal gate node that succeeds (so no loop occurs) and verifying
        // the exit is reached.
        //
        // For a more thorough test of the retry path, we'd need a custom handler.
        // Here we at least verify the goal gate path doesn't error when gates are satisfied.
        let graph = parse_graph(
            r#"digraph G {
                start [shape="Mdiamond"]
                review [shape="box", goal_gate=true, retry_target="start", label="Review", prompt="Review code"]
                done [shape="Msquare"]
                start -> review -> done
            }"#,
        );
        let executor = PipelineExecutor::with_default_registry();
        let result = executor.run(&graph).await.unwrap();

        // Goal gate is satisfied (codergen returns success), so pipeline completes
        assert!(result.completed_nodes.contains(&"done".to_string()));
    }

    // Test 6: Goal gate failure without retry target returns error
    #[tokio::test]
    async fn goal_gate_failure_without_retry_returns_error() {
        // To test this, we need a custom handler that returns Fail for the goal gate node.
        use async_trait::async_trait;
        use crate::handler::NodeHandler;
        use crate::graph::PipelineNode;

        struct FailHandler;

        #[async_trait]
        impl NodeHandler for FailHandler {
            fn handler_type(&self) -> &str {
                "codergen"
            }
            async fn execute(
                &self,
                _node: &PipelineNode,
                _ctx: &Context,
                _graph: &PipelineGraph,
            ) -> Result<Outcome> {
                Ok(Outcome::fail("intentional failure"))
            }
        }

        let graph = parse_graph(
            r#"digraph G {
                start [shape="Mdiamond"]
                review [shape="box", goal_gate=true, label="Review", prompt="Review"]
                done [shape="Msquare"]
                start -> review -> done
            }"#,
        );

        let mut registry = HandlerRegistry::new();
        registry.register(crate::handler::StartHandler);
        registry.register(crate::handler::ExitHandler);
        registry.register(crate::handler::ConditionalHandler);
        registry.register(FailHandler);

        let executor = PipelineExecutor::new(registry);
        let result = executor.run(&graph).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            AttractorError::GoalGateUnsatisfied { node } => {
                assert_eq!(node, "review");
            }
            other => panic!("Expected GoalGateUnsatisfied, got: {other:?}"),
        }
    }

    // Test 7: Goal gate failure with retry target retries correctly
    #[tokio::test]
    async fn goal_gate_failure_with_retry_target_retries() {
        use async_trait::async_trait;
        use crate::handler::NodeHandler;
        use crate::graph::PipelineNode;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // Handler that fails on first call, succeeds on subsequent calls
        struct RetryableHandler {
            call_count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl NodeHandler for RetryableHandler {
            fn handler_type(&self) -> &str {
                "codergen"
            }
            async fn execute(
                &self,
                _node: &PipelineNode,
                _ctx: &Context,
                _graph: &PipelineGraph,
            ) -> Result<Outcome> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Ok(Outcome::fail("first attempt fails"))
                } else {
                    Ok(Outcome::success("retry succeeded"))
                }
            }
        }

        let graph = parse_graph(
            r#"digraph G {
                start [shape="Mdiamond"]
                review [shape="box", goal_gate=true, retry_target="start", label="Review", prompt="Review"]
                done [shape="Msquare"]
                start -> review -> done
            }"#,
        );

        let call_count = Arc::new(AtomicUsize::new(0));
        let mut registry = HandlerRegistry::new();
        registry.register(crate::handler::StartHandler);
        registry.register(crate::handler::ExitHandler);
        registry.register(crate::handler::ConditionalHandler);
        registry.register(RetryableHandler {
            call_count: call_count.clone(),
        });

        let executor = PipelineExecutor::new(registry);
        let result = executor.run(&graph).await.unwrap();

        // Should have retried: start -> review(fail) -> exit(goal gate fails, retry to start)
        // -> start -> review(success) -> exit(done)
        assert!(result.completed_nodes.contains(&"done".to_string()));
        // The handler was called twice (once fail, once success)
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    // Test 8: PipelineExecutor::new and with_default_registry
    #[test]
    fn executor_constructors() {
        let executor = PipelineExecutor::with_default_registry();
        assert!(executor.registry.has("start"));
        assert!(executor.registry.has("exit"));
        assert!(executor.registry.has("codergen"));

        let custom = PipelineExecutor::new(HandlerRegistry::new());
        assert!(!custom.registry.has("start"));
    }
}
