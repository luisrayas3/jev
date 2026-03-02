use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::PathBuf;

pub const API_DOCS: &str = r#"## Stash - `jevs::stash::Stash`

```rust
// Store data, get a content-addressed handle
let handle = res.stash.put(b"some data").await?;

// Retrieve data by handle
let data: Vec<u8> = res.stash.get(&handle).await?;

// Content-addressed: same data = same handle
let h1 = res.stash.put(b"hello").await?;
let h2 = res.stash.put(b"hello").await?;
assert_eq!(h1, h2);
```

Plan-local content-addressed blob storage.
Use for intermediate results too large for memory
or shared between tasks.
The hash is the reference: no naming, no conflicts.
Both `put` and `get` take `&self` (shared access),
so stash operations can run in parallel.
Access through `res.stash`.
"#;

/// Content-addressed hash handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hash(String);

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Plan-local content-addressed blob storage.
///
/// Backed by a temporary directory.
/// Cleaned up on drop.
pub struct Stash {
    root: PathBuf,
}

impl Stash {
    /// Create a new stash backed by a temporary directory.
    pub fn new() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "jev-stash-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).context("creating stash directory")?;
        Ok(Stash { root })
    }

    /// Store data and return its content-addressed handle.
    /// Takes `&self`: concurrent puts are safe (idempotent writes).
    pub async fn put(&self, data: &[u8]) -> Result<Hash> {
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        };
        let path = self.root.join(&hash);
        tokio::fs::write(&path, data)
            .await
            .with_context(|| format!("writing to stash {hash}"))?;
        Ok(Hash(hash))
    }

    /// Retrieve data by its content-addressed handle.
    /// Takes `&self`: concurrent gets are safe.
    pub async fn get(&self, handle: &Hash) -> Result<Vec<u8>> {
        let path = self.root.join(&handle.0);
        tokio::fs::read(&path)
            .await
            .with_context(|| format!("reading from stash {}", handle.0))
    }
}

impl Drop for Stash {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_and_get() {
        let stash = Stash::new().unwrap();
        let handle = stash.put(b"hello world").await.unwrap();
        let data = stash.get(&handle).await.unwrap();
        assert_eq!(data, b"hello world");
    }

    #[tokio::test]
    async fn content_addressed() {
        let stash = Stash::new().unwrap();
        let h1 = stash.put(b"same").await.unwrap();
        let h2 = stash.put(b"same").await.unwrap();
        assert_eq!(h1, h2);

        let h3 = stash.put(b"different").await.unwrap();
        assert_ne!(h1, h3);
    }

    #[tokio::test]
    async fn get_missing() {
        let stash = Stash::new().unwrap();
        let bogus = Hash("deadbeef".to_string());
        assert!(stash.get(&bogus).await.is_err());
    }

    #[tokio::test]
    async fn cleanup_on_drop() {
        let root = {
            let stash = Stash::new().unwrap();
            stash.put(b"ephemeral").await.unwrap();
            stash.root.clone()
        };
        assert!(!root.exists());
    }
}
