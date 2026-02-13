//! Pipeline execution engine, node handlers, validation, and edge selection.
//!
//! This crate implements the core Attractor pipeline runner: DOT graph traversal,
//! handler dispatch, edge selection, goal gate enforcement, checkpoint/resume,
//! and the 11 built-in lint rules.

pub mod checkpoint;
pub mod condition;
pub mod edge_selection;
pub mod engine;
pub mod events;
pub mod goal_gate;
pub mod graph;
pub mod handler;
pub mod handlers;
pub mod interviewer;
pub mod retry;
pub mod stylesheet;
pub mod transforms;
pub mod validation;

pub use condition::{
    parse_condition, evaluate_condition, Clause, ConditionExpr, Operator,
};
pub use edge_selection::select_edge;
pub use goal_gate::{check_goal_gates, enforce_goal_gates, GoalGateResult};
pub use graph::{PipelineEdge, PipelineGraph, PipelineNode};
pub use handler::{
    default_registry, default_registry_with_interviewer, ConditionalHandler, DynHandler, ExitHandler,
    HandlerRegistry, NodeHandler, StartHandler,
};
pub use handlers::{CodergenHandler, FanInHandler, ManagerLoopHandler, ParallelHandler, ToolHandler};
pub use handlers::wait_human::WaitHumanHandler;
pub use interviewer::{
    Answer, AutoApproveInterviewer, ConsoleInterviewer, Interviewer, Question,
    RecordingInterviewer,
};
pub use engine::{PipelineConfig, PipelineExecutor, PipelineResult};
pub use stylesheet::{apply_stylesheet, parse_stylesheet, Declaration, Rule, Selector, Stylesheet};
pub use transforms::{apply_transforms, expand_variables};
pub use validation::{validate, validate_or_raise, Diagnostic, LintRule, Severity};
pub use checkpoint::{clear_checkpoint, load_checkpoint, save_checkpoint, PipelineCheckpoint};
pub use events::{EventEmitter, PipelineEvent};
pub use retry::{execute_with_retry, BackoffPolicy};
