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
