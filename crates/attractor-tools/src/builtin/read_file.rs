use std::path::Path;

use async_trait::async_trait;
use serde_json::json;

use crate::environment::ExecutionEnvironment;
use crate::tool::{Tool, ToolDefinition};
use crate::truncation::{truncate_output, TruncationMode};

const MAX_OUTPUT_CHARS: usize = 50_000;

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read a file from the filesystem. Returns content with line numbers."
                .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["file_path"],
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path to the file to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start reading from (1-based)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read",
                        "default": 2000
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
        let file_path = arguments
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| attractor_types::AttractorError::ToolError {
                tool: "read_file".into(),
                message: "file_path is required".into(),
            })?;

        let offset = arguments
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(2000);

        let content = env.read_file(Path::new(file_path)).await?;

        let lines: Vec<&str> = content.lines().collect();
        let start = offset.unwrap_or(1).saturating_sub(1);
        let end = (start + limit).min(lines.len());
        let selected = &lines[start..end];

        let width = if end == 0 { 1 } else { end.to_string().len() };
        let numbered: Vec<String> = selected
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>width$} | {}", start + i + 1, line, width = width))
            .collect();

        let output = numbered.join("\n");
        Ok(truncate_output(&output, MAX_OUTPUT_CHARS, TruncationMode::HeadTail))
    }
}
