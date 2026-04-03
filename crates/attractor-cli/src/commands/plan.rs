use anyhow;

pub async fn cmd_plan(
    prd: bool,
    spec: bool,
    from_prompt: Option<&str>,
    output: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    // Validate: exactly one of --prd or --spec must be true
    if !prd && !spec {
        anyhow::bail!("Must specify either --prd or --spec");
    }

    // Determine template and default output path
    let (template_content, default_output) = if prd {
        (
            include_str!("../../../../templates/prd-template.md"),
            std::path::Path::new(".pas/prd.md"),
        )
    } else {
        (
            include_str!("../../../../templates/spec-template.md"),
            std::path::Path::new(".pas/spec.md"),
        )
    };

    let output_path = output.unwrap_or(default_output);

    if let Some(prompt_desc) = from_prompt {
        // AI mode: Use Claude CLI to generate document
        generate_with_claude(prompt_desc, template_content, output_path, prd).await?;
    } else {
        // Simple mode: Copy template to output
        copy_template(template_content, output_path)?;
    }

    // Print success message
    println!("Created {} at {}", if prd { "PRD" } else { "spec" }, output_path.display());

    if from_prompt.is_none() {
        println!("\nNext steps:");
        println!("1. Edit {} to fill in your details", output_path.display());
        println!("2. Replace all [bracketed placeholders] with actual content");
        if prd {
            println!("3. Create a beads epic: bd create --type=epic");
            println!("4. Link the epic ID in the metadata section");
        } else {
            println!("3. Create beads tasks: bd decompose {}", output_path.display());
        }
    }

    Ok(())
}

fn copy_template(content: &str, output: &std::path::Path) -> anyhow::Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, content)?;
    Ok(())
}

async fn generate_with_claude(
    description: &str,
    template: &str,
    output: &std::path::Path,
    is_prd: bool,
) -> anyhow::Result<()> {
    let doc_type = if is_prd { "PRD" } else { "Technical Specification" };

    // Build prompt for Claude
    let prompt = format!(
        "Generate a {} document following this exact template format:\n\n{}\n\n\
        User request: {}\n\n\
        Instructions:\n\
        1. Replace all [bracketed placeholders] with content based on the user request\n\
        2. Keep the exact section structure from the template\n\
        3. Fill in Status: Draft, Author: Claude, Created: {} (today's date)\n\
        4. Write concrete, specific content - no placeholder text or [brackets]\n\
        5. Output ONLY the markdown document, no explanations or commentary\n\
        6. If the user request is vague, make reasonable assumptions and document them",
        doc_type,
        template,
        description,
        chrono::Utc::now().format("%Y-%m-%d")
    );

    // Shell out to claude CLI
    let mut cmd = tokio::process::Command::new("claude");
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--dangerously-skip-permissions")
        .arg("--no-session-persistence");

    // Capture output
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output_result = cmd.output().await?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        anyhow::bail!("Claude CLI failed: {}", stderr);
    }

    let generated_content = String::from_utf8(output_result.stdout)?;

    // Create parent directory if needed
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write generated content
    std::fs::write(output, generated_content)?;

    Ok(())
}
