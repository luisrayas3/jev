mod tasks;

use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, Write};

const TASKS_SOURCE: &str = include_str!("tasks.rs");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let has_needs = !jevs::manifest::NEEDS.is_empty();
    let has_crossings = !jevs::gate::CROSSINGS.is_empty();

    if has_needs || has_crossings {
        if has_needs {
            eprintln!("Needs:");
            for need in jevs::manifest::NEEDS.iter() {
                eprintln!(
                    "  {:<10} {:<24} {} {}",
                    need.kind, need.path,
                    need.classification, need.integrity,
                );
            }
        }

        if has_crossings {
            let n = jevs::gate::CROSSINGS.len();
            eprintln!(
                "\n{n} label crossing{}",
                if n == 1 { "" } else { "s" },
            );
        }

        loop {
            if has_crossings {
                eprint!("\napprove? [y/N/t=tasks]: ");
            } else {
                eprint!("\napprove? [Y/n/t=tasks]: ");
            }
            std::io::stderr().flush()?;

            let mut input = String::new();
            std::io::stdin().lock().read_line(&mut input)?;

            match input.trim() {
                "t" | "T" => {
                    eprintln!();
                    for line in TASKS_SOURCE.lines() {
                        eprintln!("    {line}");
                    }
                    continue;
                }
                "y" | "Y" | "yes" => break,
                "n" | "N" | "no" => anyhow::bail!("rejected"),
                _ if has_crossings => anyhow::bail!("rejected"),
                _ => break,
            }
        }
    }

    jevs::gate::init()?;

    let key = jevs::RuntimeKey::init(
        std::hash::RandomState::new()
            .build_hasher()
            .finish(),
    )?;
    let mut needs = tasks::create(&key);
    tasks::root(&mut needs).await
}
