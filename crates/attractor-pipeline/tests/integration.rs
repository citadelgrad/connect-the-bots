//! End-to-end integration tests for the Attractor pipeline engine.
//!
//! Each test exercises the full pipeline: parse DOT -> build graph -> validate -> execute -> verify.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use attractor_dot::parse;
use attractor_pipeline::{
    apply_stylesheet, parse_stylesheet, validate, validate_or_raise, ConditionalHandler,
    ExitHandler, HandlerRegistry, NodeHandler, PipelineExecutor, PipelineGraph, PipelineNode,
    StartHandler,
};
use attractor_types::{Context, Outcome, StageStatus};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse DOT source into a PipelineGraph, panicking on failure.
fn build_graph(dot: &str) -> PipelineGraph {
    let parsed = parse(dot).expect("DOT parse failed");
    PipelineGraph::from_dot(parsed).expect("PipelineGraph::from_dot failed")
}

/// Build an executor with the default handler registry.
fn executor() -> PipelineExecutor {
    PipelineExecutor::with_default_registry()
}

// ---------------------------------------------------------------------------
// Test 1: Simple linear pipeline (start -> process -> done)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn simple_linear_pipeline_completes_in_order() {
    let graph = build_graph(
        r#"digraph Simple {
            start [shape="Mdiamond"]
            process [shape="box", prompt="Process data"]
            done [shape="Msquare"]
            start -> process -> done
        }"#,
    );

    // Validation should produce no errors
    let diags = validate_or_raise(&graph).expect("validation should pass");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == attractor_pipeline::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no validation errors: {errors:?}");

    // Execute
    let result = executor().run(&graph).await.expect("pipeline should succeed");

    // All 3 nodes should complete in order
    assert_eq!(
        result.completed_nodes,
        vec!["start", "process", "done"],
        "Nodes should complete in linear order"
    );

    // Each node should have a success outcome
    for node_id in &["start", "process", "done"] {
        let outcome = result
            .node_outcomes
            .get(*node_id)
            .unwrap_or_else(|| panic!("missing outcome for {node_id}"));
        assert_eq!(
            outcome.status,
            StageStatus::Success,
            "node '{node_id}' should be Success"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: Branching pipeline with conditions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn branching_pipeline_routes_via_condition() {
    let graph = build_graph(
        r#"digraph Branch {
            start [shape="Mdiamond"]
            check [shape="diamond"]
            path_a [shape="box", prompt="Path A"]
            path_b [shape="box", prompt="Path B"]
            done [shape="Msquare"]
            start -> check
            check -> path_a [condition="outcome=success"]
            check -> path_b
            path_a -> done
            path_b -> done
        }"#,
    );

    let result = executor().run(&graph).await.expect("pipeline should succeed");

    // The conditional handler returns Success, so outcome=success.
    // The edge to path_a has condition="outcome=success" which should match.
    assert!(
        result.completed_nodes.contains(&"path_a".to_string()),
        "path_a should be visited when condition matches; completed: {:?}",
        result.completed_nodes
    );
    assert!(
        !result.completed_nodes.contains(&"path_b".to_string()),
        "path_b should NOT be visited; completed: {:?}",
        result.completed_nodes
    );
    assert!(
        result.completed_nodes.contains(&"done".to_string()),
        "done should be reached"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Pipeline with goal gates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn goal_gate_satisfied_pipeline_completes() {
    let graph = build_graph(
        r#"digraph GoalGate {
            start [shape="Mdiamond"]
            review [shape="box", goal_gate=true, prompt="Review code"]
            done [shape="Msquare"]
            start -> review -> done
        }"#,
    );

    // The default codergen handler returns Success, satisfying the goal gate.
    let result = executor().run(&graph).await.expect("pipeline should succeed");

    assert!(
        result.completed_nodes.contains(&"review".to_string()),
        "review (goal gate) should be visited"
    );
    assert!(
        result.completed_nodes.contains(&"done".to_string()),
        "done should be reached after goal gate passes"
    );
    assert_eq!(
        result.node_outcomes["review"].status,
        StageStatus::Success,
        "review node should succeed"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Validation catches missing start node
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validation_catches_missing_start_node() {
    let graph = build_graph(
        r#"digraph NoStart {
            process [shape="box", prompt="Work"]
            done [shape="Msquare"]
            process -> done
        }"#,
    );

    // validate_or_raise should return an error
    let result = validate_or_raise(&graph);
    assert!(result.is_err(), "validation should fail without a start node");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("start node"),
        "error should mention start node; got: {err_msg}"
    );

    // Also verify the advisory validate() produces an Error-level diagnostic
    let diags = validate(&graph);
    assert!(
        diags.iter().any(|d| d.rule == "start_node"
            && d.severity == attractor_pipeline::Severity::Error),
        "Expected start_node error diagnostic; got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 5: Stylesheet application
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stylesheet_applies_model_to_nodes() {
    let mut graph = build_graph(
        r#"digraph Styled {
            start [shape="Mdiamond"]
            analyze [shape="box", prompt="Analyze", class="fast"]
            summarize [shape="box", prompt="Summarize", class="slow"]
            done [shape="Msquare"]
            start -> analyze -> summarize -> done
        }"#,
    );

    let css = r#"
        * { llm_model: default-model; llm_provider: anthropic; }
        .fast { llm_model: fast-model; }
        #summarize { llm_model: summarize-model; reasoning_effort: high; }
    "#;
    let stylesheet = parse_stylesheet(css).expect("stylesheet parse should succeed");
    apply_stylesheet(&mut graph, &stylesheet);

    // Universal rule sets defaults on all nodes
    let start_node = graph.node("start").unwrap();
    assert_eq!(
        start_node.llm_model.as_deref(),
        Some("default-model"),
        "start should get universal model"
    );
    assert_eq!(
        start_node.llm_provider.as_deref(),
        Some("anthropic"),
        "start should get universal provider"
    );

    // .fast class overrides universal for analyze
    let analyze_node = graph.node("analyze").unwrap();
    assert_eq!(
        analyze_node.llm_model.as_deref(),
        Some("fast-model"),
        "analyze should get .fast class model"
    );

    // #summarize ID selector overrides .slow class and universal
    let summarize_node = graph.node("summarize").unwrap();
    assert_eq!(
        summarize_node.llm_model.as_deref(),
        Some("summarize-model"),
        "summarize should get ID-specific model"
    );
    assert_eq!(
        summarize_node.reasoning_effort.as_deref(),
        Some("high"),
        "summarize should get reasoning_effort from ID selector"
    );

    // The graph should still be valid and executable after stylesheet application
    let result = PipelineExecutor::with_default_registry()
        .run(&graph)
        .await
        .expect("styled pipeline should execute");
    assert_eq!(result.completed_nodes.len(), 4);
}

// ---------------------------------------------------------------------------
// Test 6: Context propagation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_propagation_across_nodes() {
    let graph = build_graph(
        r#"digraph ContextTest {
            start [shape="Mdiamond"]
            step_one [shape="box", prompt="First step"]
            step_two [shape="box", prompt="Second step"]
            done [shape="Msquare"]
            start -> step_one -> step_two -> done
        }"#,
    );

    let result = executor().run(&graph).await.expect("pipeline should succeed");

    // The codergen handler sets "<node_id>.prompt" and "<node_id>.completed" in context_updates.
    // These should propagate through the engine into final_context.
    assert_eq!(
        result.final_context.get("step_one.prompt"),
        Some(&serde_json::json!("First step")),
        "step_one.prompt should be in final context"
    );
    assert_eq!(
        result.final_context.get("step_one.completed"),
        Some(&serde_json::json!(true)),
        "step_one.completed should be in final context"
    );
    assert_eq!(
        result.final_context.get("step_two.prompt"),
        Some(&serde_json::json!("Second step")),
        "step_two.prompt should be in final context"
    );
    assert_eq!(
        result.final_context.get("step_two.completed"),
        Some(&serde_json::json!(true)),
        "step_two.completed should be in final context"
    );

    // Engine sets "outcome" to the status string of the last non-exit node
    assert_eq!(
        result.final_context.get("outcome"),
        Some(&serde_json::json!("success")),
        "outcome should be set in final context"
    );
}

// ---------------------------------------------------------------------------
// Test 7: Pipeline with many nodes (10-node linear chain)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ten_node_linear_pipeline_completes() {
    // Programmatically build a 10-node linear DOT graph
    let mut dot = String::from("digraph ManyNodes {\n");
    dot.push_str("    start [shape=\"Mdiamond\"]\n");
    for i in 1..=8 {
        dot.push_str(&format!(
            "    step_{i} [shape=\"box\", prompt=\"Step {i}\"]\n"
        ));
    }
    dot.push_str("    done [shape=\"Msquare\"]\n");

    // Edges: start -> step_1 -> step_2 -> ... -> step_8 -> done
    dot.push_str("    start -> step_1\n");
    for i in 1..8 {
        dot.push_str(&format!("    step_{i} -> step_{}\n", i + 1));
    }
    dot.push_str("    step_8 -> done\n");
    dot.push_str("}\n");

    let graph = build_graph(&dot);

    // Validate
    let diags = validate_or_raise(&graph).expect("10-node graph should validate");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == attractor_pipeline::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "No validation errors expected: {errors:?}");

    // Execute
    let result = executor().run(&graph).await.expect("pipeline should succeed");

    // Total: start + 8 steps + done = 10 nodes
    assert_eq!(
        result.completed_nodes.len(),
        10,
        "All 10 nodes should complete; got: {:?}",
        result.completed_nodes
    );

    // Verify ordering: start first, done last
    assert_eq!(result.completed_nodes[0], "start");
    assert_eq!(result.completed_nodes[9], "done");

    // Verify all step nodes are present
    for i in 1..=8 {
        let node_id = format!("step_{i}");
        assert!(
            result.completed_nodes.contains(&node_id),
            "missing {node_id}"
        );
    }

    // All outcomes should be Success
    for (id, outcome) in &result.node_outcomes {
        assert_eq!(
            outcome.status,
            StageStatus::Success,
            "node '{id}' should succeed"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 8: Edge selection priority (weighted, labeled, condition edges)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edge_selection_respects_condition_over_weight() {
    // When a condition matches, it takes priority over weight.
    // check -> low_weight has condition="outcome=success" (matches because conditional handler
    // returns success), but low weight.
    // check -> high_weight has higher weight but no condition.
    // Condition match should win.
    let graph = build_graph(
        r#"digraph EdgePriority {
            start [shape="Mdiamond"]
            check [shape="diamond"]
            low_weight [shape="box", prompt="Low weight path"]
            high_weight [shape="box", prompt="High weight path"]
            done [shape="Msquare"]
            start -> check
            check -> low_weight [condition="outcome=success", weight=1]
            check -> high_weight [weight=100]
            low_weight -> done
            high_weight -> done
        }"#,
    );

    let result = executor().run(&graph).await.expect("pipeline should succeed");

    assert!(
        result.completed_nodes.contains(&"low_weight".to_string()),
        "condition match should win over weight; completed: {:?}",
        result.completed_nodes
    );
    assert!(
        !result.completed_nodes.contains(&"high_weight".to_string()),
        "high_weight should not be taken; completed: {:?}",
        result.completed_nodes
    );
}

// ---------------------------------------------------------------------------
// Test 9: Goal gate failure without retry target returns error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn goal_gate_unsatisfied_without_retry_returns_error() {
    // Use a custom handler that always returns Fail for the codergen type.
    struct AlwaysFailHandler;

    #[async_trait]
    impl NodeHandler for AlwaysFailHandler {
        fn handler_type(&self) -> &str {
            "codergen"
        }
        async fn execute(
            &self,
            _node: &PipelineNode,
            _ctx: &Context,
            _graph: &PipelineGraph,
        ) -> attractor_types::Result<Outcome> {
            Ok(Outcome::fail("intentional failure for test"))
        }
    }

    let graph = build_graph(
        r#"digraph GoalGateFail {
            start [shape="Mdiamond"]
            review [shape="box", goal_gate=true, prompt="Review code"]
            done [shape="Msquare"]
            start -> review -> done
        }"#,
    );

    let mut registry = HandlerRegistry::new();
    registry.register(StartHandler);
    registry.register(ExitHandler);
    registry.register(ConditionalHandler);
    registry.register(AlwaysFailHandler);

    let exec = PipelineExecutor::new(registry);
    let result = exec.run(&graph).await;

    assert!(result.is_err(), "pipeline should fail with unsatisfied goal gate");
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("Goal gate unsatisfied") || err_msg.contains("goal_gate"),
        "error should mention goal gate; got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// Test 10: Goal gate with retry target loops back and eventually succeeds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn goal_gate_with_retry_target_retries_then_succeeds() {
    // A handler that fails on the first call but succeeds on subsequent calls.
    struct FailOnceThenSucceedHandler {
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl NodeHandler for FailOnceThenSucceedHandler {
        fn handler_type(&self) -> &str {
            "codergen"
        }
        async fn execute(
            &self,
            node: &PipelineNode,
            _ctx: &Context,
            _graph: &PipelineGraph,
        ) -> attractor_types::Result<Outcome> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                Ok(Outcome::fail("first attempt fails"))
            } else {
                let mut updates = HashMap::new();
                updates.insert(
                    format!("{}.completed", node.id),
                    serde_json::json!(true),
                );
                Ok(Outcome {
                    status: StageStatus::Success,
                    preferred_label: None,
                    suggested_next_ids: vec![],
                    context_updates: updates,
                    notes: "retry succeeded".into(),
                    failure_reason: None,
                })
            }
        }
    }

    let graph = build_graph(
        r#"digraph GoalGateRetry {
            start [shape="Mdiamond"]
            review [shape="box", goal_gate=true, retry_target="start", prompt="Review"]
            done [shape="Msquare"]
            start -> review -> done
        }"#,
    );

    let call_count = Arc::new(AtomicUsize::new(0));
    let mut registry = HandlerRegistry::new();
    registry.register(StartHandler);
    registry.register(ExitHandler);
    registry.register(ConditionalHandler);
    registry.register(FailOnceThenSucceedHandler {
        call_count: call_count.clone(),
    });

    let exec = PipelineExecutor::new(registry);
    let result = exec.run(&graph).await.expect("pipeline should succeed after retry");

    // The handler was called at least twice (once fail, once success)
    assert!(
        call_count.load(Ordering::SeqCst) >= 2,
        "handler should be called at least twice (fail then succeed)"
    );
    assert!(
        result.completed_nodes.contains(&"done".to_string()),
        "pipeline should reach done after retry"
    );
}

