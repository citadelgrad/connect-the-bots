//! Node handler trait, dynamic dispatch wrapper, and handler registry.

use std::collections::HashMap;

use async_trait::async_trait;

use attractor_types::{Context, Outcome, Result};

use crate::graph::{PipelineGraph, PipelineNode};

// ---------------------------------------------------------------------------
// NodeHandler trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait NodeHandler: Send + Sync {
    /// The handler type identifier (e.g. "start", "exit", "codergen").
    fn handler_type(&self) -> &str;

    /// Execute this handler for a given node.
    async fn execute(
        &self,
        node: &PipelineNode,
        context: &Context,
        graph: &PipelineGraph,
    ) -> Result<Outcome>;
}

// ---------------------------------------------------------------------------
// DynHandler — object-safe wrapper
// ---------------------------------------------------------------------------

pub struct DynHandler(Box<dyn NodeHandler>);

impl DynHandler {
    pub fn new(handler: impl NodeHandler + 'static) -> Self {
        Self(Box::new(handler))
    }

    pub fn handler_type(&self) -> &str {
        self.0.handler_type()
    }

    pub async fn execute(
        &self,
        node: &PipelineNode,
        context: &Context,
        graph: &PipelineGraph,
    ) -> Result<Outcome> {
        self.0.execute(node, context, graph).await
    }
}

// ---------------------------------------------------------------------------
// HandlerRegistry
// ---------------------------------------------------------------------------

pub struct HandlerRegistry {
    handlers: HashMap<String, DynHandler>,
    shape_to_type: HashMap<String, String>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        let mut shape_to_type = HashMap::new();
        shape_to_type.insert("Mdiamond".into(), "start".into());
        shape_to_type.insert("Msquare".into(), "exit".into());
        shape_to_type.insert("box".into(), "codergen".into());
        shape_to_type.insert("hexagon".into(), "wait.human".into());
        shape_to_type.insert("diamond".into(), "conditional".into());
        shape_to_type.insert("component".into(), "parallel".into());
        shape_to_type.insert("tripleoctagon".into(), "parallel.fan_in".into());
        shape_to_type.insert("parallelogram".into(), "tool".into());
        shape_to_type.insert("house".into(), "stack.manager_loop".into());

