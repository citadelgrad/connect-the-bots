use std::path::Path;

use async_trait::async_trait;
use serde_json::json;

use crate::environment::ExecutionEnvironment;
use crate::tool::{Tool, ToolDefinition};

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file, creating parent directories if needed."
                .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["file_path", "content"],
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
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
                tool: "write_file".into(),
                message: "file_path is required".into(),
            })?;

        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| attractor_types::AttractorError::ToolError {
                tool: "write_file".into(),
                message: "content is required".into(),
            })?;

        env.write_file(Path::new(file_path), content).await?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            file_path
        ))
    }
}