// ---------------------------------------------------------------------------
// Test 11: Validation catches multiple structural errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validation_catches_multiple_errors() {
    // Graph with no start node AND no terminal node
    let graph = build_graph(
        r#"digraph Bad {
            a [shape="box", prompt="A"]
            b [shape="box", prompt="B"]
            a -> b
        }"#,
    );

    let diags = validate(&graph);
    let error_rules: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == attractor_pipeline::Severity::Error)
        .map(|d| d.rule.as_str())
        .collect();

    assert!(
        error_rules.contains(&"start_node"),
        "should flag missing start node; got rules: {error_rules:?}"
    );
    assert!(
        error_rules.contains(&"terminal_node"),
        "should flag missing terminal node; got rules: {error_rules:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 12: Validation detects unreachable nodes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validation_detects_unreachable_nodes() {
    let graph = build_graph(
        r#"digraph Unreachable {
            start [shape="Mdiamond"]
            reachable [shape="box", prompt="Reachable"]
            orphan [shape="box", prompt="Orphan"]
            done [shape="Msquare"]
            start -> reachable -> done
        }"#,
    );

    let diags = validate(&graph);
    let unreachable_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.rule == "reachability" && d.severity == attractor_pipeline::Severity::Error)
        .collect();

    assert!(
        !unreachable_diags.is_empty(),
        "should detect orphan node as unreachable"
    );
    assert!(
        unreachable_diags
            .iter()
            .any(|d| d.message.contains("orphan")),
        "unreachable diagnostic should mention orphan; got: {unreachable_diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 13: Edge weight tiebreaker selects highest-weight unconditional edge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edge_weight_tiebreaker_selects_highest_weight() {
    // Two unconditional edges from check: one with weight=1, one with weight=10.
    // The higher-weight edge should win.
    let graph = build_graph(
        r#"digraph WeightTest {
            start [shape="Mdiamond"]
            check [shape="box", prompt="Check"]
            low [shape="box", prompt="Low weight"]
            high [shape="box", prompt="High weight"]
            done [shape="Msquare"]
            start -> check
            check -> low [weight=1]
            check -> high [weight=10]
            low -> done
            high -> done
        }"#,
    );

    let result = executor().run(&graph).await.expect("pipeline should succeed");

    assert!(
        result.completed_nodes.contains(&"high".to_string()),
        "higher weight should be selected; completed: {:?}",
        result.completed_nodes
    );
    assert!(
        !result.completed_nodes.contains(&"low".to_string()),
        "lower weight should not be taken"
    );
}

