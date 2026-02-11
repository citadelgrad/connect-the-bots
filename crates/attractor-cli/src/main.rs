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
        } => {
            cmd_run(&pipeline, workdir.as_deref(), &logs, dry_run).await?;
        }
        Commands::Validate { pipeline } => {
            cmd_validate(&pipeline)?;
        }
        Commands::Info { pipeline } => {
            cmd_info(&pipeline)?;
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

    let executor = attractor_pipeline::PipelineExecutor::with_default_registry();
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
