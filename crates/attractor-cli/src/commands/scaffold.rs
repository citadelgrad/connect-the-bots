use anyhow;

pub async fn cmd_scaffold(epic_id: &str, output: Option<&std::path::Path>) -> anyhow::Result<()> {
    // Load epic-runner template
    let template = include_str!("../../../../templates/epic-runner.dot");

    // Get epic details via bd show --json
    let mut cmd = tokio::process::Command::new("bd");
    cmd.arg("show")
        .arg(epic_id)
        .arg("--json");

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output_result = cmd.output().await?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        anyhow::bail!("bd show failed: {}", stderr);
    }

    let json_output = String::from_utf8(output_result.stdout)?;
    let epic_array: serde_json::Value = serde_json::from_str(&json_output)?;

    // bd show --json returns an array with one element
    let epic_data = epic_array
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| anyhow::anyhow!("bd show returned empty array"))?;

    let title = epic_data["title"]
        .as_str()
        .unwrap_or("Unknown Epic");
    let description = epic_data["description"]
        .as_str()
        .unwrap_or("");

    // First, update the goal attribute BEFORE replacing EPIC_ID
    let goal_text = format!(
        "Implement all child tasks of epic {}: {}.{}",
        epic_id,
        title,
        if description.is_empty() {
            String::new()
        } else {
            format!(" {}", description)
        }
    );

    let mut pipeline_content = template.replace(
        "goal=\"Implement all child tasks of epic EPIC_ID, closing each as completed.\"",
        &format!("goal=\"{}\"", goal_text.replace('"', "\\\""))
    );

    // Then replace all remaining EPIC_ID placeholders
    pipeline_content = pipeline_content.replace("EPIC_ID", epic_id);

    // Determine output path
    let output_path = if let Some(path) = output {
        path.to_path_buf()
    } else {
        std::path::PathBuf::from(format!("pipelines/{}.dot", epic_id))
    };

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write pipeline file
    std::fs::write(&output_path, &pipeline_content)?;

    // Validate the generated pipeline
    let graph = crate::load_pipeline(&output_path)?;
    let diagnostics = attractor_pipeline::validate(&graph);

    let has_error = diagnostics.iter().any(|d| {
        matches!(d.severity, attractor_pipeline::Severity::Error)
    });

    if has_error {
        println!("⚠ Pipeline generated but has validation errors:");
        for diag in &diagnostics {
            if matches!(diag.severity, attractor_pipeline::Severity::Error) {
                println!("  [ERROR] {}: {}", diag.rule, diag.message);
            }
        }
    }

    // Count nodes
    let node_count = graph.all_nodes().count();

    println!("✓ Pipeline scaffolded");
    println!("  Output: {}", output_path.display());
    println!("  Epic: {} ({})", epic_id, title);
    println!("  Nodes: {}", node_count);
    println!("  Validation: {}", if has_error { "FAILED" } else { "PASSED" });

    if !has_error {
        println!("\nNext steps:");
        println!("1. Review pipeline: cat {}", output_path.display());
        println!("2. Run pipeline: pas run {} -w .", output_path.display());
    }

    Ok(())
}
