use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use crate::exec::{binary_path, try_build};
use crate::llm::{call_llm_raw, log_exchange, parse_response, Message};
use crate::prompt::SYSTEM_PROMPT;

pub(crate) fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().to_path_buf()
}

pub(crate) fn plans_dir() -> PathBuf {
    workspace_root().join("plans")
}

pub(crate) fn logs_dir() -> PathBuf {
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

pub(crate) fn latest_plan() -> Result<String> {
    let dir = plans_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .context("no plans directory")?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().map(|t| t.is_dir()).unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    entries
        .last()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .context("no plans found")
}

fn write_plan(id: &str, tasks_code: &str) -> Result<PathBuf> {
    let root = workspace_root();
    let plan_dir = plans_dir().join(id);
    let src_dir = plan_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let cargo_toml = include_str!("../assets/Cargo.tmpl.toml")
        .replace("{plan_name}", &format!("plan{id}"))
        .replace(
            "{jevs_path}",
            &root.join("jevs").display().to_string(),
        );
    std::fs::write(plan_dir.join("Cargo.toml"), cargo_toml)?;

    std::fs::write(
        src_dir.join("main.rs"),
        include_str!("../assets/main.tmpl.rs"),
    )?;

    std::fs::write(src_dir.join("tasks.rs"), tasks_code)?;

    Ok(plan_dir)
}

const MAX_RETRIES: usize = 3;

pub(crate) async fn plan_and_compile(
    task: &str,
) -> Result<PathBuf> {
    let catalog = jevs::api::catalog();

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY not set")?;
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

        eprintln!(
            "  attempt {}/{}",
            attempt + 1,
            MAX_RETRIES + 1
        );
        let raw =
            call_llm_raw(&client, &api_key, &messages).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_id_includes_code() {
        let id1 = plan_id("task", "catalog", "code-a");
        let id2 = plan_id("task", "catalog", "code-b");
        assert_ne!(id1, id2);
    }
}
