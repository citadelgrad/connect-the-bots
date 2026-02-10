use std::path::Path;

use async_trait::async_trait;
use serde_json::json;

use crate::environment::ExecutionEnvironment;
use crate::tool::{Tool, ToolDefinition};
use crate::truncation::{truncate_output, TruncationMode};

const MAX_OUTPUT_CHARS: usize = 20_000;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "glob".to_string(),
            description: "Find files matching a glob pattern.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match files against"
                    },
                    "path": {
                        "type": "string",
                        "description": "Base directory to search in (defaults to working directory)"
                    }
                }
            }),
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        env: &dyn ExecutionEnvironment,
    ) -> attractor_types::Result<String> {
        let pattern = arguments
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| attractor_types::AttractorError::ToolError {
                tool: "glob".into(),
                message: "pattern is required".into(),
            })?;

        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .map(Path::new)
            .unwrap_or_else(|| env.working_directory());

        let matches = env.glob_files(pattern, path).await?;

        let output = matches
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(truncate_output(&output, MAX_OUTPUT_CHARS, TruncationMode::Tail))
    }
}
