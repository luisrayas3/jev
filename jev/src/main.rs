use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// Plan, confirm, build, and run in one shot
    Go {
        /// The task description
        task: String,
    },
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().to_path_buf()
}

fn plans_dir() -> PathBuf {
    workspace_root().join("plans")
}

fn logs_dir() -> PathBuf {
    workspace_root().join("logs")
}

fn plan_id(task: &str, catalog: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    task.hash(&mut h);
    SYSTEM_PROMPT.hash(&mut h);
    catalog.hash(&mut h);
    format!("{:x}", h.finish())
}

fn latest_plan() -> Result<String> {
    let dir = plans_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .context("no plans directory")?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    entries
        .last()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .context("no plans found")
}

const RESOURCES_DOCS: &str = r#"## Resources struct

Your code receives a `&mut Resources` with these fields:
```rust
pub struct Resources {
    pub fs: jevs::file::File,  // filesystem rooted at "."
}
```
Access resources through `res.fs`, not by constructing them.
"#;

const SYSTEM_PROMPT: &str = r#"You are a Rust code generator for the jev agent system.

Wrap your output in a ```rust``` fenced code block.
Output ONLY the fenced code block: no explanation, no commentary.

Rules:
- Start with `use crate::resources::Resources;` and any needed qualified imports (e.g. `use jevs::text::line_count;`).
- Do NOT use `use jevs::*;` - use qualified paths like `jevs::file::File`, `jevs::text::line_count`, `jevs::trust::Unverified`.
- Implement `pub async fn root(res: &mut Resources) -> anyhow::Result<()>`
- Access the filesystem through `res.fs` (it's a `jevs::file::File`).
- Do NOT construct File, use RuntimeKey, or reference jevsr. Resources are pre-constructed.
- `res.fs.read()` and `res.fs.glob()` take `&self` (shared read access).
- `res.fs.write()` takes `&mut self` (exclusive write access).
- Use `tokio::join!` for parallel reads.
- Never combine `&` and `&mut` access in the same join; it won't compile.
- Print results to stdout so the user can see them.
"#;

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

fn log_exchange(id: &str, messages: &[Message], label: &str) -> Result<()> {
    let dir = logs_dir().join(id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{label}.json"));
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "system": SYSTEM_PROMPT,
        "messages": messages,
    }))?;
    std::fs::write(&path, &json)?;
    eprintln!("  log: {}", path.display());
    Ok(())
}

async fn call_llm_raw(
    client: &reqwest::Client,
    api_key: &str,
    messages: &[Message],
) -> Result<String> {
    let request = ApiRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 4096,
        system: SYSTEM_PROMPT.to_string(),
        messages: messages.to_vec(),
    };

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .context("API request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("API error {status}: {body}");
    }

    let api_resp: ApiResponse =
        resp.json().await.context("parsing API response")?;
    let code = api_resp
        .content
        .into_iter()
        .filter_map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");

    Ok(code)
}

/// Check that tasks.rs doesn't reference jevsr, RuntimeKey, or File::open.
fn check_boundary(code: &str) -> Result<()> {
    let violations: Vec<&str> = ["jevsr", "RuntimeKey", "File::open"]
        .into_iter()
        .filter(|term| code.contains(term))
        .collect();

    if !violations.is_empty() {
        bail!(
            "Boundary violation: tasks.rs must not reference {}. \
             Resources are provided via the Resources struct.",
            violations.join(", ")
        );
    }
    Ok(())
}

fn strip_fences(code: &str) -> Result<String> {
    let s = code.trim();
    let s = s.strip_prefix("```rust")
        .or_else(|| s.strip_prefix("```"))
        .context("response missing opening ```rust fence")?;
    let s = s.strip_suffix("```")
        .context("response missing closing ``` fence")?;
    Ok(s.trim().to_string())
}

const MAX_RETRIES: usize = 3;

