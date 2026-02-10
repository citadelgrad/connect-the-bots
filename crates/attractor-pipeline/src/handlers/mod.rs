//! Additional node handlers beyond the basic start/exit/conditional.

pub mod manager;
pub mod parallel;
pub mod wait_human;

pub use manager::ManagerLoopHandler;
pub use parallel::{FanInHandler, ParallelHandler};

use std::collections::HashMap;

use async_trait::async_trait;
use attractor_dot::AttributeValue;
use attractor_types::{AttractorError, Context, Outcome, Result, StageStatus};

use crate::graph::{PipelineGraph, PipelineNode};
use crate::handler::NodeHandler;

// ---------------------------------------------------------------------------
// ToolHandler — executes a shell command (parallelogram shape)
// ---------------------------------------------------------------------------

pub struct ToolHandler;

#[async_trait]
impl NodeHandler for ToolHandler {
    fn handler_type(&self) -> &str {
        "tool"
    }

    async fn execute(
        &self,
        node: &PipelineNode,
        _context: &Context,
        _graph: &PipelineGraph,
    ) -> Result<Outcome> {
        let command = node
            .raw_attrs
            .get("tool_command")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| AttractorError::HandlerError {
                handler: "tool".into(),
                node: node.id.clone(),
                message: "Missing tool_command attribute".into(),
            })?;

        tracing::info!(node = %node.id, label = %node.label, command = %command, "Executing tool handler");

        Ok(Outcome {
            status: StageStatus::Success,
            preferred_label: None,
            suggested_next_ids: vec![],
            context_updates: {
                let mut m = HashMap::new();
                m.insert(
                    "last_tool_command".into(),
                    serde_json::Value::String(command.clone()),
                );
                m.insert(
                    format!("{}.completed", node.id),
                    serde_json::Value::Bool(true),
                );
                m
            },
            notes: format!("Tool command recorded: {}", command),
            failure_reason: None,
        })
    }
}

// ---------------------------------------------------------------------------
// CodergenHandler — LLM task handler (box shape)
//
// Uses an AgentSession to run an LLM agentic loop for the node's task.
// Currently operates as a structured placeholder that extracts the prompt
// and returns it as the outcome. A real backend (LlmClient, ToolRegistry,
// ExecutionEnvironment) can be injected later via the pipeline context or
// a shared backend reference.
// ---------------------------------------------------------------------------

pub struct CodergenHandler;

#[async_trait]
impl NodeHandler for CodergenHandler {
    fn handler_type(&self) -> &str {
        "codergen"
    }

