use anyhow::{Context, Result};
use std::path::PathBuf;

/// Filesystem resource rooted at a directory.
///
/// Safety semantics via Rust's borrow system:
/// - `&Fs`     → read access (shared, parallelizable)
/// - `&mut Fs` → write access (exclusive)
pub struct Fs {
    root: PathBuf,
}

impl Fs {
    /// Open a filesystem resource rooted at `root`.
    pub fn open(root: &str) -> Self {
        let root = std::fs::canonicalize(root)
            .unwrap_or_else(|_| PathBuf::from(root));
        Fs { root }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    /// Read a file's contents. Takes `&self` — shared read access.
    pub async fn read(&self, path: &str) -> Result<String> {
        let full = self.resolve(path);
        tokio::fs::read_to_string(&full)
            .await
            .with_context(|| format!("reading {}", full.display()))
    }

    /// Write content to a file. Takes `&mut self` — exclusive write access.
    pub async fn write(&mut self, path: &str, content: &str) -> Result<()> {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, content)
            .await
            .with_context(|| format!("writing {}", full.display()))
    }

    /// Glob for files matching a pattern. Takes `&self` — shared read access.
    pub async fn glob(&self, pattern: &str) -> Result<Vec<String>> {
        let full_pattern = self.root.join(pattern);
        let pattern_str = full_pattern
            .to_str()
            .context("invalid pattern")?
            .to_string();
        let root = self.root.clone();

        tokio::task::spawn_blocking(move || {
            let entries: Vec<String> = glob::glob(&pattern_str)
                .context("invalid glob pattern")?
                .filter_map(|e| e.ok())
                .filter_map(|p| {
                    p.strip_prefix(&root)
                        .ok()
                        .and_then(|rel| rel.to_str().map(String::from))
                })
                .collect();
            Ok(entries)
        })
        .await?
    }
}