/// Generate code, write it, compile it.
/// On compile failure, feed errors back to the LLM and retry.
/// Reuses existing plan if one exists for the same inputs.
/// Returns (plan_dir, final_code).
async fn plan_and_compile(task: &str) -> Result<(PathBuf, String)> {
    let catalog = jevs::api::catalog();
    let full_docs = format!("{catalog}\n{RESOURCES_DOCS}");
    let id = plan_id(task, &full_docs);
    let plan_dir = plans_dir().join(&id);

    // Reuse existing compiled plan
    if binary_path(&plan_dir).exists() {
        let code = std::fs::read_to_string(
            plan_dir.join("src/tasks.rs"),
        )?;
        eprintln!("  reusing existing plan {id}");
        return Ok((plan_dir, code));
    }

    let api_key =
        std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
    let client = reqwest::Client::new();

    let user_prompt = format!(
        "Task: {task}\n\nAvailable API:\n{full_docs}\n\n\
         Generate tasks.rs (just the module body, starting with use statements)."
    );

    let mut messages = vec![Message {
        role: "user".to_string(),
        content: user_prompt,
    }];

    for attempt in 0..=MAX_RETRIES {
        let label = if attempt == 0 {
            "initial".to_string()
        } else {
            format!("retry-{attempt}")
        };

        eprintln!("  attempt {}/{}", attempt + 1, MAX_RETRIES + 1);
        let code = call_llm_raw(&client, &api_key, &messages).await?;
        let code = strip_fences(&code)?;

        // Log the conversation so far + response
        messages.push(Message {
            role: "assistant".to_string(),
            content: code.clone(),
        });
        log_exchange(&id, &messages, &label)?;

        // Check boundary before writing
        if let Err(e) = check_boundary(&code) {
            if attempt == MAX_RETRIES {
                bail!(
                    "Boundary violation after {} attempts: {e}",
                    MAX_RETRIES + 1
                );
            }
            eprintln!("  boundary violation, retrying...");
            messages.push(Message {
                role: "user".to_string(),
                content: format!(
                    "That code violates the compilation boundary: {e}\n\n\
                     Do NOT construct File, use RuntimeKey, or reference jevsr.\n\
                     Access resources through `res.fs`.\n\n\
                     Fix ALL issues and output the complete corrected tasks.rs \
                     in a ```rust``` fenced code block. No explanation."
                ),
            });
            continue;
        }

        // Write and try to compile
        let plan_dir = write_plan(&id, &code)?;
        match try_build(&plan_dir) {
            Ok(_) => {
                eprintln!("  compiled ok");
                return Ok((plan_dir, code));
            }
            Err(stderr) => {
                if attempt == MAX_RETRIES {
                    bail!(
                        "Failed to compile after {} attempts.\n\
                         Last error:\n{stderr}",
                        MAX_RETRIES + 1
                    );
                }
                eprintln!("  compile error, retrying...");
                messages.push(Message {
                    role: "user".to_string(),
                    content: format!(
                        "That code failed to compile. \
                         Fix ALL errors and output the complete corrected tasks.rs \
                         in a ```rust``` fenced code block. No explanation.\n\n\
                         Compiler errors:\n{stderr}"
                    ),
                });
            }
        }
    }

    unreachable!()
}

fn write_plan(id: &str, tasks_code: &str) -> Result<PathBuf> {
    let root = workspace_root();
    let plan_dir = plans_dir().join(id);
    let src_dir = plan_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let cargo_toml = format!(
        r#"[package]
name = "plan{id}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
jevs = {{ path = "{jevs_path}" }}
jevsr = {{ path = "{jevsr_path}" }}
tokio = {{ version = "1", features = ["full"] }}
anyhow = "1"
"#,
        jevs_path = root.join("jevs").display(),
        jevsr_path = root.join("jevsr").display(),
    );

    std::fs::write(plan_dir.join("Cargo.toml"), cargo_toml)?;

    // Write main.rs from embedded asset
    std::fs::write(
        src_dir.join("main.rs"),
        include_str!("../assets/plan_main.rs"),
    )?;

    // Generate resources.rs
    let resources_code = r#"pub struct Resources {
    pub fs: jevs::file::File,
}

