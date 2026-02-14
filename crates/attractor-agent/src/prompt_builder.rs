//! System prompt builder with project doc discovery.
//!
//! Assembles the agent's system prompt from multiple sources: base prompt,
//! project docs, node-specific instructions, and tool descriptions.

use std::path::Path;

/// Builds a complete system prompt for an agent session.
pub struct SystemPromptBuilder {
    sections: Vec<PromptSection>,
}

#[derive(Debug, Clone)]
struct PromptSection {
    heading: String,
    content: String,
    priority: u8, // 0 = highest
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    /// Add the base system prompt (always first).
    pub fn with_base(mut self, prompt: &str) -> Self {
        self.sections.push(PromptSection {
            heading: String::new(),
            content: prompt.to_string(),
            priority: 0,
        });
        self
    }

    /// Add the pipeline goal.
    pub fn with_goal(mut self, goal: &str) -> Self {
        if !goal.is_empty() {
            self.sections.push(PromptSection {
                heading: "Goal".to_string(),
                content: goal.to_string(),
                priority: 1,
            });
        }
        self
    }

    /// Add node-specific prompt/instructions.
    pub fn with_node_prompt(mut self, prompt: &str) -> Self {
        self.sections.push(PromptSection {
            heading: "Task".to_string(),
            content: prompt.to_string(),
            priority: 2,
        });
        self
    }

    /// Add tool descriptions as a section.
    pub fn with_tools(mut self, tool_definitions: &[attractor_tools::ToolDefinition]) -> Self {
        if !tool_definitions.is_empty() {
            let mut content = String::from("You have access to the following tools:\n\n");
            for tool in tool_definitions {
                content.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
            }
            self.sections.push(PromptSection {
                heading: "Available Tools".to_string(),
                content,
                priority: 3,
            });
        }
        self
    }

    /// Add project documentation from discovered files.
    pub fn with_project_docs(mut self, docs: &[ProjectDoc]) -> Self {
        for doc in docs {
            self.sections.push(PromptSection {
                heading: format!("Project Context: {}", doc.source),
                content: doc.content.clone(),
                priority: 4,
            });
        }
        self
    }

