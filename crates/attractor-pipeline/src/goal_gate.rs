use std::collections::HashMap;

use attractor_dot::AttributeValue;
use attractor_types::{AttractorError, Outcome, StageStatus};

use crate::graph::{PipelineGraph, PipelineNode};

/// Result of checking all goal gate nodes.
#[derive(Debug)]
pub struct GoalGateResult {
    pub all_satisfied: bool,
    pub failed_node_id: Option<String>,
    pub retry_target: Option<String>,
}

/// Check whether all visited goal gate nodes have succeeded.
/// Only checks nodes that appear in `node_outcomes` (visited nodes).
pub fn check_goal_gates(
    graph: &PipelineGraph,
    node_outcomes: &HashMap<String, Outcome>,
) -> GoalGateResult {
    for (node_id, outcome) in node_outcomes {
        if let Some(node) = graph.node(node_id) {
            if node.goal_gate
                && !matches!(
                    outcome.status,
                    StageStatus::Success | StageStatus::PartialSuccess
                )
            {
                let retry = resolve_retry_target(node, graph);
                return GoalGateResult {
                    all_satisfied: false,
                    failed_node_id: Some(node_id.clone()),
                    retry_target: retry,
                };
            }
        }
    }
    GoalGateResult {
        all_satisfied: true,
        failed_node_id: None,
        retry_target: None,
    }
}

/// Resolve the retry target using 4-level fallback:
/// 1. Node `retry_target`
/// 2. Node `fallback_retry_target`
/// 3. Graph `retry_target` attribute
/// 4. Graph `fallback_retry_target` attribute
fn resolve_retry_target(node: &PipelineNode, graph: &PipelineGraph) -> Option<String> {
    node.retry_target
        .clone()
        .or_else(|| node.fallback_retry_target.clone())
        .or_else(|| {
            graph.attrs.get("retry_target").and_then(|v| match v {
                AttributeValue::String(s) => Some(s.clone()),
                _ => None,
            })
        })
        .or_else(|| {
            graph
                .attrs
                .get("fallback_retry_target")
                .and_then(|v| match v {
                    AttributeValue::String(s) => Some(s.clone()),
                    _ => None,
                })
        })
}

/// Enforce goal gates: if unsatisfied and no retry target, return error.
pub fn enforce_goal_gates(
    graph: &PipelineGraph,
    node_outcomes: &HashMap<String, Outcome>,
) -> Result<GoalGateResult, AttractorError> {
    let result = check_goal_gates(graph, node_outcomes);
    if !result.all_satisfied && result.retry_target.is_none() {
        return Err(AttractorError::GoalGateUnsatisfied {
            node: result.failed_node_id.unwrap_or_default(),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::PipelineGraph;

    fn parse_and_build(dot: &str) -> PipelineGraph {
        let graph = attractor_dot::parse(dot).unwrap();
        PipelineGraph::from_dot(graph).unwrap()
    }

    fn make_outcome(status: StageStatus) -> Outcome {
        Outcome {
            status,
            preferred_label: None,
            suggested_next_ids: vec![],
            context_updates: HashMap::new(),
            notes: String::new(),
            failure_reason: None,
        }
    }

    #[test]
    fn all_goal_gates_satisfied() {
        let pg = parse_and_build(
            r#"digraph G {
            review [goal_gate=true]
            review -> done
        }"#,
        );

        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::Success));

        let result = check_goal_gates(&pg, &outcomes);
        assert!(result.all_satisfied);
        assert!(result.failed_node_id.is_none());
        assert!(result.retry_target.is_none());
    }

    #[test]
    fn failed_goal_gate_with_retry_target() {
        let pg = parse_and_build(
            r#"digraph G {
            review [goal_gate=true, retry_target="draft"]
            draft -> review -> done
        }"#,
        );

        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::Fail));

        let result = check_goal_gates(&pg, &outcomes);
        assert!(!result.all_satisfied);
        assert_eq!(result.failed_node_id.as_deref(), Some("review"));
        assert_eq!(result.retry_target.as_deref(), Some("draft"));
    }

    #[test]
    fn failed_goal_gate_without_retry_returns_error() {
        let pg = parse_and_build(
            r#"digraph G {
            review [goal_gate=true]
            review -> done
        }"#,
        );

        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::Fail));

        let err = enforce_goal_gates(&pg, &outcomes).unwrap_err();
        match err {
            AttractorError::GoalGateUnsatisfied { node } => {
                assert_eq!(node, "review");
            }
            other => panic!("expected GoalGateUnsatisfied, got: {other:?}"),
        }
    }

    #[test]
    fn non_goal_gate_nodes_ignored_even_if_failed() {
        let pg = parse_and_build(
            r#"digraph G {
            step_a [goal_gate=false]
            step_b [goal_gate=true]
            step_a -> step_b -> done
        }"#,
        );

        let mut outcomes = HashMap::new();
        outcomes.insert("step_a".into(), make_outcome(StageStatus::Fail));
        outcomes.insert("step_b".into(), make_outcome(StageStatus::Success));

        let result = check_goal_gates(&pg, &outcomes);
        assert!(result.all_satisfied);
    }

    #[test]
    fn only_visited_nodes_checked() {
        let pg = parse_and_build(
            r#"digraph G {
            gate_a [goal_gate=true]
            gate_b [goal_gate=true]
            gate_a -> gate_b -> done
        }"#,
        );

        // Only gate_a was visited (and succeeded); gate_b is unvisited.
        let mut outcomes = HashMap::new();
        outcomes.insert("gate_a".into(), make_outcome(StageStatus::Success));

        let result = check_goal_gates(&pg, &outcomes);
        assert!(result.all_satisfied);
    }

    #[test]
    fn four_level_retry_fallback_chain() {
        // Level 1: node retry_target
        let pg = parse_and_build(
            r#"digraph G {
            review [goal_gate=true, retry_target="node_rt"]
            review -> done
        }"#,
        );
        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::Fail));
        assert_eq!(
            check_goal_gates(&pg, &outcomes).retry_target.as_deref(),
            Some("node_rt")
        );

        // Level 2: node fallback_retry_target
        let pg = parse_and_build(
            r#"digraph G {
            review [goal_gate=true, fallback_retry_target="node_frt"]
            review -> done
        }"#,
        );
        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::Fail));
        assert_eq!(
            check_goal_gates(&pg, &outcomes).retry_target.as_deref(),
            Some("node_frt")
        );

        // Level 3: graph retry_target
        let pg = parse_and_build(
            r#"digraph G {
            retry_target = "graph_rt"
            review [goal_gate=true]
            review -> done
        }"#,
        );
        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::Fail));
        assert_eq!(
            check_goal_gates(&pg, &outcomes).retry_target.as_deref(),
            Some("graph_rt")
        );

        // Level 4: graph fallback_retry_target
        let pg = parse_and_build(
            r#"digraph G {
            fallback_retry_target = "graph_frt"
            review [goal_gate=true]
            review -> done
        }"#,
        );
        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::Fail));
        assert_eq!(
            check_goal_gates(&pg, &outcomes).retry_target.as_deref(),
            Some("graph_frt")
        );
    }

    #[test]
    fn partial_success_counts_as_satisfied() {
        let pg = parse_and_build(
            r#"digraph G {
            review [goal_gate=true]
            review -> done
        }"#,
        );

        let mut outcomes = HashMap::new();
        outcomes.insert("review".into(), make_outcome(StageStatus::PartialSuccess));

        let result = check_goal_gates(&pg, &outcomes);
        assert!(result.all_satisfied);
    }
}
