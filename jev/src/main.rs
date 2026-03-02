use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
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

// -- Resource declarations --------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Access {
    Read,
    Write,
    ReadWrite,
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Access::Read => write!(f, "read"),
            Access::Write => write!(f, "write"),
            Access::ReadWrite => write!(f, "readwrite"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ClassificationLevel {
    Public,
    Private,
}

impl fmt::Display for ClassificationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClassificationLevel::Public => write!(f, "public"),
            ClassificationLevel::Private => write!(f, "private"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum IntegrityLevel {
    Me,
    Friend,
    World,
}

impl fmt::Display for IntegrityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntegrityLevel::Me => write!(f, "me"),
            IntegrityLevel::Friend => write!(f, "friend"),
            IntegrityLevel::World => write!(f, "world"),
        }
    }
}

fn parse_classification(s: &str) -> Result<ClassificationLevel> {
    match s {
        "public" => Ok(ClassificationLevel::Public),
        "private" => Ok(ClassificationLevel::Private),
        other => bail!(
            "unknown confidentiality \"{other}\"; \
             expected public or private"
        ),
    }
}

fn parse_integrity(s: &str) -> Result<IntegrityLevel> {
    match s {
        "me" => Ok(IntegrityLevel::Me),
        "friend" => Ok(IntegrityLevel::Friend),
        "world" => Ok(IntegrityLevel::World),
        other => bail!(
            "unknown integrity \"{other}\"; \
             expected me, friend, or world"
        ),
    }
}

#[derive(Debug, Clone)]
struct ResourceDecl {
    name: String,
    url: String,
    access: Access,
    classification: ClassificationLevel,
    integrity: IntegrityLevel,
}

#[derive(Deserialize)]
struct ResourceToml {
    resources: HashMap<String, ResourceEntry>,
}

#[derive(Deserialize)]
struct ResourceEntry {
    url: String,
    access: String,
    confidentiality: Option<String>,
    integrity: Option<String>,
}

fn parse_access(s: &str) -> Result<Access> {
    match s {
        "read" => Ok(Access::Read),
        "write" => Ok(Access::Write),
        "readwrite" => Ok(Access::ReadWrite),
        other => bail!("unknown access mode \"{other}\"; expected read, write, or readwrite"),
    }
}

/// Parse a resource URL into (scheme, path, is_dir).
/// Trailing `/` distinguishes directories from files:
/// `file:/tmp/foo` = single file, `file:./` = directory.
fn parse_url(url: &str) -> Result<(&str, &str, bool)> {
    let (scheme, rest) = url
        .split_once(':')
        .context("resource URL must contain ':' (e.g. file:./)")?;
    match scheme {
        "file" => {
            let is_dir = rest.ends_with('/');
            Ok((scheme, rest, is_dir))
        }
        other => bail!(
            "unsupported URL scheme \"{other}:\"; only file: is supported for now"
        ),
    }
}