    /// Add context variables as key-value pairs.
    pub fn with_context(
        mut self,
        context: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Self {
        if !context.is_empty() {
            let mut content = String::new();
            for (key, value) in context {
                content.push_str(&format!("- {}: {}\n", key, value));
            }
            self.sections.push(PromptSection {
                heading: "Context Variables".to_string(),
                content,
                priority: 5,
            });
        }
        self
    }

    /// Build the final system prompt.
    pub fn build(mut self) -> String {
        self.sections.sort_by_key(|s| s.priority);
        let mut parts = Vec::new();
        for section in &self.sections {
            if section.heading.is_empty() {
                parts.push(section.content.clone());
            } else {
                parts.push(format!("# {}\n\n{}", section.heading, section.content));
            }
        }
        parts.join("\n\n")
    }
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A discovered project document.
#[derive(Debug, Clone)]
pub struct ProjectDoc {
    pub source: String,
    pub content: String,
}

/// Discover project documentation files in a directory.
/// Looks for README.md, CLAUDE.md, .cursorrules, etc.
pub async fn discover_project_docs(project_root: &Path) -> Vec<ProjectDoc> {
    let candidates = [
        ".attractor/CLAUDE.md",
        "CLAUDE.md",
        "README.md",
        ".cursorrules",
        ".github/copilot-instructions.md",
        "CONTRIBUTING.md",
    ];

    let mut docs = Vec::new();
    for name in &candidates {
        let path = project_root.join(name);
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            // Truncate long docs
            let truncated = if content.len() > 10_000 {
                format!("{}...\n\n(truncated)", &content[..10_000])
            } else {
                content
            };
            docs.push(ProjectDoc {
                source: name.to_string(),
                content: truncated,
            });
        }
    }
    docs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn build_with_base_prompt_only() {
        let prompt = SystemPromptBuilder::new()
            .with_base("You are a helpful assistant.")
            .build();

        assert_eq!(prompt, "You are a helpful assistant.");
    }

    #[test]
    fn build_with_multiple_sections_ordered_correctly() {
        let prompt = SystemPromptBuilder::new()
            .with_node_prompt("Refactor the auth module.") // priority 2
            .with_base("You are a coding agent.") // priority 0
            .with_goal("Improve code quality.") // priority 1
            .build();

        // Sections should be ordered by priority regardless of insertion order.
        // Base (priority 0) should come first.
        assert!(prompt.starts_with("You are a coding agent."));

        // Goal (priority 1) before Task (priority 2).
        let goal_pos = prompt.find("# Goal").expect("Goal section missing");
        let task_pos = prompt.find("# Task").expect("Task section missing");
        assert!(
            goal_pos < task_pos,
            "Goal section should appear before Task section"
        );

        // Verify content is present.
        assert!(prompt.contains("Improve code quality."));
        assert!(prompt.contains("Refactor the auth module."));
    }

    #[test]
    fn empty_goal_is_skipped() {
        let prompt = SystemPromptBuilder::new()
            .with_base("Base prompt.")
            .with_goal("") // should be skipped
            .with_node_prompt("Do something.")
            .build();

        assert!(!prompt.contains("# Goal"));
        assert!(prompt.contains("Base prompt."));
        assert!(prompt.contains("# Task"));
    }

    #[test]
    fn with_project_docs_adds_sections() {
        let docs = vec![
            ProjectDoc {
                source: "README.md".to_string(),
                content: "# My Project\nA cool project.".to_string(),
            },
            ProjectDoc {
                source: "CLAUDE.md".to_string(),
                content: "Use Rust.".to_string(),
            },
        ];

        let prompt = SystemPromptBuilder::new()
            .with_base("Base.")
            .with_project_docs(&docs)
            .build();

        assert!(prompt.contains("# Project Context: README.md"));
        assert!(prompt.contains("A cool project."));
        assert!(prompt.contains("# Project Context: CLAUDE.md"));
        assert!(prompt.contains("Use Rust."));
    }

    #[test]
    fn with_tools_adds_tool_section() {
        let tools = vec![attractor_tools::ToolDefinition {
            name: "read_file".to_string(),
            description: "Read a file from disk.".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];

        let prompt = SystemPromptBuilder::new()
            .with_base("Base.")
            .with_tools(&tools)
            .build();

        assert!(prompt.contains("# Available Tools"));
        assert!(prompt.contains("- **read_file**: Read a file from disk."));
    }

    #[test]
    fn empty_tools_skipped() {
        let prompt = SystemPromptBuilder::new()
            .with_base("Base.")
            .with_tools(&[])
            .build();

        assert!(!prompt.contains("Available Tools"));
    }

    #[test]
    fn with_context_adds_variables() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "project".to_string(),
            serde_json::Value::String("attractor".to_string()),
        );

        let prompt = SystemPromptBuilder::new()
            .with_base("Base.")
            .with_context(&ctx)
            .build();

        assert!(prompt.contains("# Context Variables"));
        assert!(prompt.contains("- project: \"attractor\""));
    }

    #[test]
    fn empty_context_skipped() {
        let ctx: HashMap<String, serde_json::Value> = HashMap::new();
        let prompt = SystemPromptBuilder::new()
            .with_base("Base.")
            .with_context(&ctx)
            .build();

        assert!(!prompt.contains("Context Variables"));
    }

    #[tokio::test]
    async fn discover_project_docs_finds_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create some candidate files.
        tokio::fs::write(root.join("README.md"), "# Hello\nWorld")
            .await
            .unwrap();
        tokio::fs::write(root.join("CLAUDE.md"), "Use async Rust.")
            .await
            .unwrap();

        let docs = discover_project_docs(root).await;

        assert_eq!(docs.len(), 2);

        // CLAUDE.md is checked first in the candidate list.
        assert_eq!(docs[0].source, "CLAUDE.md");
        assert_eq!(docs[0].content, "Use async Rust.");

        assert_eq!(docs[1].source, "README.md");
        assert_eq!(docs[1].content, "# Hello\nWorld");
    }

    #[tokio::test]
    async fn discover_project_docs_truncates_long_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a file longer than 10,000 characters.
        let long_content = "x".repeat(15_000);
        tokio::fs::write(root.join("README.md"), &long_content)
            .await
            .unwrap();

        let docs = discover_project_docs(root).await;

        assert_eq!(docs.len(), 1);
        assert!(docs[0].content.len() < long_content.len());
        assert!(docs[0].content.ends_with("...\n\n(truncated)"));
    }

    #[tokio::test]
    async fn discover_project_docs_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let docs = discover_project_docs(dir.path()).await;
        assert!(docs.is_empty());
    }
}
