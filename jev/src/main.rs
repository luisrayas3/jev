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
    /// Plan, compile, and run in one shot
    Go {
        /// The task description
        task: String,
    },
}

// -- Response parsing -------------------------------------------------------

fn parse_response(raw: &str) -> Result<String> {
    extract_fenced(raw, "rust")
        .context("response missing ```rust``` fenced block")
}

fn extract_fenced(text: &str, lang: &str) -> Option<String> {
    let opener = format!("```{lang}");
    let start = text.find(&opener)?;
    let after_opener = start + opener.len();
    let rest = &text[after_opener..];
    let end = rest.find("```")?;
    Some(rest[..end].trim().to_string())
}

// -- Paths and IDs ----------------------------------------------------------

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

fn plan_id(task: &str, catalog: &str, code: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    task.hash(&mut h);
    SYSTEM_PROMPT.hash(&mut h);
    catalog.hash(&mut h);
    code.hash(&mut h);
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

// -- LLM prompts ------------------------------------------------------------

const SYSTEM_PROMPT: &str = r#"You are a Rust code generator for the jev agent system.

Output ONE fenced ```rust``` block. No explanation.

Use #[jevs::needs(...)] to declare resources:

```rust
use jevs::{File, FileTree, Labeled};
use jevs::label::*;

#[jevs::needs(
    name: File<Classification, Integrity> = "path",
    name: FileTree<Classification, Integrity> = "path/",
)]
pub async fn root(
    needs: &mut Needs,
) -> anyhow::Result<()> { ... }
```

The macro generates the Needs struct
and create() function. Access fields as needs.name.

File types:
- File<C, I>: single file.
  read(): needs.f.read()
  write(content): needs.f.write(content)
- FileTree<C, I>: directory.
  read(path): needs.fs.read("file.txt")
  write(path, content): needs.fs.write("f.txt", c)
  glob(pattern): needs.fs.glob("*.txt")

Labels:
- Classification: Private (default), Public
- Integrity: Me (default), Friend, World
- use jevs::label::* to import

Data:
- read() returns Labeled<String, C, I>
- Use .inner(), .into_inner(), .map(|s| ...)
- write() takes Labeled<String, Ci, Ii>
  (labels must satisfy resource labels)
- Labeled::local(value) creates Public, Me data
- declassify: jevs::declassify!(expr).await?
- accredit: jevs::accredit!(expr, Tier).await?

Rules:
- read()/glob() take &self (shared access)
- write() takes &mut self (exclusive access)
- Use tokio::join! for parallel reads
- Never mix & and &mut in same join
- Print results to stdout
- For temp storage: jevs::stash::Stash::new()?
"#;

// -- API types and LLM calls ------------------------------------------------

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

// -- Plan orchestration -----------------------------------------------------

const MAX_RETRIES: usize = 3;

async fn plan_and_compile(task: &str) -> Result<PathBuf> {
    let catalog = jevs::api::catalog();

    let api_key =
        std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
    let client = reqwest::Client::new();

    let user_prompt = format!(
        "Task: {task}\n\nAvailable API:\n{catalog}\n\n\
         Generate tasks.rs with #[jevs::needs(...)]."
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
        let raw = call_llm_raw(&client, &api_key, &messages).await?;
        let tasks_code = parse_response(&raw)?;

        let id = plan_id(task, &catalog, &tasks_code);
        let plan_dir = plans_dir().join(&id);

        if binary_path(&plan_dir).exists() {
            eprintln!("  reusing existing plan {id}");
            return Ok(plan_dir);
        }

        messages.push(Message {
            role: "assistant".to_string(),
            content: raw,
        });
        log_exchange(&id, &messages, &label)?;

        let plan_dir = write_plan(&id, &tasks_code)?;
        match try_build(&plan_dir) {
            Ok(_) => {
                eprintln!("  compiled ok");
                return Ok(plan_dir);
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
                         Fix ALL errors and output the complete \
                         corrected ```rust``` block. \
                         No explanation.\n\n\
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

    let cargo_toml = include_str!("../assets/Cargo.tmpl.toml")
        .replace("{plan_name}", &format!("plan{id}"))
        .replace("{jevs_path}", &root.join("jevs").display().to_string());
    std::fs::write(plan_dir.join("Cargo.toml"), cargo_toml)?;

    std::fs::write(
        src_dir.join("main.rs"),
        include_str!("../assets/main.tmpl.rs"),
    )?;

    std::fs::write(src_dir.join("tasks.rs"), tasks_code)?;

    Ok(plan_dir)
}

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

fn tasks_code(plan_dir: &Path) -> Result<String> {
    std::fs::read_to_string(plan_dir.join("src/tasks.rs"))
        .context("reading tasks.rs")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Plan { task } => {
            eprintln!("Planning: {task}");
            let plan_dir = plan_and_compile(&task).await?;
            let id = plan_dir.file_name().unwrap().to_str().unwrap();
            eprintln!("\nPlan {id} compiled.");
            eprintln!("Run with: jev run {id}");
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
            let plan_dir = plan_and_compile(&task).await?;
            let binary = binary_path(&plan_dir);

            loop {
                eprint!("\nRun? [y/N/t=show tasks] ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).ok();
                match input.trim() {
                    "y" | "Y" | "yes" => {
                        run_binary(&binary)?;
                        break;
                    }
                    "t" | "T" => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_fenced_basic() {
        let text = "before\n```rust\nfn main() {}\n```\nafter";
        let result = extract_fenced(text, "rust").unwrap();
        assert_eq!(result, "fn main() {}");
    }

    #[test]
    fn parse_response_missing_rust() {
        let raw = "no code here";
        let err = parse_response(raw).unwrap_err();
        assert!(
            format!("{err}").contains("rust"),
            "error should mention rust: {err}"
        );
    }

    #[test]
    fn parse_response_single_block() {
        let raw = r#"```rust
use jevs::{File, Labeled};
use jevs::label::*;

#[jevs::needs(
    f: File<Private, Me> = "./data",
)]
pub async fn root(
    needs: &mut Needs,
) -> anyhow::Result<()> {
    Ok(())
}
```
"#;
        let code = parse_response(raw).unwrap();
        assert!(code.contains("#[jevs::needs("));
        assert!(code.contains("pub async fn root"));
    }

    #[test]
    fn plan_id_includes_code() {
        let id1 = plan_id("task", "catalog", "code-a");
        let id2 = plan_id("task", "catalog", "code-b");
        assert_ne!(id1, id2);
    }
}
