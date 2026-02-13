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
        context: &Context,
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

        tracing::info!(node = %node.id, label = %node.label, command = %command, "Executing tool command");

        // Check if dry_run is set in context
        let dry_run = context
            .get("dry_run")
            .await
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if dry_run {
            tracing::info!(node = %node.id, "Dry run — skipping command execution");
            return Ok(Outcome {
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
                    m.insert(
                        format!("{}.dry_run", node.id),
                        serde_json::Value::Bool(true),
                    );
                    m
                },
                notes: format!("Dry run — command not executed: {}", command),
                failure_reason: None,
            });
        }

        // Build the shell command
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&command);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Set working directory from context
        let snapshot = context.snapshot().await;
        if let Some(serde_json::Value::String(dir)) = snapshot.get("workdir") {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn().map_err(|e| AttractorError::HandlerError {
            handler: "tool".into(),
            node: node.id.clone(),
            message: format!("Failed to spawn command: {}", e),
        })?;

        // Apply timeout if configured on the node, default 5 minutes
        let timeout_dur = node.timeout.unwrap_or(std::time::Duration::from_secs(300));
        let output = tokio::time::timeout(timeout_dur, child.wait_with_output())
            .await
            .map_err(|_| AttractorError::CommandTimeout {
                timeout_ms: timeout_dur.as_millis() as u64,
            })?
            .map_err(|e| AttractorError::HandlerError {
                handler: "tool".into(),
                node: node.id.clone(),
                message: format!("Command execution failed: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        tracing::info!(
            node = %node.id,
            exit_code = exit_code,
            stdout_len = stdout.len(),
            stderr_len = stderr.len(),
            "Tool command completed"
        );

        let status = if output.status.success() {
            StageStatus::Success
        } else {
            StageStatus::Fail
        };

        let mut updates = HashMap::new();
        updates.insert(
            "last_tool_command".into(),
            serde_json::Value::String(command.clone()),
        );
        updates.insert(
            format!("{}.completed", node.id),
            serde_json::Value::Bool(true),
        );
        updates.insert(
            format!("{}.exit_code", node.id),
            serde_json::json!(exit_code),
        );
        updates.insert(
            format!("{}.stdout", node.id),
            serde_json::Value::String(stdout.clone()),
        );
        if !stderr.is_empty() {
            updates.insert(
                format!("{}.stderr", node.id),
                serde_json::Value::String(stderr.clone()),
            );
        }

        // Combine stdout + stderr for notes, truncating if very long
        let combined = if stderr.is_empty() {
            stdout
        } else {
            format!("{}\n--- stderr ---\n{}", stdout, stderr)
        };
        let notes = if combined.len() > 4096 {
            // Find a valid UTF-8 boundary at or before byte 4096
            let truncate_at = combined
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= 4096)
                .last()
                .unwrap_or(0);
            format!("{}...(truncated)", &combined[..truncate_at])
        } else {
            combined
        };

        Ok(Outcome {
            status,
            preferred_label: None,
            suggested_next_ids: vec![],
            context_updates: updates,
            notes,
            failure_reason: if status == StageStatus::Fail {
                Some(format!("Command exited with code {}", exit_code))
            } else {
                None
            },
        })
    }
}

// ---------------------------------------------------------------------------
// CodergenHandler — LLM task handler (box shape)
//
// Shells out to `claude` CLI (Claude Code) for each node, passing the node's
// prompt. This uses the user's Claude subscription — no API keys needed.
//
// The handler builds a system prompt that includes the pipeline goal and
// prior context, then invokes `claude -p` with JSON output. The response
// is parsed to determine success/failure and to extract any preferred_label
// for conditional edge routing.
//
// Supported node attributes:
//   - prompt (required): The task prompt sent to Claude Code
//   - llm_model: Override the model (e.g. "sonnet", "haiku", "opus")
//   - allowed_tools: Comma-separated tool list (default: all)
//   - max_budget_usd: Spending cap for this node
//
// The pipeline context key "workdir" controls the working directory.
// ---------------------------------------------------------------------------

pub struct CodergenHandler;

/// Result shape from `claude -p --output-format json`
#[derive(serde::Deserialize)]
struct ClaudeOutput {
    #[serde(default)]
    result: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    subtype: String,
    #[serde(default)]
    total_cost_usd: f64,
    #[serde(default)]
    num_turns: u32,
}

#[async_trait]
impl NodeHandler for CodergenHandler {
    fn handler_type(&self) -> &str {
        "codergen"
    }

