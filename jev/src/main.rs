mod exec;
mod llm;
mod plan;
mod prompt;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

/// jev - agent orchestration via Rust code generation
#[derive(Parser)]
#[command(name = "jev")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a plan: LLM produces Rust source for the given task
    Plan {
        /// The task description
        task: String,
    },
    /// Build and run the most recent plan (or a specific plan ID)
    Run {
        /// Plan ID to run (defaults to most recent)
        id: Option<String>,
    },
    /// Plan, compile, and run in one shot
    Go {
        /// The task description
        task: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Plan { task } => {
            eprintln!("Planning: {task}");
            let plan_dir = plan::plan_and_compile(&task).await?;
            let id = plan_dir
                .file_name()
                .unwrap()
                .to_str()
                .unwrap();
            eprintln!("\nPlan {id} compiled.");
            eprintln!("Run with: jev run {id}");
        }
        Command::Run { id } => {
            let id =
                id.map_or_else(|| plan::latest_plan(), Ok)?;
            let plan_dir = plan::plans_dir().join(&id);
            if !plan_dir.exists() {
                bail!("Plan {id} not found");
            }
            let binary = exec::build_plan(&plan_dir)?;
            exec::run_binary(&binary)?;
        }
        Command::Go { task } => {
            eprintln!("Planning: {task}");
            let plan_dir = plan::plan_and_compile(&task).await?;
            exec::run_binary(&exec::binary_path(&plan_dir))?;
        }
    }

    Ok(())
}