// ---------------------------------------------------------------------------
// Test 14: Full round-trip with graph-level goal attribute
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graph_goal_attribute_propagates_to_context() {
    let graph = build_graph(
        r#"digraph GoalTest {
            goal = "Build a working pipeline"
            start [shape="Mdiamond"]
            work [shape="box", prompt="Do the work"]
            done [shape="Msquare"]
            start -> work -> done
        }"#,
    );

    assert_eq!(graph.goal, "Build a working pipeline");

    let result = executor().run(&graph).await.expect("pipeline should succeed");

    // Graph attrs are loaded into context during initialization
    assert_eq!(
        result.final_context.get("goal"),
        Some(&serde_json::json!("Build a working pipeline")),
        "goal should be in final context"
    );
}

// ---------------------------------------------------------------------------
// Test 15: Condition-based routing with fail condition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn condition_routes_to_fallback_on_no_match() {
    // When outcome=success but condition requires outcome=fail,
    // the unconditional fallback edge should be taken.
    let graph = build_graph(
        r#"digraph CondFallback {
            start [shape="Mdiamond"]
            check [shape="diamond"]
            fail_path [shape="box", prompt="Fail path"]
            default_path [shape="box", prompt="Default path"]
            done [shape="Msquare"]
            start -> check
            check -> fail_path [condition="outcome=fail"]
            check -> default_path
            fail_path -> done
            default_path -> done
        }"#,
    );

    let result = executor().run(&graph).await.expect("pipeline should succeed");

    // Conditional handler returns Success, so outcome=success, which does NOT match
    // the condition "outcome=fail". The unconditional edge to default_path should be taken.
    assert!(
        result
            .completed_nodes
            .contains(&"default_path".to_string()),
        "default_path should be taken when condition does not match; completed: {:?}",
        result.completed_nodes
    );
    assert!(
        !result.completed_nodes.contains(&"fail_path".to_string()),
        "fail_path should not be taken"
    );
}
