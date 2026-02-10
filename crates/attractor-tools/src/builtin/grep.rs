use std::path::Path;

use async_trait::async_trait;
use serde_json::json;

use crate::environment::{ExecutionEnvironment, GrepOptions};
use crate::tool::{Tool, ToolDefinition};
use crate::truncation::{truncate_output, TruncationMode};

const MAX_OUTPUT_CHARS: usize = 20_000;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".to_string(),
            description: "Search file contents using a regex pattern.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to search in (defaults to working directory)"
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "Case insensitive search (default: false)"
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
                tool: "grep".into(),
                message: "pattern is required".into(),
            })?;

        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .map(Path::new)
            .unwrap_or_else(|| env.working_directory());

        let case_insensitive = arguments
            .get("case_insensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let options = GrepOptions {
            case_insensitive,
            include_line_numbers: true,
            ..Default::default()
        };

        let output = env.grep(pattern, path, &options).await?;
        Ok(truncate_output(&output, MAX_OUTPUT_CHARS, TruncationMode::Tail))
    }
}