    async fn execute(
        &self,
        node: &PipelineNode,
        _context: &Context,
        graph: &PipelineGraph,
    ) -> Result<Outcome> {
        let prompt = node.prompt.as_deref().unwrap_or("No prompt specified");
        let label = node.label.clone();

        tracing::info!(node = %node.id, label = %label, "Executing codergen handler");

        // Extract the graph-level goal for context (an AgentSession would use this)
        let _goal = graph.goal.clone();

        // TODO: When a real backend is available, create an AgentSession here:
        //   let session = AgentSession::new(llm_client, tool_registry, env);
        //   let result = session.run(prompt, &goal).await?;

        Ok(Outcome {
            status: StageStatus::Success,
            preferred_label: None,
            suggested_next_ids: vec![],
            context_updates: {
                let mut updates = HashMap::new();
                updates.insert(
                    format!("{}.completed", node.id),
                    serde_json::Value::Bool(true),
                );
                updates.insert(
                    format!("{}.prompt", node.id),
                    serde_json::Value::String(prompt.to_string()),
                );
                updates
            },
            notes: format!("Codergen completed for '{}': {}", label, prompt),
            failure_reason: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use attractor_dot::AttributeValue;

    fn make_node(
        id: &str,
        shape: &str,
        prompt: Option<&str>,
        attrs: HashMap<String, AttributeValue>,
    ) -> PipelineNode {
        PipelineNode {
            id: id.to_string(),
            label: id.to_string(),
            shape: shape.to_string(),
            node_type: None,
            prompt: prompt.map(String::from),
            max_retries: 0,
            goal_gate: false,
            retry_target: None,
            fallback_retry_target: None,
            fidelity: None,
            thread_id: None,
            classes: Vec::new(),
            timeout: None,
            llm_model: None,
            llm_provider: None,
            reasoning_effort: None,
            auto_status: true,
            allow_partial: false,
            raw_attrs: attrs,
        }
    }

    fn make_minimal_graph() -> PipelineGraph {
        let dot = r#"digraph G { A -> B }"#;
        let parsed = attractor_dot::parse(dot).unwrap();
        PipelineGraph::from_dot(parsed).unwrap()
    }

    #[tokio::test]
    async fn tool_handler_extracts_command() {
        let handler = ToolHandler;
        let mut attrs = HashMap::new();
        attrs.insert(
            "tool_command".into(),
            AttributeValue::String("cargo test".into()),
        );
        let node = make_node("t", "parallelogram", None, attrs);
        let ctx = Context::default();
        let graph = make_minimal_graph();

        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert_eq!(
            outcome.context_updates.get("last_tool_command"),
            Some(&serde_json::Value::String("cargo test".into()))
        );
        assert_eq!(
            outcome.context_updates.get("t.completed"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(outcome.notes.contains("cargo test"));
    }

    #[tokio::test]
    async fn tool_handler_errors_on_missing_command() {
        let handler = ToolHandler;
        let node = make_node("t", "parallelogram", None, HashMap::new());
        let ctx = Context::default();
        let graph = make_minimal_graph();

        let result = handler.execute(&node, &ctx, &graph).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Missing tool_command"),
            "Expected error about missing tool_command, got: {err}"
        );
    }

    #[tokio::test]
    async fn tool_handler_returns_success() {
        let handler = ToolHandler;
        let mut attrs = HashMap::new();
        attrs.insert(
            "tool_command".into(),
            AttributeValue::String("echo hello".into()),
        );
        let node = make_node("run_echo", "parallelogram", None, attrs);
        let ctx = Context::default();
        let graph = make_minimal_graph();

        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert!(outcome.failure_reason.is_none());
    }

    #[tokio::test]
    async fn codergen_handler_with_prompt_returns_success() {
        let handler = CodergenHandler;
        let node = make_node("c", "box", Some("Write unit tests"), HashMap::new());
        let ctx = Context::default();
        let graph = make_minimal_graph();

        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert!(outcome.notes.contains("Write unit tests"));
        assert_eq!(
            outcome.context_updates.get("c.completed"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            outcome.context_updates.get("c.prompt"),
            Some(&serde_json::Value::String("Write unit tests".into()))
        );
    }

    #[tokio::test]
    async fn codergen_handler_without_prompt_returns_default_message() {
        let handler = CodergenHandler;
        let node = make_node("c", "box", None, HashMap::new());
        let ctx = Context::default();
        let graph = make_minimal_graph();

        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert!(outcome.notes.contains("No prompt specified"));
        assert_eq!(
            outcome.context_updates.get("c.prompt"),
            Some(&serde_json::Value::String("No prompt specified".into()))
        );
    }

    #[tokio::test]
    async fn codergen_handler_notes_include_label() {
        let handler = CodergenHandler;
        let mut node = make_node("my_task", "box", Some("Do the thing"), HashMap::new());
        node.label = "Build the feature".into();
        let ctx = Context::default();
        let graph = make_minimal_graph();

        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert!(
            outcome.notes.contains("Build the feature"),
            "Expected notes to contain label, got: {}",
            outcome.notes
        );
        assert!(
            outcome.notes.contains("Do the thing"),
            "Expected notes to contain prompt, got: {}",
            outcome.notes
        );
    }
}
