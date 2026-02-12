//! CLI binary for running and validating Attractor pipelines.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "attractor", version, about = "DOT-based pipeline runner for AI workflows")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a pipeline from a .dot file
    Run {
        /// Path to the pipeline .dot file
        pipeline: PathBuf,

        /// Working directory for tool execution
        #[arg(short, long)]
        workdir: Option<PathBuf>,

        /// Logs output directory
        #[arg(short, long, default_value = ".attractor/logs")]
        logs: PathBuf,

        /// Don't actually call LLMs (dry run)
        #[arg(long)]
        dry_run: bool,

        /// Maximum total spend across all nodes (USD). Pipeline aborts if exceeded.
        #[arg(long)]
        max_budget_usd: Option<f64>,

        /// Maximum number of node executions before aborting. Prevents runaway loops. Default: 200.
        #[arg(long, default_value = "200")]
        max_steps: u64,
    },

    /// Validate a pipeline .dot file
    Validate {
        /// Path to the pipeline .dot file
        pipeline: PathBuf,
    },

    /// Show information about a pipeline
    Info {
        /// Path to the pipeline .dot file
        pipeline: PathBuf,
    },

    /// Generate PRD or spec documents from templates
    Plan {
        /// Generate a PRD document
        #[arg(long, conflicts_with = "spec")]
        prd: bool,

        /// Generate a spec document
        #[arg(long, conflicts_with = "prd")]
        spec: bool,

        /// Generate from a prompt description (uses Claude CLI)
        #[arg(long)]
        from_prompt: Option<String>,

        /// Output file path (defaults: .attractor/prd.md or .attractor/spec.md)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Decompose a spec into beads epic and tasks
    Decompose {
        /// Path to the spec markdown file
        spec_path: PathBuf,

        /// Print the generated shell commands without executing them
        #[arg(long)]
        dry_run: bool,
    },

    /// Scaffold a pipeline from a beads epic
    Scaffold {
        /// Beads epic ID (e.g., beads-xxx)
        epic_id: String,

        /// Output file path (default: pipelines/<epic-id>.dot)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Setup tracing
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    match cli.command {
        Commands::Run {
            pipeline,
            workdir,
            logs,
            dry_run,
            max_budget_usd,
            max_steps,
        } => {
            cmd_run(&pipeline, workdir.as_deref(), &logs, dry_run, max_budget_usd, max_steps).await?;
        }
        Commands::Validate { pipeline } => {
            cmd_validate(&pipeline)?;
        }
        Commands::Info { pipeline } => {
            cmd_info(&pipeline)?;
        }
        Commands::Plan { prd, spec, from_prompt, output } => {
            cmd_plan(prd, spec, from_prompt.as_deref(), output.as_deref()).await?;
        }
        Commands::Decompose { spec_path, dry_run } => {
            cmd_decompose(&spec_path, dry_run).await?;
        }
        Commands::Scaffold { epic_id, output } => {
            cmd_scaffold(&epic_id, output.as_deref()).await?;
        }
    }

    Ok(())
}

fn load_pipeline(path: &std::path::Path) -> anyhow::Result<attractor_pipeline::PipelineGraph> {
    let source = std::fs::read_to_string(path)?;
    let dot = attractor_dot::parse(&source)?;
    let graph = attractor_pipeline::PipelineGraph::from_dot(dot)?;
    Ok(graph)
}

fn cmd_validate(path: &std::path::Path) -> anyhow::Result<()> {
    let graph = load_pipeline(path)?;
    let diagnostics = attractor_pipeline::validate(&graph);

    if diagnostics.is_empty() {
        println!("Pipeline is valid");
        return Ok(());
    }

    let mut has_error = false;
    for diag in &diagnostics {
        let severity = match diag.severity {
            attractor_pipeline::Severity::Error => {
                has_error = true;
                "ERROR"
            }
            attractor_pipeline::Severity::Warning => "WARN",
            attractor_pipeline::Severity::Info => "INFO",
        };
        println!("[{}] {}: {}", severity, diag.rule, diag.message);
    }

    if has_error {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_info(path: &std::path::Path) -> anyhow::Result<()> {
    let graph = load_pipeline(path)?;

    println!("Pipeline: {}", graph.name);
    if !graph.goal.is_empty() {
        println!("Goal: {}", graph.goal);
    }

    let node_count = graph.all_nodes().count();
    let edge_count = graph.all_edges().len();
    println!("Nodes: {}", node_count);
    println!("Edges: {}", edge_count);

    if let Some(start) = graph.start_node() {
        println!("Start: {} ({})", start.id, start.label);
    }
    if let Some(exit) = graph.exit_node() {
        println!("Exit: {} ({})", exit.id, exit.label);
    }

    // List nodes with their types
    println!("\nNodes:");
    for node in graph.all_nodes() {
        let node_type = node.node_type.as_deref().unwrap_or("(default)");
        println!(
            "  {} [{}] shape={} type={}",
            node.id, node.label, node.shape, node_type
        );
    }

    Ok(())
}

async fn cmd_run(
    path: &std::path::Path,
    workdir: Option<&std::path::Path>,
    _logs: &std::path::Path,
    dry_run: bool,
    max_budget_usd: Option<f64>,
    max_steps: u64,
) -> anyhow::Result<()> {
    let graph = load_pipeline(path)?;

    println!("Running pipeline: {}", graph.name);
    if !graph.goal.is_empty() {
        println!("Goal: {}", graph.goal);
    }
    if dry_run {
        println!("(dry run mode -- no LLM calls)");
    }

    // Set up the pipeline context with workdir
    let context = attractor_types::Context::new();
    if let Some(dir) = workdir {
        let abs = std::fs::canonicalize(dir)?;
        context
            .set(
                "workdir",
                serde_json::Value::String(abs.to_string_lossy().into_owned()),
            )
            .await;
        println!("Working directory: {}", abs.display());
    }
    if dry_run {
        context
            .set("dry_run", serde_json::Value::Bool(true))
            .await;
    }

    // Safety limits
    if let Some(budget) = max_budget_usd {
        context
            .set("max_budget_usd", serde_json::json!(budget))
            .await;
        println!("Budget limit: ${:.2}", budget);
    }
    context
        .set("max_steps", serde_json::json!(max_steps))
        .await;
    println!("Step limit: {}", max_steps);

    let interviewer = std::sync::Arc::new(attractor_pipeline::ConsoleInterviewer);
    let registry =
        attractor_pipeline::default_registry_with_interviewer(interviewer);
    let executor = attractor_pipeline::PipelineExecutor::new(registry);
    let result = executor.run_with_context(&graph, context).await?;

    println!("\nPipeline completed");
    println!("Completed nodes: {:?}", result.completed_nodes);

    // Print cost summary
    let total_cost: f64 = result
        .final_context
        .iter()
        .filter(|(k, _)| k.ends_with(".cost_usd"))
        .filter_map(|(_, v)| v.as_f64())
        .sum();
    if total_cost > 0.0 {
        println!("Total cost: ${:.4}", total_cost);
    }

    Ok(())
}

async fn cmd_plan(
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
            include_str!("../../../templates/prd-template.md"),
            std::path::Path::new(".attractor/prd.md"),
        )
    } else {
        (
            include_str!("../../../templates/spec-template.md"),
            std::path::Path::new(".attractor/spec.md"),
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

async fn cmd_decompose(spec_path: &std::path::Path, dry_run: bool) -> anyhow::Result<()> {
    // Read the spec file
    let spec_content = std::fs::read_to_string(spec_path)?;

    // Build prompt for Claude to generate bd commands
    let prompt = format!(
        "Read this technical specification and generate a shell script of beads (bd) CLI commands to create an epic and tasks.\n\n\
        SPEC:\n{}\n\n\
        INSTRUCTIONS:\n\
        1. Extract the title from the spec (usually in the first heading)\n\
        2. Extract implementation phases/tasks from the '## Implementation Phases' or similar section\n\
        3. Create an epic first: EPIC_ID=$(bd create --title='TITLE' --type=epic --priority=P1 --description='OVERVIEW' --silent)\n\
        4. For each task/phase: TASK_N=$(bd create --title='TITLE' --type=task --priority=PN --description='DESC' --silent)\n\
        5. Add dependencies using: bd dep add $BLOCKED_TASK $BLOCKER_TASK (the blocked task depends on the blocker)\n\
        6. Output ONLY executable shell commands - no markdown code fences (```), no explanations, no commentary\n\
        7. Use set -e at the start so it fails fast on errors\n\
        8. Echo the epic ID at the end: echo $EPIC_ID\n\
        9. Priority should be P2 for most tasks unless critical (P1) or backlog (P3/P4)\n\
        10. Task descriptions should be 1-2 sentences summarizing what needs to be done\n\
        11. CRITICAL: Do NOT wrap output in markdown code fences. Start directly with 'set -e'",
        spec_content
    );

    // Call Claude CLI with JSON output format
    let mut cmd = tokio::process::Command::new("claude");
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--output-format")
        .arg("json")
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

    let output_json = String::from_utf8(output_result.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(&output_json)?;

    let mut shell_commands = parsed["result"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Claude output missing 'result' field"))?
        .to_string();

    // Strip markdown code fences if present
    if shell_commands.starts_with("```") {
        let lines: Vec<&str> = shell_commands.lines().collect();
        if lines.len() > 2 && lines[0].starts_with("```") && lines[lines.len() - 1] == "```" {
            shell_commands = lines[1..lines.len() - 1].join("\n");
        }
    }

    // Prepend shebang (and set -e if not already present)
    let full_script = if shell_commands.starts_with("set -e") {
        format!("#!/bin/sh\n{}", shell_commands)
    } else {
        format!("#!/bin/sh\nset -e\n\n{}", shell_commands)
    };

    if dry_run {
        println!("Generated shell commands (dry run):\n");
        println!("{}", full_script);
        return Ok(());
    }

    // Execute the shell script
    let temp_script = std::env::temp_dir().join("attractor-decompose.sh");
    std::fs::write(&temp_script, &full_script)?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_script)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&temp_script, perms)?;
    }

    // Execute with sh -e
    let exec_result = tokio::process::Command::new("sh")
        .arg("-e")
        .arg(&temp_script)
        .output()
        .await?;

    // Clean up temp file
    std::fs::remove_file(&temp_script).ok();

    if !exec_result.status.success() {
        let stderr = String::from_utf8_lossy(&exec_result.stderr);
        anyhow::bail!("Shell script execution failed: {}", stderr);
    }

    let output_text = String::from_utf8(exec_result.stdout)?;

    // Extract epic ID from last line of output
    let epic_id = output_text
        .lines()
        .last()
        .unwrap_or("")
        .trim();

    // Count tasks created (number of lines with TASK_N=)
    let task_count = shell_commands.matches("TASK_").count();

    // Count dependencies (number of bd dep add lines)
    let dep_count = shell_commands.matches("bd dep add").count();

    println!("✓ Decomposition complete");
    println!("  Epic ID: {}", epic_id);
    println!("  Tasks created: {}", task_count);
    println!("  Dependencies: {}", dep_count);
    println!("\nNext steps:");
    println!("1. Review tasks: bd list");
    println!("2. Generate pipeline: attractor scaffold {}", epic_id);

    Ok(())
}

async fn cmd_scaffold(epic_id: &str, output: Option<&std::path::Path>) -> anyhow::Result<()> {
    // Load epic-runner template
    let template = include_str!("../../../templates/epic-runner.dot");

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
    let graph = load_pipeline(&output_path)?;
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
        println!("2. Run pipeline: attractor run {} -w .", output_path.display());
    }

    Ok(())
}
