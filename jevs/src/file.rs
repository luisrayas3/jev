use crate::runtime::RuntimeKey;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub const FILE_API_DOCS: &str = r#"## Single file - `jevs::file::File`

```rust
// Read the file (shared ref, can parallelize reads)
let content: String = res.config.read().await?;

// Write the file (exclusive ref, no concurrent access)
res.config.write("new content").await?;
```

Key: `&File` = read, `&mut File` = write.
No path parameter; each File is bound to one path.

Do NOT construct File yourself.
It is provided via `res.<name>`.
"#;

pub const TREE_API_DOCS: &str = r#"## Directory tree - `jevs::file::FileTree`

```rust
// Read a file (shared ref, can parallelize reads)
let content: String = res.fs.read("file.txt").await?;

// Glob for files (shared ref)
let files: Vec<String> = res.fs.glob("*.rs").await?;

// Write a file (exclusive ref, no concurrent access)
res.fs.write("out.txt", "content").await?;
```

Key: `&FileTree` = read, `&mut FileTree` = write.
Multiple reads can run in parallel via `tokio::join!`.
A write requires exclusive access;
no concurrent reads or writes.

Do NOT construct FileTree yourself.
It is provided via `res.<name>`.
"#;

/// Single-file resource bound to one path.
///
/// Safety semantics via Rust's borrow system:
/// - `&File`     → read access (shared, parallelizable)
/// - `&mut File` → write access (exclusive)
pub struct File {
    path: PathBuf,
    _private: (),
}

impl File {
    /// Open a single-file resource at `path`.
    /// Requires a `&RuntimeKey`; only `main` holds one.
    pub fn open(_key: &RuntimeKey, path: &str) -> Self {
        let path = std::fs::canonicalize(path)
            .unwrap_or_else(|_| PathBuf::from(path));
        File { path, _private: () }
    }

    /// Read the file's contents. Takes `&self`, shared read access.
    pub async fn read(&self) -> Result<String> {
        tokio::fs::read_to_string(&self.path)
            .await
            .with_context(|| format!("reading {}", self.path.display()))
    }

    /// Write content to the file. Takes `&mut self`, exclusive write access.
    pub async fn write(&mut self, content: &str) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, content)
            .await
            .with_context(|| format!("writing {}", self.path.display()))
    }
}

/// Directory tree resource rooted at a directory.
///
/// Safety semantics via Rust's borrow system:
/// - `&FileTree`     → read access (shared, parallelizable)
/// - `&mut FileTree` → write access (exclusive)
pub struct FileTree {
    root: PathBuf,
    _private: (),
}

impl FileTree {
    /// Open a directory tree resource rooted at `root`.
    /// Requires a `&RuntimeKey`; only `main` holds one.
    pub fn open(_key: &RuntimeKey, root: &str) -> Self {
        let root = std::fs::canonicalize(root)
            .unwrap_or_else(|_| PathBuf::from(root));
        FileTree { root, _private: () }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    /// Read a file's contents. Takes `&self`, shared read access.
    pub async fn read(&self, path: &str) -> Result<String> {
        let full = self.resolve(path);
        tokio::fs::read_to_string(&full)
            .await
            .with_context(|| format!("reading {}", full.display()))
    }

    /// Write content to a file. Takes `&mut self`, exclusive write access.
    pub async fn write(&mut self, path: &str, content: &str) -> Result<()> {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, content)
            .await
            .with_context(|| format!("writing {}", full.display()))
    }

    /// Glob for files matching a pattern. Takes `&self`, shared read access.
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

    use std::sync::OnceLock;

    fn test_key() -> &'static RuntimeKey {
        static KEY: OnceLock<RuntimeKey> = OnceLock::new();
        KEY.get_or_init(|| RuntimeKey::init(0).unwrap())
    }

    // -- FileTree tests --

    #[tokio::test]
    async fn tree_read_and_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = FileTree::open(&test_key(), dir.path().to_str().unwrap());
        f.write("hello.txt", "hello world").await.unwrap();
        let content = f.read("hello.txt").await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn tree_glob_matches() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = FileTree::open(&test_key(), dir.path().to_str().unwrap());
        f.write("a.txt", "a").await.unwrap();
        f.write("b.txt", "b").await.unwrap();
        f.write("c.md", "c").await.unwrap();
        let mut matches = f.glob("*.txt").await.unwrap();
        matches.sort();
        assert_eq!(matches, vec!["a.txt", "b.txt"]);
    }

    #[tokio::test]
    async fn tree_read_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = FileTree::open(&test_key(), dir.path().to_str().unwrap());
        assert!(f.read("nope.txt").await.is_err());
    }

    // -- File tests --

    #[tokio::test]
    async fn file_read_and_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let mut f = File::open(&test_key(), path.to_str().unwrap());
        f.write("hello file").await.unwrap();
        let content = f.read().await.unwrap();
        assert_eq!(content, "hello file");
    }

    #[tokio::test]
    async fn file_read_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.txt");
        let f = File::open(&test_key(), path.to_str().unwrap());
        assert!(f.read().await.is_err());
    }

    #[tokio::test]
    async fn file_write_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/dir/test.txt");
        let mut f = File::open(&test_key(), path.to_str().unwrap());
        f.write("nested").await.unwrap();
        let content = f.read().await.unwrap();
        assert_eq!(content, "nested");
    }
}