pub fn create() -> Resources {
    Resources {
        fs: jevsr::open_file("."),
    }
}
"#;
    std::fs::write(src_dir.join("resources.rs"), resources_code)?;

    // Write LLM-generated tasks.rs
    std::fs::write(src_dir.join("tasks.rs"), tasks_code)?;

    Ok(plan_dir)
}

/// Try to compile. Returns Ok(()) on success, Err(stderr) on failure.
fn try_build(plan_dir: &Path) -> std::result::Result<(), String> {
    let output = std::process::Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(plan_dir)
        .output()
        .expect("failed to invoke cargo");

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn binary_path(plan_dir: &Path) -> PathBuf {
    let bin_name = plan_dir.file_name().unwrap().to_str().unwrap();
    plan_dir
        .join("target")
        .join("release")
        .join(format!("plan{bin_name}"))
}

fn run_binary(binary: &Path) -> Result<()> {
    eprintln!("Running...");
    let status = std::process::Command::new(binary)
        .status()
        .context("failed to execute plan binary")?;

    if !status.success() {
        bail!("Plan exited with {status}");
    }
    Ok(())
}

fn build_plan(plan_dir: &Path) -> Result<PathBuf> {
    eprintln!("Building plan...");
    try_build(plan_dir).map_err(|e| anyhow::anyhow!("Compilation failed:\n{e}"))?;
    Ok(binary_path(plan_dir))
}

fn resources_code(plan_dir: &Path) -> Result<String> {
    std::fs::read_to_string(plan_dir.join("src/resources.rs"))
        .context("reading resources.rs")
}

fn tasks_code(plan_dir: &Path) -> Result<String> {
    std::fs::read_to_string(plan_dir.join("src/tasks.rs"))
        .context("reading tasks.rs")
}

fn show_resources(plan_dir: &Path) -> Result<()> {
    let code = resources_code(plan_dir)?;
    eprintln!("--- resources.rs ---");
    eprint!("{code}");
    eprintln!("--- end ---");
    Ok(())
}

/// Prompt for action. Returns 'y' (approve), 't' (show tasks), or 'n' (abort).
fn prompt_action() -> char {
    eprint!("Approve? [y/N/t=show tasks] ");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok();
    match input.trim() {
        "y" | "Y" | "yes" => 'y',
        "t" | "T" => 't',
        _ => 'n',
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Plan { task } => {
            eprintln!("Planning: {task}");
            let (plan_dir, _code) = plan_and_compile(&task).await?;
            let id = plan_dir.file_name().unwrap().to_str().unwrap();
            eprintln!("\nPlan {id}");
            show_resources(&plan_dir)?;
            eprintln!("\nRun with: jev run {id}");
        }
        Command::Run { id } => {
            let id = id.map_or_else(|| latest_plan(), Ok)?;
            let plan_dir = plans_dir().join(&id);
            if !plan_dir.exists() {
                bail!("Plan {id} not found");
            }
            let binary = build_plan(&plan_dir)?;
            run_binary(&binary)?;
        }
        Command::Go { task } => {
            eprintln!("Planning: {task}");
            let (plan_dir, _code) = plan_and_compile(&task).await?;
            eprintln!();
            show_resources(&plan_dir)?;

            loop {
                match prompt_action() {
                    'y' => {
                        run_binary(&binary_path(&plan_dir))?;
                        break;
                    }
                    't' => {
                        let code = tasks_code(&plan_dir)?;
                        eprintln!("--- tasks.rs ---");
                        eprint!("{code}");
                        eprintln!("--- end ---");
                    }
                    _ => {
                        eprintln!("Aborted.");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
