use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// jev — agent orchestration via Rust code generation
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

fn plan_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{:x}", ts)
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

const API_CATALOG: &str = r#"
# jevstd API

## Filesystem — `jevstd::Fs`

```rust
// Open a filesystem rooted at a directory
let fs = Fs::open("/some/path");

// Read a file (shared ref — can parallelize reads)
let content: String = fs.read("file.txt").await?;

// Glob for files (shared ref)
let files: Vec<String> = fs.glob("*.rs").await?;

// Write a file (exclusive ref — no concurrent access)
fs.write("out.txt", "content").await?;
```

Key: `&Fs` = read, `&mut Fs` = write.
Multiple reads can run in parallel via `tokio::join!`.
A write requires exclusive access — no concurrent reads or writes.

## Text — pure functions

```rust
let n = jevstd::line_count("hello\nworld");  // 2
let s = jevstd::concat(&["a", "b", "c"]);    // "abc"
```

## Trust types

```rust
let raw = Unverified(some_value);       // untrusted data
let checked = raw.verify();              // -> Verified<T>
checked.inner()                          // &T
checked.into_inner()                     // T
```

Functions that require trust take `Verified<T>`.
Passing `Unverified<T>` is a compile error.
"#;

const SYSTEM_PROMPT: &str = r#"You are a Rust code generator for the jev agent system.

You receive a task description and produce a complete Rust `main.rs` that accomplishes the task using the jevstd library.

Rules:
- Output ONLY raw Rust source code. No markdown fences. No explanation. No commentary.
- The program must be a complete `main.rs` with `use jevstd::*;`
- Use `#[tokio::main]` for async main.
- Use `anyhow::Result` for error handling.
- `Fs::open(path)` returns `Fs` directly (not Result). Do NOT use `?` on it.
- `fs.read()` and `fs.glob()` take `&self` (shared read access).
- `fs.write()` takes `&mut self` (exclusive write access).
- Use `tokio::join!` for parallel reads.
- Never combine `&` and `&mut` access in the same join — it won't compile.
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

const MAX_RETRIES: usize = 3;

/// Generate code, write it, compile it.
/// On compile failure, feed errors back to the LLM and retry.
/// Returns (plan_dir, final_code).
async fn plan_and_compile(task: &str) -> Result<(PathBuf, String)> {
    let api_key =
        std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
    let client = reqwest::Client::new();
    let id = plan_id();

    let user_prompt = format!(
        "Task: {task}\n\nAvailable API:\n{API_CATALOG}\n\nGenerate the Rust main.rs."
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

        // Log the conversation so far + response
        messages.push(Message {
            role: "assistant".to_string(),
            content: code.clone(),
        });
        log_exchange(&id, &messages, &label)?;

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
                // Feed error back as user message
                messages.push(Message {
                    role: "user".to_string(),
                    content: format!(
                        "That code failed to compile. \
                         Fix ALL errors and output the complete corrected main.rs. \
                         No markdown fences, no explanation.\n\n\
                         Compiler errors:\n{stderr}"
                    ),
                });
            }
        }
    }

    unreachable!()
}

fn write_plan(id: &str, code: &str) -> Result<PathBuf> {
    let root = workspace_root();
    let plan_dir = plans_dir().join(id);
    let src_dir = plan_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let cargo_toml = format!(
        r#"[package]
name = "plan-{id}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
jevstd = {{ path = "{jevstd_path}" }}
tokio = {{ version = "1", features = ["full"] }}
anyhow = "1"
"#,
        jevstd_path = root.join("jevstd").display(),
    );

    std::fs::write(plan_dir.join("Cargo.toml"), cargo_toml)?;
    std::fs::write(src_dir.join("main.rs"), code)?;

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
        .join(format!("plan-{bin_name}"))
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

fn confirm(prompt: &str) -> bool {
    eprint!("{prompt} [y/N] ");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok();
    matches!(input.trim(), "y" | "Y" | "yes")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Plan { task } => {
            eprintln!("Planning: {task}");
            let (plan_dir, code) = plan_and_compile(&task).await?;
            let id = plan_dir.file_name().unwrap().to_str().unwrap();
            eprintln!("\nPlan {id} written to {}", plan_dir.display());
            eprintln!("--- generated code ---");
            println!("{code}");
            eprintln!("--- end ---");
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
            let (plan_dir, code) = plan_and_compile(&task).await?;
            eprintln!("\n--- generated code ---");
            println!("{code}");
            eprintln!("--- end ---\n");

            if !confirm("Build and run?") {
                eprintln!("Aborted.");
                return Ok(());
            }

            run_binary(&binary_path(&plan_dir))?;
        }
    }

    Ok(())
}
