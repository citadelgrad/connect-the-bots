use std::path::Path;

use async_trait::async_trait;
use serde_json::json;

use crate::environment::ExecutionEnvironment;
use crate::tool::{Tool, ToolDefinition};

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit_file".to_string(),
            description: "Edit a file by replacing an exact string with a new string.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["file_path", "old_string", "new_string"],
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact string to find and replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The replacement string"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences (default: false)",
                        "default": false
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
                tool: "edit_file".into(),
                message: "file_path is required".into(),
            })?;

        let old_string = arguments
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| attractor_types::AttractorError::ToolError {
                tool: "edit_file".into(),
                message: "old_string is required".into(),
            })?;

        let new_string = arguments
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| attractor_types::AttractorError::ToolError {
                tool: "edit_file".into(),
                message: "new_string is required".into(),
            })?;

        let replace_all = arguments
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let path = Path::new(file_path);
        let content = env.read_file(path).await?;

        let match_count = content.matches(old_string).count();

        if match_count == 0 {
            return Err(attractor_types::AttractorError::ToolError {
                tool: "edit_file".into(),
                message: format!(
                    "old_string not found in {}",
                    file_path
                ),
            });
        }

        if match_count > 1 && !replace_all {
            // Find line numbers of each occurrence
            let mut occurrences = Vec::new();
            let mut search_start = 0;
            while let Some(pos) = content[search_start..].find(old_string) {
                let abs_pos = search_start + pos;
                let line_num = content[..abs_pos].lines().count();
                occurrences.push(line_num);
                search_start = abs_pos + old_string.len();
            }
            return Err(attractor_types::AttractorError::ToolError {
                tool: "edit_file".into(),
                message: format!(
                    "old_string found {} times in {} (at lines: {:?}). Use replace_all=true to replace all occurrences.",
                    match_count, file_path, occurrences
                ),
            });
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        env.write_file(path, &new_content).await?;

        let replacements = if replace_all { match_count } else { 1 };
        Ok(format!(
            "Successfully replaced {} occurrence(s) in {}",
            replacements, file_path
        ))
    }
}
