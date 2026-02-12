use leptos::prelude::*;
use leptos::server_fn::error::NoCustomError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GenerateResponse {
    pub prd: String,
    pub spec: String,
}

#[server]
pub async fn generate_prd_spec(prompt: String) -> Result<GenerateResponse, ServerFnError<NoCustomError>> {
    use tokio::process::Command;
    use tracing::{error, info};

    info!("Generating PRD/Spec for prompt: {}", prompt);

    let system_prompt = r#"You are a technical requirements expert. Generate a PRD (Product Requirements Document) and a technical specification from the user's prompt.

Return ONLY valid JSON with exactly this structure:
{
  "prd": "markdown-formatted PRD document",
  "spec": "markdown-formatted technical specification"
}

The PRD should include:
- Project Overview
- Goals and Objectives
- User Stories
- Success Metrics
- Timeline/Milestones

The Spec should include:
- Architecture Overview
- Technical Components
- API Design
- Data Models
- Implementation Plan"#;

    // Execute claude CLI
    let output = Command::new("claude")
        .arg("-p")
        .arg("--output-format")
        .arg("json")
        .arg("--system-prompt")
        .arg(system_prompt)
        .arg(&prompt)
        .output()
        .await
        .map_err(|e| {
            error!("Failed to spawn claude: {}", e);
            ServerFnError::<NoCustomError>::ServerError(format!("Failed to execute claude CLI: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Claude command failed: {}", stderr);
        return Err(ServerFnError::<NoCustomError>::ServerError(format!(
            "Claude CLI failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    info!("Claude output received, length: {} bytes", stdout.len());

    // Parse JSON response
    let response: GenerateResponse = serde_json::from_str(&stdout).map_err(|e| {
        error!("Failed to parse JSON response: {}", e);
        error!("Raw output: {}", stdout);
        ServerFnError::<NoCustomError>::ServerError(format!(
            "Failed to parse JSON response from Claude: {}",
            e
        ))
    })?;

    info!("Successfully generated PRD and Spec");
    Ok(response)
}
