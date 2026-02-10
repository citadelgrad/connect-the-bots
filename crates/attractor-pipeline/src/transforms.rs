//! AST transforms for pipeline graphs.
//!
//! Processes the pipeline graph after parsing: applies stylesheets, expands
//! variables in prompts, and performs other AST-level transformations.

use std::collections::HashMap;

use crate::graph::PipelineGraph;
use crate::stylesheet::{apply_stylesheet, parse_stylesheet};

/// Apply all transforms to a pipeline graph in the correct order.
pub fn apply_transforms(graph: &mut PipelineGraph) -> attractor_types::Result<()> {
    // 1. Apply model stylesheet (if present in graph attrs)
    apply_model_stylesheet(graph)?;

    // 2. Expand variables in prompts
    expand_prompt_variables(graph);

    Ok(())
}

/// Apply model stylesheet from graph attributes.
fn apply_model_stylesheet(graph: &mut PipelineGraph) -> attractor_types::Result<()> {
    let stylesheet_str = graph
        .attrs
        .get("model_stylesheet")
        .and_then(|v| match v {
            attractor_dot::AttributeValue::String(s) => Some(s.clone()),
            _ => None,
        });

    if let Some(css) = stylesheet_str {
        let stylesheet = parse_stylesheet(&css)?;
        apply_stylesheet(graph, &stylesheet);
    }

    Ok(())
}

/// Expand `${variable}` references in node prompts using graph-level attributes.
fn expand_prompt_variables(graph: &mut PipelineGraph) {
    // Collect graph-level attribute values for substitution
    let vars: HashMap<String, String> = graph
        .attrs
        .iter()
        .filter_map(|(k, v)| match v {
            attractor_dot::AttributeValue::String(s) => Some((k.clone(), s.clone())),
            attractor_dot::AttributeValue::Integer(i) => Some((k.clone(), i.to_string())),
            attractor_dot::AttributeValue::Boolean(b) => Some((k.clone(), b.to_string())),
            attractor_dot::AttributeValue::Float(f) => Some((k.clone(), f.to_string())),
            attractor_dot::AttributeValue::Duration(_) => None,
        })
        .collect();

    for node in graph.all_nodes_mut() {
        if let Some(ref prompt) = node.prompt {
            let expanded = expand_variables(prompt, &vars);
            if expanded != *prompt {
                node.prompt = Some(expanded);
            }
        }
    }
}

/// Expand `${variable}` patterns in a string.
///
/// Replaces each occurrence of `${key}` with the corresponding value from `vars`.
/// Variables that do not appear in `vars` are left as-is.
pub fn expand_variables(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let pattern = format!("${{{}}}", key);
        result = result.replace(&pattern, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::PipelineGraph;

    fn parse_and_build(dot: &str) -> PipelineGraph {
        let graph = attractor_dot::parse(dot).unwrap();
        PipelineGraph::from_dot(graph).unwrap()
    }

    // ---- expand_variables unit tests ----

    #[test]
    fn expand_single_variable() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "world".to_string());
        assert_eq!(expand_variables("Hello ${name}!", &vars), "Hello world!");
    }

    #[test]
    fn expand_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("greeting".to_string(), "Hello".to_string());
        vars.insert("target".to_string(), "world".to_string());
        let result = expand_variables("${greeting}, ${target}!", &vars);
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn expand_no_matches_leaves_template_unchanged() {
        let vars = HashMap::new();
        let template = "No variables here, just ${unknown} text.";
        assert_eq!(expand_variables(template, &vars), template);
    }

    #[test]
    fn expand_same_variable_multiple_times() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "42".to_string());
        assert_eq!(expand_variables("${x} + ${x} = ?", &vars), "42 + 42 = ?");
    }

    #[test]
    fn expand_empty_template() {
        let mut vars = HashMap::new();
        vars.insert("key".to_string(), "val".to_string());
        assert_eq!(expand_variables("", &vars), "");
    }

    // ---- apply_transforms integration tests ----

    #[test]
    fn apply_transforms_with_model_stylesheet() {
        let dot = r#"digraph G {
            model_stylesheet = "* { llm_model: test-model; }"
            start [shape="Mdiamond"]
            work [label="Work"]
            done [shape="Msquare"]
            start -> work -> done
        }"#;
        let mut graph = parse_and_build(dot);
        apply_transforms(&mut graph).unwrap();

        // All nodes should have llm_model set by the stylesheet
        for node in graph.all_nodes() {
            assert_eq!(
                node.llm_model.as_deref(),
                Some("test-model"),
                "node '{}' should have llm_model set by stylesheet",
                node.id
            );
        }
    }

    #[test]
    fn apply_transforms_expands_prompt_variables() {
        let dot = r#"digraph G {
            language = "Rust"
            version = "2024"
            start [shape="Mdiamond"]
            work [prompt="Write a ${language} program for ${version}"]
            done [shape="Msquare"]
            start -> work -> done
        }"#;
        let mut graph = parse_and_build(dot);
        apply_transforms(&mut graph).unwrap();

        let work = graph.node("work").unwrap();
        assert_eq!(
            work.prompt.as_deref(),
            Some("Write a Rust program for 2024")
        );
    }

    #[test]
    fn apply_transforms_no_stylesheet_is_ok() {
        let dot = r#"digraph G {
            start [shape="Mdiamond"]
            done [shape="Msquare"]
            start -> done
        }"#;
        let mut graph = parse_and_build(dot);
        // Should not error even without a model_stylesheet attr
        apply_transforms(&mut graph).unwrap();
    }

    #[test]
    fn apply_transforms_prompt_without_variables_unchanged() {
        let dot = r#"digraph G {
            start [shape="Mdiamond"]
            work [prompt="No variables here"]
            done [shape="Msquare"]
            start -> work -> done
        }"#;
        let mut graph = parse_and_build(dot);
        apply_transforms(&mut graph).unwrap();

        let work = graph.node("work").unwrap();
        assert_eq!(work.prompt.as_deref(), Some("No variables here"));
    }
}
