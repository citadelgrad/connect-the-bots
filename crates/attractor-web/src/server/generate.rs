use leptos::prelude::*;
use leptos::server_fn::error::NoCustomError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GenerateResponse {
    pub prd: String,
    pub spec: String,
    pub session_id: Option<String>,
}

/// Stream event from claude CLI's `--output-format stream-json`
#[derive(Serialize, Deserialize, Clone, Debug)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    result: String,
}

#[server]
pub async fn generate_prd_spec(prompt: String) -> Result<GenerateResponse, ServerFnError<NoCustomError>> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;
    use tracing::{error, info};
    use uuid::Uuid;
    use std::process::Stdio;

    info!("Generating PRD/Spec with streaming for prompt: {}", prompt);

    // Generate unique session ID for this generation
    let session_id = Uuid::new_v4().to_string();
    info!("Session ID: {}", session_id);

    let system_prompt = r##"You are a technical requirements expert. Generate a PRD (Product Requirements Document) and a technical specification from the user's prompt.

First write the PRD with these sections:
# PRD
## Project Overview
## Goals and Objectives
## User Stories
## Success Metrics
## Timeline/Milestones

Then write the technical specification with these sections:
# Technical Specification
## Architecture Overview
## Technical Components
## API Design
## Data Models
## Implementation Plan

Write in markdown format. Start with "# PRD" header and use "# Technical Specification" to separate the two documents."##;

    // Execute claude CLI with streaming output
    let mut child = Command::new("claude")
        .arg("-p")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--session-id")
        .arg(&session_id)
        .arg("--system-prompt")
        .arg(system_prompt)
        .arg(&prompt)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            error!("Failed to spawn claude: {}", e);
            ServerFnError::<NoCustomError>::ServerError(format!("Failed to execute claude CLI: {}", e))
        })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        ServerFnError::<NoCustomError>::ServerError("Failed to capture stdout".into())
    })?;

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut accumulated_text = String::new();

    // Read stream events line-by-line
    while let Some(line) = lines.next_line().await.map_err(|e| {
        error!("Failed to read line from stream: {}", e);
        ServerFnError::<NoCustomError>::ServerError(format!("Stream read error: {}", e))
    })? {
        // Parse NDJSON event
        let event: StreamEvent = serde_json::from_str(&line).map_err(|e| {
            error!("Failed to parse stream event: {}", e);
            error!("Raw line: {}", line);
            ServerFnError::<NoCustomError>::ServerError(format!("Failed to parse stream event: {}", e))
        })?;

        match event.event_type.as_str() {
            "text" => {
                // Accumulate text chunks
                accumulated_text.push_str(&event.text);

                // Publish to SSE channel for live updates
                crate::server::stream::publish_event(
                    &session_id,
                    serde_json::to_string(&event).unwrap_or_default(),
                );
            }
            "result" => {
                // Final event with full result
                info!("Received final result event");
                if !event.result.is_empty() {
                    accumulated_text = event.result;
                }
                break;
            }
            _ => {
                // Ignore other event types
                info!("Received event type: {}", event.event_type);
            }
        }
    }

    // Wait for process to complete
    let status = child.wait().await.map_err(|e| {
        error!("Failed to wait for claude process: {}", e);
        ServerFnError::<NoCustomError>::ServerError(format!("Process wait error: {}", e))
    })?;

    if !status.success() {
        return Err(ServerFnError::<NoCustomError>::ServerError(format!(
            "Claude CLI exited with error: {}",
            status
        )));
    }

    // Split PRD from Spec using markdown headers
    let (prd, spec) = split_prd_and_spec(&accumulated_text);

    info!(
        "Successfully generated PRD ({} chars) and Spec ({} chars)",
        prd.len(),
        spec.len()
    );

    // Send a final completion event
    let completion_event = serde_json::json!({
        "type": "complete",
        "prd_length": prd.len(),
        "spec_length": spec.len(),
    });
    crate::server::stream::publish_event(
        &session_id,
        serde_json::to_string(&completion_event).unwrap_or_default(),
    );

    Ok(GenerateResponse {
        prd,
        spec,
        session_id: Some(session_id),
    })
}

/// Split accumulated text into PRD and Spec based on markdown headers.
///
/// Looks for "# Technical Specification" header to separate the two documents.
/// Falls back to splitting at midpoint if header not found.
fn split_prd_and_spec(text: &str) -> (String, String) {
    // Look for "# Technical Specification" header (case-insensitive)
    let spec_markers = [
        "# Technical Specification",
        "# Technical Spec",
        "#Technical Specification",
        "## Technical Specification",
    ];

    for marker in &spec_markers {
        if let Some(pos) = text.to_lowercase().find(&marker.to_lowercase()) {
            let prd = text[..pos].trim().to_string();
            let spec = text[pos..].trim().to_string();
            return (prd, spec);
        }
    }

    // Fallback: split at midpoint if no marker found
    tracing::warn!("No specification header found, splitting at midpoint");
    let midpoint = text.len() / 2;
    let prd = text[..midpoint].trim().to_string();
    let spec = text[midpoint..].trim().to_string();
    (prd, spec)
}
