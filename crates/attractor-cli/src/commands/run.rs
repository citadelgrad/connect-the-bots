use std::path::PathBuf;

use anyhow;

/// Generate a unique logs directory name from the pipeline filename and a short random suffix.
/// Format: `.pas/logs/<stem>-<8hex>` e.g. `.pas/logs/attractor-e0n-a3f1b2c9`
fn unique_logs_dir(pipeline_path: &std::path::Path) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let stem = pipeline_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    // Hash the current time + PID for uniqueness across concurrent runs
    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let hash = hasher.finish();

    PathBuf::from(format!(".pas/logs/{}-{:08x}", stem, hash as u32))
}

pub async fn cmd_run(
    path: &std::path::Path,
    workdir: Option<&std::path::Path>,
    logs: Option<&std::path::Path>,
    dry_run: bool,
    max_budget_usd: Option<f64>,
    max_steps: u64,
) -> anyhow::Result<()> {
    let graph = crate::load_pipeline(path)?;

    // Resolve logs directory: explicit flag or auto-generated unique path
    let logs_dir = match logs {
        Some(l) => l.to_path_buf(),
        None => unique_logs_dir(path),
    };

    println!("Running pipeline: {}", graph.name);
    if !graph.goal.is_empty() {
        println!("Goal: {}", graph.goal);
    }
    println!("Logs: {}", logs_dir.display());
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
