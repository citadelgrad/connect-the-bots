//! CLI binary for running and validating Attractor pipelines.

mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use commands::{cmd_decompose, cmd_generate, cmd_info, cmd_plan, cmd_run, cmd_scaffold, cmd_validate, validate_decomposition};

#[derive(Parser)]
#[command(name = "pas", version, about = "Pascal's Discrete Attractor — DOT-based pipeline runner for AI workflows")]
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

        /// Logs output directory (default: .pas/logs/<pipeline>-<hash>)
        #[arg(short, long)]
        logs: Option<PathBuf>,

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

        /// Output file path (defaults: .pas/prd.md or .pas/spec.md)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Decompose a spec into beads epic and tasks
    Decompose {
        /// Path to the spec markdown file
        spec_path: PathBuf,

        /// Print the generated shell commands without executing them
        #[arg(long, conflicts_with = "validate")]
        dry_run: bool,

        /// Validate existing tickets against spec (skip LLM, just check coverage)
        #[arg(long, conflicts_with = "dry_run")]
        validate: Option<String>,
    },

    /// Scaffold a pipeline from a beads epic
    Scaffold {
        /// Beads epic ID (e.g., beads-xxx)
        epic_id: String,

        /// Output file path (default: pipelines/<epic-id>.dot)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Generate a pipeline directly from PRD and/or spec files (no beads)
    Generate {
        /// Input files: spec only, or prd then spec (positional)
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,

        /// PRD file path (alternative to positional)
        #[arg(long)]
        prd: Option<PathBuf>,

        /// Spec file path (alternative to positional)
        #[arg(long)]
        spec: Option<PathBuf>,

        /// Output .dot file path (default: pipelines/<spec-stem>.dot)
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
            cmd_run(&pipeline, workdir.as_deref(), logs.as_deref(), dry_run, max_budget_usd, max_steps).await?;
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
        Commands::Decompose { spec_path, dry_run, validate } => {
            if let Some(epic_id) = validate {
                let spec_content = std::fs::read_to_string(&spec_path)?;
                validate_decomposition(&spec_content, Some(&epic_id)).await?;
            } else {
                cmd_decompose(&spec_path, dry_run).await?;
            }
        }
        Commands::Scaffold { epic_id, output } => {
            cmd_scaffold(&epic_id, output.as_deref()).await?;
        }
        Commands::Generate { files, prd, spec, output } => {
            // Resolve spec and prd from positional args and/or named flags.
            // Named flags take precedence over positional args.
            let (resolved_prd, resolved_spec) = match (prd, spec, files.len()) {
                // Both named flags provided
                (Some(p), Some(s), _) => (Some(p), s),
                // Only --spec provided
                (None, Some(s), _) => (None, s),
                // Only --prd provided + one positional (the spec)
                (Some(p), None, 1) => (Some(p), files[0].clone()),
                // No flags, one positional = spec only
                (None, None, 1) => (None, files[0].clone()),
                // No flags, two positional = prd then spec
                (None, None, 2) => (Some(files[0].clone()), files[1].clone()),
                // --prd flag + no positional and no --spec
                (Some(_), None, 0) => {
                    anyhow::bail!("Spec file is required. Usage: pas generate [--prd PRD] <SPEC>");
                }
                // No args at all
                (None, None, 0) => {
                    anyhow::bail!("Spec file is required. Usage: pas generate [PRD] <SPEC>");
                }
                _ => {
                    anyhow::bail!(
                        "Too many arguments. Usage:\n  \
                         pas generate <SPEC>\n  \
                         pas generate <PRD> <SPEC>\n  \
                         pas generate --prd <PRD> --spec <SPEC>"
                    );
                }
            };
            cmd_generate(resolved_prd.as_deref(), &resolved_spec, output.as_deref(), cli.verbose).await?;
        }
    }

    Ok(())
}

pub(crate) fn load_pipeline(path: &std::path::Path) -> anyhow::Result<attractor_pipeline::PipelineGraph> {
    let source = std::fs::read_to_string(path)?;
    let dot = attractor_dot::parse(&source)?;
    let graph = attractor_pipeline::PipelineGraph::from_dot(dot)?;
    Ok(graph)
}