        Self {
            handlers: HashMap::new(),
            shape_to_type,
        }
    }

    pub fn register(&mut self, handler: impl NodeHandler + 'static) {
        let t = handler.handler_type().to_string();
        self.handlers.insert(t, DynHandler::new(handler));
    }

    /// Resolve a node to its handler type using 3-step priority:
    /// 1. Explicit `type` attribute on the node
    /// 2. Shape-based mapping
    /// 3. Default: `"codergen"`
    ///
    /// Special case: conditional nodes with a prompt are routed to `"codergen"`
    /// so the prompt actually gets executed via Claude. The `ConditionalHandler`
    /// is a pass-through for pure routing nodes with no prompt.
    pub fn resolve_type(&self, node: &PipelineNode) -> String {
        if let Some(ref t) = node.node_type {
            if t == "conditional" && node.prompt.is_some() {
                return "codergen".to_string();
            }
            return t.clone();
        }
        if let Some(t) = self.shape_to_type.get(&node.shape) {
            if t == "conditional" && node.prompt.is_some() {
                return "codergen".to_string();
            }
            return t.clone();
        }
        "codergen".to_string()
    }

    pub fn get(&self, handler_type: &str) -> Option<&DynHandler> {
        self.handlers.get(handler_type)
    }

    pub fn has(&self, handler_type: &str) -> bool {
        self.handlers.contains_key(handler_type)
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in handlers
// ---------------------------------------------------------------------------

pub struct StartHandler;

#[async_trait]
impl NodeHandler for StartHandler {
    fn handler_type(&self) -> &str {
        "start"
    }

    async fn execute(
        &self,
        _node: &PipelineNode,
        _ctx: &Context,
        _graph: &PipelineGraph,
    ) -> Result<Outcome> {
        Ok(Outcome::success("Pipeline started"))
    }
}

pub struct ExitHandler;

#[async_trait]
impl NodeHandler for ExitHandler {
    fn handler_type(&self) -> &str {
        "exit"
    }

    async fn execute(
        &self,
        _node: &PipelineNode,
        _ctx: &Context,
        _graph: &PipelineGraph,
    ) -> Result<Outcome> {
        Ok(Outcome::success("Pipeline completed"))
    }
}

pub struct ConditionalHandler;

#[async_trait]
impl NodeHandler for ConditionalHandler {
    fn handler_type(&self) -> &str {
        "conditional"
    }

    async fn execute(
        &self,
        _node: &PipelineNode,
        _ctx: &Context,
        _graph: &PipelineGraph,
    ) -> Result<Outcome> {
        Ok(Outcome::success("Conditional pass-through"))
    }
}

// ---------------------------------------------------------------------------
// Default registry factory
// ---------------------------------------------------------------------------

pub fn default_registry() -> HandlerRegistry {
    let mut reg = HandlerRegistry::new();
    reg.register(StartHandler);
    reg.register(ExitHandler);
    reg.register(ConditionalHandler);
    reg.register(crate::handlers::ToolHandler);
    reg.register(crate::handlers::CodergenHandler);
    reg.register(crate::handlers::ParallelHandler);
    reg.register(crate::handlers::FanInHandler);
    reg.register(crate::handlers::ManagerLoopHandler);
    reg
}

/// Create the default handler registry with WaitHumanHandler registered.
///
/// This factory function creates a registry with all the standard handlers
/// plus WaitHumanHandler configured with the provided interviewer.
/// Use this when you need to support hexagon (human review) nodes in pipelines.
pub fn default_registry_with_interviewer(
    interviewer: std::sync::Arc<dyn crate::interviewer::Interviewer>,
) -> HandlerRegistry {
    let mut reg = default_registry();
    reg.register(crate::handlers::wait_human::WaitHumanHandler::new(interviewer));
    reg
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_node(id: &str, shape: &str, node_type: Option<&str>) -> PipelineNode {
        PipelineNode {
            id: id.to_string(),
            label: id.to_string(),
            shape: shape.to_string(),
            node_type: node_type.map(String::from),
            prompt: None,
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
            raw_attrs: HashMap::new(),
        }
    }

    #[test]
    fn resolve_type_explicit_attribute() {
        let reg = HandlerRegistry::new();
        let node = make_node("n", "box", Some("custom_handler"));
        assert_eq!(reg.resolve_type(&node), "custom_handler");
    }

    #[test]
    fn resolve_type_shape_mapping() {
        let reg = HandlerRegistry::new();
        let node = make_node("begin", "Mdiamond", None);
        assert_eq!(reg.resolve_type(&node), "start");
    }

    #[test]
    fn resolve_type_defaults_to_codergen() {
        let reg = HandlerRegistry::new();
        let node = make_node("x", "unknown_shape", None);
        assert_eq!(reg.resolve_type(&node), "codergen");
    }

    #[test]
    fn resolve_type_conditional_without_prompt_stays_conditional() {
        let reg = HandlerRegistry::new();
        // Diamond with no prompt → conditional (pass-through)
        let node = make_node("check", "diamond", None);
        assert_eq!(reg.resolve_type(&node), "conditional");
    }

    #[test]
    fn resolve_type_conditional_with_prompt_becomes_codergen() {
        let reg = HandlerRegistry::new();
        // Diamond with a prompt → codergen (needs LLM to run the prompt)
        let mut node = make_node("check", "diamond", None);
        node.prompt = Some("Check if tasks remain".to_string());
        assert_eq!(reg.resolve_type(&node), "codergen");
    }

    #[test]
    fn resolve_type_explicit_conditional_with_prompt_becomes_codergen() {
        let reg = HandlerRegistry::new();
        // Explicit node_type="conditional" with a prompt → codergen
        let mut node = make_node("check", "diamond", Some("conditional"));
        node.prompt = Some("Check if tasks remain".to_string());
        assert_eq!(reg.resolve_type(&node), "codergen");
    }

    #[test]
    fn register_and_get_handler() {
        let mut reg = HandlerRegistry::new();
        reg.register(StartHandler);
        assert!(reg.has("start"));
        assert!(reg.get("start").is_some());
        assert!(!reg.has("nonexistent"));
        assert!(reg.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn start_handler_returns_success() {
        let handler = StartHandler;
        let node = make_node("s", "Mdiamond", None);
        let ctx = Context::default();
        let graph = make_minimal_graph();
        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert_eq!(outcome.status, attractor_types::StageStatus::Success);
        assert_eq!(outcome.notes, "Pipeline started");
    }

    #[tokio::test]
    async fn exit_handler_returns_success() {
        let handler = ExitHandler;
        let node = make_node("e", "Msquare", None);
        let ctx = Context::default();
        let graph = make_minimal_graph();
        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert_eq!(outcome.status, attractor_types::StageStatus::Success);
        assert_eq!(outcome.notes, "Pipeline completed");
    }

    #[test]
    fn default_registry_has_builtins() {
        let reg = default_registry();
        assert!(reg.has("start"));
        assert!(reg.has("exit"));
        assert!(reg.has("conditional"));
        assert!(reg.has("tool"));
        assert!(reg.has("codergen"));
        assert!(reg.has("parallel"));
        assert!(reg.has("parallel.fan_in"));
        assert!(reg.has("stack.manager_loop"));
    }

    fn make_minimal_graph() -> PipelineGraph {
        let dot = r#"digraph G { A -> B }"#;
        let parsed = attractor_dot::parse(dot).unwrap();
        PipelineGraph::from_dot(parsed).unwrap()
    }
}
