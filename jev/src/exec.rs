use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub(crate) fn try_build(
    plan_dir: &Path,
) -> std::result::Result<(), String> {
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

pub(crate) fn binary_path(plan_dir: &Path) -> PathBuf {
    let bin_name = plan_dir.file_name().unwrap().to_str().unwrap();
    plan_dir
        .join("target")
        .join("release")
        .join(format!("plan{bin_name}"))
}

pub(crate) fn build_plan(plan_dir: &Path) -> Result<PathBuf> {
    eprintln!("Building plan...");
    try_build(plan_dir)
        .map_err(|e| anyhow::anyhow!("Compilation failed:\n{e}"))?;
    Ok(binary_path(plan_dir))
}

pub(crate) fn run_binary(binary: &Path) -> Result<()> {
    eprintln!("Running...");
    let status = std::process::Command::new(binary)
        .status()
        .context("failed to execute plan binary")?;

    if !status.success() {
        bail!("Plan exited with {status}");
    }
    Ok(())
}
