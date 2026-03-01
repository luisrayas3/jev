use crate::runtime::RuntimeKey;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub const API_DOCS: &str = r#"## Filesystem — `jevs::file::File`

```rust
// Read a file (shared ref — can parallelize reads)
let content: String = res.fs.read("file.txt").await?;

// Glob for files (shared ref)
let files: Vec<String> = res.fs.glob("*.rs").await?;

// Write a file (exclusive ref — no concurrent access)
res.fs.write("out.txt", "content").await?;
```

Key: `&File` = read, `&mut File` = write.
Multiple reads can run in parallel via `tokio::join!`.
A write requires exclusive access —
no concurrent reads or writes.

Do NOT construct File yourself.
It is provided via `res.fs`.
"#;

/// Filesystem resource rooted at a directory.
///
/// Safety semantics via Rust's borrow system:
/// - `&File`     → read access (shared, parallelizable)
/// - `&mut File` → write access (exclusive)
pub struct File {
    root: PathBuf,
    _private: (),
}

impl File {
    /// Open a filesystem resource rooted at `root`.
    /// Requires a `RuntimeKey` — not available in generated task code.
    pub fn open(key: RuntimeKey, root: &str) -> Self {
        let _ = key;
        let root = std::fs::canonicalize(root)
            .unwrap_or_else(|_| PathBuf::from(root));
        File { root, _private: () }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> RuntimeKey {
        RuntimeKey::new(0x6A65_7673)
    }

    #[tokio::test]
    async fn read_and_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = File::open(test_key(), dir.path().to_str().unwrap());
        f.write("hello.txt", "hello world").await.unwrap();
        let content = f.read("hello.txt").await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn glob_matches() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = File::open(test_key(), dir.path().to_str().unwrap());
        f.write("a.txt", "a").await.unwrap();
        f.write("b.txt", "b").await.unwrap();
        f.write("c.md", "c").await.unwrap();
        let mut matches = f.glob("*.txt").await.unwrap();
        matches.sort();
        assert_eq!(matches, vec!["a.txt", "b.txt"]);
    }

    #[tokio::test]
    async fn read_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = File::open(test_key(), dir.path().to_str().unwrap());
        assert!(f.read("nope.txt").await.is_err());
    }
}
