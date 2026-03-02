use crate::label::{
    Classification, Integrity, Labeled, SatisfiesClassification,
    SatisfiesIntegrity,
};
use crate::runtime::RuntimeKey;
use anyhow::{Context, Result};
use std::marker::PhantomData;
use std::path::PathBuf;

pub const FILE_API_DOCS: &str = r#"## Single file - `jevs::file::File`

```rust
// Read returns labeled data
let content = res.config.read().await?;

// Transform with map (preserves labels)
let upper = content.map(|s| s.to_uppercase());

// Write labeled data (labels must be compatible)
res.config.write(upper).await?;

// Write fresh local data
res.config.write(jevs::label::Labeled::local("new".to_string())).await?;
```

Key: `&File` = read, `&mut File` = write.
No path parameter; each File is bound to one path.

Do NOT construct File yourself.
It is provided via `res.<name>`.
"#;

pub const TREE_API_DOCS: &str = r#"## Directory tree - `jevs::file::FileTree`

```rust
// Read returns labeled data
let content = res.fs.read("file.txt").await?;

// Glob for files (returns paths, not labeled)
let files: Vec<String> = res.fs.glob("*.rs").await?;

// Write labeled data
res.fs.write("out.txt", content).await?;

// Write fresh local data
res.fs.write("out.txt", jevs::label::Labeled::local("hi".to_string())).await?;
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
/// Type parameters carry classification and integrity labels.
/// - `&File`     → read access (shared), returns `Labeled<String, C, I>`
/// - `&mut File` → write access (exclusive), takes labeled data
pub struct File<C: Classification, I: Integrity> {
    path: PathBuf,
    _c: PhantomData<C>,
    _i: PhantomData<I>,
}

impl<C: Classification, I: Integrity> File<C, I> {
    /// Open a single-file resource at `path`.
    /// Requires a `&RuntimeKey`; only `main` holds one.
    pub fn open(_key: &RuntimeKey, path: &str) -> Self {
        let path = std::fs::canonicalize(path)
            .unwrap_or_else(|_| PathBuf::from(path));
        File {
            path,
            _c: PhantomData,
            _i: PhantomData,
        }
    }

    /// Read the file's contents.
    /// Takes `&self`, shared read access.
    /// Returns data carrying the resource's labels.
    pub async fn read(&self) -> Result<Labeled<String, C, I>> {
        let content = tokio::fs::read_to_string(&self.path)
            .await
            .with_context(|| format!("reading {}", self.path.display()))?;
        Ok(Labeled::new(content))
    }

    /// Write labeled data to the file.
    /// Takes `&mut self`, exclusive write access.
    /// Data labels must be compatible with the resource:
    /// classification at most as restrictive,
    /// integrity at least as high.
    pub async fn write<Ci: Classification, Ii: Integrity>(
        &mut self,
        content: Labeled<String, Ci, Ii>,
    ) -> Result<()>
    where
        Ci: SatisfiesClassification<C>,
        Ii: SatisfiesIntegrity<I>,
    {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, content.into_inner())
            .await
            .with_context(|| format!("writing {}", self.path.display()))
    }
}

/// Directory tree resource rooted at a directory.
///
/// Type parameters carry classification and integrity labels.
/// - `&FileTree`     → read access (shared), returns `Labeled<String, C, I>`
/// - `&mut FileTree` → write access (exclusive), takes labeled data
pub struct FileTree<C: Classification, I: Integrity> {
    root: PathBuf,
    _c: PhantomData<C>,
    _i: PhantomData<I>,
}

impl<C: Classification, I: Integrity> FileTree<C, I> {
    /// Open a directory tree resource rooted at `root`.
    /// Requires a `&RuntimeKey`; only `main` holds one.
    pub fn open(_key: &RuntimeKey, root: &str) -> Self {
        let root = std::fs::canonicalize(root)
            .unwrap_or_else(|_| PathBuf::from(root));
        FileTree {
            root,
            _c: PhantomData,
            _i: PhantomData,
        }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    /// Read a file's contents.
    /// Takes `&self`, shared read access.
    /// Returns data carrying the resource's labels.
    pub async fn read(&self, path: &str) -> Result<Labeled<String, C, I>> {
        let full = self.resolve(path);
        let content = tokio::fs::read_to_string(&full)
            .await
            .with_context(|| format!("reading {}", full.display()))?;
        Ok(Labeled::new(content))
    }

    /// Write labeled data to a file.
    /// Takes `&mut self`, exclusive write access.
    /// Data labels must be compatible with the resource.
    pub async fn write<Ci: Classification, Ii: Integrity>(
        &mut self,
        path: &str,
        content: Labeled<String, Ci, Ii>,
    ) -> Result<()>
    where
        Ci: SatisfiesClassification<C>,
        Ii: SatisfiesIntegrity<I>,
    {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, content.into_inner())
            .await
            .with_context(|| format!("writing {}", full.display()))
    }

    /// Glob for files matching a pattern.
    /// Takes `&self`, shared read access.
    /// Returns paths (unlabeled structural metadata).
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
    use crate::label::{Private, Public, Me};

    use std::sync::OnceLock;

    fn test_key() -> &'static RuntimeKey {
        static KEY: OnceLock<RuntimeKey> = OnceLock::new();
        KEY.get_or_init(|| RuntimeKey::init(0).unwrap())
    }

    // -- FileTree tests --

    #[tokio::test]
    async fn tree_read_and_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut f: FileTree<Private, Me> =
            FileTree::open(test_key(), dir.path().to_str().unwrap());
        f.write(
            "hello.txt",
            Labeled::local("hello world".to_string()),
        )
        .await
        .unwrap();
        let content = f.read("hello.txt").await.unwrap();
        assert_eq!(content.into_inner(), "hello world");
    }

    #[tokio::test]
    async fn tree_glob_matches() {
        let dir = tempfile::tempdir().unwrap();
        let mut f: FileTree<Private, Me> =
            FileTree::open(test_key(), dir.path().to_str().unwrap());
        f.write("a.txt", Labeled::local("a".to_string()))
            .await
            .unwrap();
        f.write("b.txt", Labeled::local("b".to_string()))
            .await
            .unwrap();
        f.write("c.md", Labeled::local("c".to_string()))
            .await
            .unwrap();
        let mut matches = f.glob("*.txt").await.unwrap();
        matches.sort();
        assert_eq!(matches, vec!["a.txt", "b.txt"]);
    }

    #[tokio::test]
    async fn tree_read_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let f: FileTree<Private, Me> =
            FileTree::open(test_key(), dir.path().to_str().unwrap());
        assert!(f.read("nope.txt").await.is_err());
    }

    // -- File tests --

    #[tokio::test]
    async fn file_read_and_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let mut f: File<Private, Me> =
            File::open(test_key(), path.to_str().unwrap());
        f.write(Labeled::local("hello file".to_string()))
            .await
            .unwrap();
        let content = f.read().await.unwrap();
        assert_eq!(content.into_inner(), "hello file");
    }

    #[tokio::test]
    async fn file_read_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.txt");
        let f: File<Private, Me> =
            File::open(test_key(), path.to_str().unwrap());
        assert!(f.read().await.is_err());
    }

    #[tokio::test]
    async fn file_write_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/dir/test.txt");
        let mut f: File<Private, Me> =
            File::open(test_key(), path.to_str().unwrap());
        f.write(Labeled::local("nested".to_string()))
            .await
            .unwrap();
        let content = f.read().await.unwrap();
        assert_eq!(content.into_inner(), "nested");
    }

    #[tokio::test]
    async fn file_write_public_to_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let mut f: File<Private, Me> =
            File::open(test_key(), path.to_str().unwrap());
        let data: Labeled<String, Public, Me> =
            Labeled::local("public data".to_string());
        f.write(data).await.unwrap();
        let content = f.read().await.unwrap();
        assert_eq!(content.into_inner(), "public data");
    }
}