    async fn execute(
        &self,
        node: &PipelineNode,
        context: &Context,
        graph: &PipelineGraph,
    ) -> Result<Outcome> {
        let prompt = node.prompt.as_deref().unwrap_or("No prompt specified");
        let label = node.label.clone();

        tracing::info!(node = %node.id, label = %label, "Executing codergen handler via Claude Code");

        // Check if dry_run is set in context
        let dry_run = context
            .get("dry_run")
            .await
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if dry_run {
            tracing::info!(node = %node.id, "Dry run — skipping Claude CLI execution");
            return Ok(Outcome {
                status: StageStatus::Success,
                preferred_label: None,
                suggested_next_ids: vec![],
                context_updates: {
                    let mut m = HashMap::new();
                    m.insert(
                        format!("{}.result", node.id),
                        serde_json::Value::String(format!("Dry run — prompt not sent: {}", prompt)),
                    );
                    m.insert(
                        format!("{}.completed", node.id),
                        serde_json::Value::Bool(true),
                    );
                    m.insert(
                        format!("{}.dry_run", node.id),
                        serde_json::Value::Bool(true),
                    );
                    m
                },
                notes: format!("Dry run — Claude CLI not invoked for: {}", label),
                failure_reason: None,
            });
        }

        // Build the full prompt with pipeline context
        let goal = &graph.goal;
        let mut full_prompt = String::new();

        if !goal.is_empty() {
            full_prompt.push_str(&format!("Pipeline goal: {}\n\n", goal));
        }

        // Inject relevant context from prior nodes
        let snapshot = context.snapshot().await;
        let context_keys: Vec<_> = snapshot
            .iter()
            .filter(|(k, _)| k.ends_with(".result") || k.ends_with(".output"))
            .collect();
        if !context_keys.is_empty() {
            full_prompt.push_str("Context from prior pipeline steps:\n");
            for (k, v) in &context_keys {
                if let serde_json::Value::String(s) = v {
                    full_prompt.push_str(&format!("- {}: {}\n", k, s));
                } else {
                    full_prompt.push_str(&format!("- {}: {}\n", k, v));
                }
            }
            full_prompt.push('\n');
        }

        full_prompt.push_str(&format!("Task ({}): {}", label, prompt));

        // If this is a conditional node, instruct Claude to output a label
        if node.shape == "diamond" || node.node_type.as_deref() == Some("conditional") {
            // Collect the possible labels from outgoing edges
            let edges = graph.outgoing_edges(&node.id);
            let labels: Vec<_> = edges
                .iter()
                .filter_map(|e| e.label.as_deref())
                .collect();
            if !labels.is_empty() {
                full_prompt.push_str(&format!(
                    "\n\nYou MUST end your response with exactly one of these labels on its own line: {}",
                    labels.join(", ")
                ));
            }
        }

        // Build claude CLI command
        let mut cmd = tokio::process::Command::new("claude");
        cmd.arg("-p")
            .arg(&full_prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--no-session-persistence")
            .arg("--dangerously-skip-permissions");

        // Model override from node attribute or graph-level model
        if let Some(ref model) = node.llm_model {
            cmd.arg("--model").arg(model);
        } else if let Some(AttributeValue::String(model)) = graph.attrs.get("model") {
            cmd.arg("--model").arg(model);
        }

        // Allowed tools from node attribute
        if let Some(AttributeValue::String(tools)) = node.raw_attrs.get("allowed_tools") {
            cmd.arg("--allowedTools").arg(tools);
        }

        // Budget cap from node attribute
        if let Some(AttributeValue::String(budget)) = node.raw_attrs.get("max_budget_usd") {
            cmd.arg("--max-budget-usd").arg(budget);
        }

        // Working directory from context or current dir
        if let Some(serde_json::Value::String(dir)) = snapshot.get("workdir") {
            cmd.current_dir(dir);
        }

        // Capture output
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let child = cmd.spawn().map_err(|e| AttractorError::HandlerError {
            handler: "codergen".into(),
            node: node.id.clone(),
            message: format!("Failed to spawn claude CLI: {}", e),
        })?;

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| AttractorError::HandlerError {
                handler: "codergen".into(),
                node: node.id.clone(),
                message: format!("Claude CLI execution failed: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() && stdout.is_empty() {
            return Err(AttractorError::HandlerError {
                handler: "codergen".into(),
                node: node.id.clone(),
                message: format!(
                    "Claude CLI exited with {}: {}",
                    output.status,
                    stderr.trim()
                ),
            });
        }

        // Parse JSON output
        let claude_result: ClaudeOutput =
            serde_json::from_str(&stdout).map_err(|e| AttractorError::HandlerError {
                handler: "codergen".into(),
                node: node.id.clone(),
                message: format!("Failed to parse claude output: {} — raw: {}", e, &stdout[..stdout.len().min(500)]),
            })?;

        tracing::info!(
            node = %node.id,
            turns = claude_result.num_turns,
            cost = claude_result.total_cost_usd,
            error = claude_result.is_error,
            "Claude Code completed"
        );

        // Determine status
        let status = if claude_result.is_error || claude_result.subtype == "error" {
            StageStatus::Fail
        } else {
            StageStatus::Success
        };

        // Extract preferred_label from the response for conditional routing
        let preferred_label = if node.shape == "diamond"
            || node.node_type.as_deref() == Some("conditional")
        {
            let edges = graph.outgoing_edges(&node.id);
            let labels: Vec<String> = edges
                .iter()
                .filter_map(|e| e.label.clone())
                .collect();
            // Look for a label match in the last line(s) of the result
            extract_label(&claude_result.result, &labels)
        } else {
            None
        };

        // Build context updates
        let mut updates = HashMap::new();
        updates.insert(
            format!("{}.completed", node.id),
            serde_json::Value::Bool(true),
        );
        updates.insert(
            format!("{}.result", node.id),
            serde_json::Value::String(claude_result.result.clone()),
        );
        updates.insert(
            format!("{}.cost_usd", node.id),
            serde_json::json!(claude_result.total_cost_usd),
        );
        updates.insert(
            format!("{}.turns", node.id),
            serde_json::json!(claude_result.num_turns),
        );
        if let Some(ref lbl) = preferred_label {
            updates.insert(
                format!("{}.label", node.id),
                serde_json::Value::String(lbl.clone()),
            );
        }

        Ok(Outcome {
            status,
            preferred_label,
            suggested_next_ids: vec![],
            context_updates: updates,
            notes: claude_result.result,
            failure_reason: if status == StageStatus::Fail {
                Some("Claude Code returned an error".into())
            } else {
                None
            },
        })
    }
}

/// Scan the Claude response for one of the expected edge labels.
/// Checks the last few lines first (where we asked Claude to put it),
/// then falls back to scanning the full text.
fn extract_label(response: &str, labels: &[String]) -> Option<String> {
    let lines: Vec<&str> = response.lines().rev().take(5).collect();
    // Check last lines for an exact match
    for line in &lines {
        let trimmed = line.trim();
        for label in labels {
            if trimmed.eq_ignore_ascii_case(label) {
                return Some(label.clone());
            }
        }
    }
    // Fallback: search full response for label as a standalone word
    let upper = response.to_uppercase();
    for label in labels {
        if upper.contains(&label.to_uppercase()) {
            return Some(label.clone());
        }
    }
    None
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
    async fn tool_handler_dry_run_skips_execution() {
        let handler = ToolHandler;
        let mut attrs = HashMap::new();
        attrs.insert(
            "tool_command".into(),
            AttributeValue::String("cargo test".into()),
        );
        let node = make_node("t", "parallelogram", None, attrs);
        let ctx = Context::default();
        ctx.set("dry_run", serde_json::Value::Bool(true)).await;
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
        assert_eq!(
            outcome.context_updates.get("t.dry_run"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(outcome.notes.contains("Dry run"));
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
    async fn tool_handler_executes_command() {
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
        assert!(outcome.notes.contains("hello"));
        assert_eq!(
            outcome.context_updates.get("run_echo.exit_code"),
            Some(&serde_json::json!(0))
        );
        assert!(outcome
            .context_updates
            .get("run_echo.stdout")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("hello"));
    }

    #[tokio::test]
    async fn tool_handler_captures_failure() {
        let handler = ToolHandler;
        let mut attrs = HashMap::new();
        attrs.insert(
            "tool_command".into(),
            AttributeValue::String("exit 42".into()),
        );
        let node = make_node("fail_cmd", "parallelogram", None, attrs);
        let ctx = Context::default();
        let graph = make_minimal_graph();

        let outcome = handler.execute(&node, &ctx, &graph).await.unwrap();
        assert_eq!(outcome.status, StageStatus::Fail);
        assert!(outcome.failure_reason.is_some());
        assert!(outcome.failure_reason.unwrap().contains("42"));
        assert_eq!(
            outcome.context_updates.get("fail_cmd.exit_code"),
            Some(&serde_json::json!(42))
        );
    }

    #[test]
    fn extract_label_finds_exact_last_line() {
        let labels = vec!["BUY".into(), "HOLD".into(), "SELL".into()];
        let response = "Based on analysis, I recommend:\n\nBUY";
        assert_eq!(extract_label(response, &labels), Some("BUY".into()));
    }

    #[test]
    fn extract_label_case_insensitive() {
        let labels = vec!["BUY".into(), "HOLD".into(), "SELL".into()];
        let response = "The recommendation is:\n\nhold";
        assert_eq!(extract_label(response, &labels), Some("HOLD".into()));
    }

    #[test]
    fn extract_label_fallback_to_body_scan() {
        let labels = vec!["BUY".into(), "HOLD".into(), "SELL".into()];
        let response = "I recommend a SELL rating because the player is declining.";
        assert_eq!(extract_label(response, &labels), Some("SELL".into()));
    }

    #[test]
    fn extract_label_returns_none_when_no_match() {
        let labels = vec!["BUY".into(), "HOLD".into(), "SELL".into()];
        let response = "This player is interesting but I need more data.";
        assert_eq!(extract_label(response, &labels), None);
    }
}