fn parse_resource_decls(toml_str: &str) -> Result<Vec<ResourceDecl>> {
    let parsed: ResourceToml =
        toml::from_str(toml_str).context("parsing resource declarations")?;
    let mut decls: Vec<ResourceDecl> = parsed
        .resources
        .into_iter()
        .map(|(name, entry)| {
            let (_, _, _) = parse_url(&entry.url)?;
            let access = parse_access(&entry.access)?;
            let classification = entry
                .confidentiality
                .as_deref()
                .map(parse_classification)
                .transpose()?
                .unwrap_or(ClassificationLevel::Private);
            let integrity = entry
                .integrity
                .as_deref()
                .map(parse_integrity)
                .transpose()?
                .unwrap_or(IntegrityLevel::Me);
            Ok(ResourceDecl {
                name,
                url: entry.url,
                access,
                classification,
                integrity,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    decls.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(decls)
}

fn label_types(
    classification: &ClassificationLevel,
    integrity: &IntegrityLevel,
) -> String {
    let c = match classification {
        ClassificationLevel::Public => "jevs::label::Public",
        ClassificationLevel::Private => "jevs::label::Private",
    };
    let i = match integrity {
        IntegrityLevel::Me => "jevs::label::Me",
        IntegrityLevel::Friend => "jevs::label::Friend",
        IntegrityLevel::World => "jevs::label::World",
    };
    format!("<{c}, {i}>")
}

fn generate_resources_rs(decls: &[ResourceDecl]) -> String {
    let mut fields = String::new();
    let mut inits = String::new();
    for decl in decls {
        let (_, path, is_dir) = parse_url(&decl.url).expect("already validated");
        let labels = label_types(&decl.classification, &decl.integrity);
        if is_dir {
            let path = path.trim_end_matches('/');
            fields.push_str(&format!(
                "    pub {}: jevs::file::FileTree{labels},\n",
                decl.name,
            ));
            inits.push_str(&format!(
                "        {}: jevs::file::FileTree::open(key, \"{path}\"),\n",
                decl.name,
            ));
        } else {
            fields.push_str(&format!(
                "    pub {}: jevs::file::File{labels},\n",
                decl.name,
            ));
            inits.push_str(&format!(
                "        {}: jevs::file::File::open(key, \"{path}\"),\n",
                decl.name,
            ));
        }
    }
    format!(
        "pub struct Resources {{\n\
         {fields}\
         }}\n\
         \n\
         pub fn create(key: &jevs::runtime::RuntimeKey) -> Resources {{\n\
         \x20   Resources {{\n\
         {inits}\
         \x20   }}\n\
         }}\n"
    )
}

// -- Response parsing -------------------------------------------------------

/// Extract ```rust``` and ```toml``` fenced blocks from LLM response.
fn parse_response(raw: &str) -> Result<(String, String)> {
    let rust = extract_fenced(raw, "rust")
        .context("response missing ```rust``` fenced block")?;
    let toml = extract_fenced(raw, "toml")
        .context("response missing ```toml``` fenced block")?;
    Ok((rust, toml))
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

fn plan_id(task: &str, catalog: &str, resource_toml: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    task.hash(&mut h);
    SYSTEM_PROMPT.hash(&mut h);
    catalog.hash(&mut h);
    resource_toml.hash(&mut h);
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

const RESOURCES_DOCS: &str = r#"## Resource declarations

After the ```rust``` block, output a ```toml``` block declaring the resources your code needs.

Format:
```toml
[resources.<name>]
url = "<scheme>:<path>"
access = "read" | "write" | "readwrite"
confidentiality = "public" | "private"  # default: private
integrity = "me" | "friend" | "world"   # default: me
```

- `<name>` becomes a field on the Resources struct: `res.<name>` in your code.
- URL scheme determines the resource kind. Only `file:` is supported for now.
- `access` controls permissions: `read`, `write`, or `readwrite`.
- `confidentiality` and `integrity` are labels. Defaults: `private` and `me` (user's own files).

**Trailing `/` convention:**
- `file:/tmp/foo` = single file → `jevs::file::File` (no path arg: `res.f.read()`, `res.f.write(content)`)
- `file:./` or `file:/data/` = directory → `jevs::file::FileTree` (path arg: `res.fs.read("file.txt")`, `res.fs.write("file.txt", content)`)

Example:
```toml
[resources.fs]
url = "file:./"
access = "readwrite"

[resources.config]
url = "file:./config.toml"
access = "read"
```

For temporary storage, create a stash directly (no declaration needed):
`let stash = jevs::stash::Stash::new()?;`
"#;

const SYSTEM_PROMPT: &str = r#"You are a Rust code generator for the jev agent system.

Output TWO fenced blocks, in this order:
1. ```rust``` — tasks.rs code
2. ```toml``` — resource declarations

Output ONLY these two blocks: no explanation, no commentary.

Rules for the ```rust``` block:
- Start with `use crate::resources::Resources;` and any needed qualified imports (e.g. `use jevs::label::Labeled;`).
- Do NOT use `use jevs::*;` - use qualified paths like `jevs::file::File`, `jevs::label::Labeled`.
- Implement `pub async fn root(res: &mut Resources) -> anyhow::Result<()>`
- Access resources through fields on `res` (e.g. `res.fs`). The field names match the resource names you declare in TOML.
- For temporary storage, create a stash: `let stash = jevs::stash::Stash::new()?;`
- Do NOT construct resources. They are provided via the Resources struct.
- **File** (single file, no trailing `/` in URL): `res.<name>.read()` and `res.<name>.write(content)` — no path parameter.
- **FileTree** (directory, trailing `/` in URL): `res.<name>.read(path)`, `res.<name>.write(path, content)`, `res.<name>.glob(pattern)` — path parameter required.
- `read()` returns `Labeled<String>`. Use `.inner()` for `&String`, `.into_inner()` for owned `String`, or `.map(|s| ...)` to transform.
- `write()` takes `Labeled<String>`. Labels must be compatible with the resource (same or less restrictive).
- `glob()` returns `Vec<String>` (unlabeled paths).
- Create local data with `jevs::label::Labeled::local("text".to_string())`.
- Cross label boundaries: `.declassify().await?` (Private→Public), `.accredit::<Tier>().await?` (increase integrity).
- `read()` and `glob()` take `&self` (shared read access).
- `write()` takes `&mut self` (exclusive write access).
- Use `tokio::join!` for parallel reads.
- Never combine `&` and `&mut` access in the same join; it won't compile.
- Print results to stdout. Use `.inner()` or `.into_inner()` to extract values for printing.

Rules for the ```toml``` block:
- Declare each resource under `[resources.<name>]` with `url` and `access`.
- `<name>` must match the field name you use as `res.<name>` in code.
- Only `file:` URL scheme is supported.
- Trailing `/` distinguishes type: `file:./` or `file:/data/` = directory (FileTree), `file:./config.toml` or `file:/tmp/foo` = single file (File).
- Access: `read`, `write`, or `readwrite`.
- Labels (optional): `confidentiality` = `public` or `private` (default: `private`), `integrity` = `me`, `friend`, or `world` (default: `me`).
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

/// Generate code, write it, compile it.
/// On compile failure, feed errors back to the LLM and retry.
/// Reuses existing plan if one exists for the same inputs.
/// Returns (plan_dir, decls, tasks_code).
async fn plan_and_compile(
    task: &str,
) -> Result<(PathBuf, Vec<ResourceDecl>, String)> {
    let catalog = jevs::api::catalog();
    let full_docs = format!("{catalog}\n{RESOURCES_DOCS}");

    let api_key =
        std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
    let client = reqwest::Client::new();

    let user_prompt = format!(
        "Task: {task}\n\nAvailable API:\n{full_docs}\n\n\
         Generate tasks.rs and resource declarations."
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
        let (tasks_code, resource_toml) = parse_response(&raw)?;
        let decls = parse_resource_decls(&resource_toml)?;

        let id = plan_id(task, &full_docs, &resource_toml);
        let plan_dir = plans_dir().join(&id);

        // Reuse existing compiled plan
        if binary_path(&plan_dir).exists() {
            eprintln!("  reusing existing plan {id}");
            return Ok((plan_dir, decls, tasks_code));
        }

        // Log the conversation so far + response
        messages.push(Message {
            role: "assistant".to_string(),
            content: raw,
        });
        log_exchange(&id, &messages, &label)?;

        // Write and try to compile
        let plan_dir = write_plan(&id, &decls, &tasks_code)?;
        match try_build(&plan_dir) {
            Ok(_) => {
                eprintln!("  compiled ok");
                return Ok((plan_dir, decls, tasks_code));
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
                         Fix ALL errors and output both blocks: \
                         the complete corrected ```rust``` tasks.rs \
                         and the ```toml``` resource declarations. \
                         No explanation.\n\n\
                         Compiler errors:\n{stderr}"
                    ),
                });
            }
        }
    }

    unreachable!()
}

fn write_plan(
    id: &str,
    decls: &[ResourceDecl],
    tasks_code: &str,
) -> Result<PathBuf> {
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

    std::fs::write(
        src_dir.join("resources.rs"),
        generate_resources_rs(decls),
    )?;

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

fn tasks_code(plan_dir: &Path) -> Result<String> {
    std::fs::read_to_string(plan_dir.join("src/tasks.rs"))
        .context("reading tasks.rs")
}

fn show_manifest(decls: &[ResourceDecl]) {
    eprintln!("This plan requires:\n");
    for decl in decls {
        eprintln!(
            "  {:<8} {:<16} {:<10} {} {}",
            decl.name, decl.url, decl.access, decl.classification, decl.integrity,
        );
    }
}

/// Prompt for action. Returns 'y' (approve), 't' (show tasks), or 'n' (abort).
fn prompt_action() -> char {
    eprint!("\nApprove? [y/N/t=show tasks] ");
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
            let (plan_dir, decls, _code) =
                plan_and_compile(&task).await?;
            let id = plan_dir.file_name().unwrap().to_str().unwrap();
            eprintln!("\nPlan {id}");
            show_manifest(&decls);
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
            let (plan_dir, decls, _code) =
                plan_and_compile(&task).await?;
            eprintln!();
            show_manifest(&decls);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_response_both_blocks() {
        let raw = r#"Here is code:

```rust
use crate::resources::Resources;

pub async fn root(res: &mut Resources) -> anyhow::Result<()> {
    Ok(())
}
```

```toml
[resources.fs]
url = "file:./"
access = "readwrite"
```
"#;
        let (rust, toml) = parse_response(raw).unwrap();
        assert!(rust.contains("pub async fn root"));
        assert!(toml.contains("[resources.fs]"));
    }

    #[test]
    fn parse_response_missing_toml() {
        let raw = "```rust\nfn main() {}\n```\n";
        let err = parse_response(raw).unwrap_err();
        assert!(
            format!("{err}").contains("toml"),
            "error should mention toml: {err}"
        );
    }

    #[test]
    fn parse_response_missing_rust() {
        let raw = "```toml\n[resources.fs]\nurl = \"file:.\"\naccess = \"read\"\n```\n";
        let err = parse_response(raw).unwrap_err();
        assert!(
            format!("{err}").contains("rust"),
            "error should mention rust: {err}"
        );
    }

    #[test]
    fn parse_decls_single() {
        let toml = r#"
[resources.fs]
url = "file:./"
access = "readwrite"
"#;
        let decls = parse_resource_decls(toml).unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "fs");
        assert_eq!(decls[0].url, "file:./");
        assert_eq!(decls[0].access, Access::ReadWrite);
        assert_eq!(decls[0].classification, ClassificationLevel::Private);
        assert_eq!(decls[0].integrity, IntegrityLevel::Me);
    }

    #[test]
    fn parse_decls_multiple_sorted() {
        let toml = r#"
[resources.data]
url = "file:/data/"
access = "read"

[resources.fs]
url = "file:./"
access = "readwrite"
"#;
        let decls = parse_resource_decls(toml).unwrap();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].name, "data");
        assert_eq!(decls[1].name, "fs");
    }

    #[test]
    fn parse_decls_bad_scheme() {
        let toml = r#"
[resources.web]
url = "https://example.com"
access = "read"
"#;
        let err = parse_resource_decls(toml).unwrap_err();
        assert!(
            format!("{err}").contains("unsupported URL scheme"),
            "error should mention scheme: {err}"
        );
    }

    #[test]
    fn parse_decls_bad_access() {
        let toml = r#"
[resources.fs]
url = "file:."
access = "execute"
"#;
        let err = parse_resource_decls(toml).unwrap_err();
        assert!(
            format!("{err}").contains("unknown access mode"),
            "error should mention access: {err}"
        );
    }

    #[test]
    fn generate_resources_dir() {
        let decls = vec![ResourceDecl {
            name: "fs".to_string(),
            url: "file:./".to_string(),
            access: Access::ReadWrite,
            classification: ClassificationLevel::Private,
            integrity: IntegrityLevel::Me,
        }];
        let rs = generate_resources_rs(&decls);
        assert!(rs.contains(
            "pub fs: jevs::file::FileTree<jevs::label::Private, jevs::label::Me>,"
        ));
        assert!(rs.contains("fs: jevs::file::FileTree::open(key, \".\"),"));
    }

    #[test]
    fn generate_resources_file() {
        let decls = vec![ResourceDecl {
            name: "config".to_string(),
            url: "file:./config.toml".to_string(),
            access: Access::Read,
            classification: ClassificationLevel::Private,
            integrity: IntegrityLevel::Me,
        }];
        let rs = generate_resources_rs(&decls);
        assert!(rs.contains(
            "pub config: jevs::file::File<jevs::label::Private, jevs::label::Me>,"
        ));
        assert!(rs.contains(
            "config: jevs::file::File::open(key, \"./config.toml\"),"
        ));
    }

    #[test]
    fn generate_resources_mixed() {
        let decls = vec![
            ResourceDecl {
                name: "data".to_string(),
                url: "file:/data/".to_string(),
                access: Access::Read,
                classification: ClassificationLevel::Private,
                integrity: IntegrityLevel::Me,
            },
            ResourceDecl {
                name: "out".to_string(),
                url: "file:/tmp/result.txt".to_string(),
                access: Access::Write,
                classification: ClassificationLevel::Public,
                integrity: IntegrityLevel::Me,
            },
        ];
        let rs = generate_resources_rs(&decls);
        assert!(rs.contains(
            "pub data: jevs::file::FileTree<jevs::label::Private, jevs::label::Me>,"
        ));
        assert!(rs.contains(
            "pub out: jevs::file::File<jevs::label::Public, jevs::label::Me>,"
        ));
        assert!(rs.contains(
            "data: jevs::file::FileTree::open(key, \"/data\"),"
        ));
        assert!(rs.contains(
            "out: jevs::file::File::open(key, \"/tmp/result.txt\"),"
        ));
    }

    #[test]
    fn parse_decls_explicit_labels() {
        let toml = r#"
[resources.web_data]
url = "file:./downloads/"
access = "read"
confidentiality = "public"
integrity = "world"
"#;
        let decls = parse_resource_decls(toml).unwrap();
        assert_eq!(decls[0].classification, ClassificationLevel::Public);
        assert_eq!(decls[0].integrity, IntegrityLevel::World);
    }

    #[test]
    fn plan_id_includes_resource_toml() {
        let id1 = plan_id("task", "catalog", "toml-a");
        let id2 = plan_id("task", "catalog", "toml-b");
        assert_ne!(id1, id2);
    }

    #[test]
    fn extract_fenced_basic() {
        let text = "before\n```rust\nfn main() {}\n```\nafter";
        let result = extract_fenced(text, "rust").unwrap();
        assert_eq!(result, "fn main() {}");
    }

    #[test]
    fn parse_url_dir() {
        let (scheme, path, is_dir) = parse_url("file:./").unwrap();
        assert_eq!(scheme, "file");
        assert_eq!(path, "./");
        assert!(is_dir);
    }

    #[test]
    fn parse_url_file() {
        let (scheme, path, is_dir) = parse_url("file:/tmp/foo").unwrap();
        assert_eq!(scheme, "file");
        assert_eq!(path, "/tmp/foo");
        assert!(!is_dir);
    }

    #[test]
    fn parse_url_abs_dir() {
        let (_, path, is_dir) = parse_url("file:/data/").unwrap();
        assert_eq!(path, "/data/");
        assert!(is_dir);
    }

    #[test]
    fn parse_url_relative_file() {
        let (_, path, is_dir) = parse_url("file:./config.toml").unwrap();
        assert_eq!(path, "./config.toml");
        assert!(!is_dir);
    }
}
