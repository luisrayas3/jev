use std::sync::OnceLock;

static KEY: OnceLock<u64> = OnceLock::new();

pub struct RuntimeKey(());

impl RuntimeKey {
    /// Initialize with a random key. Once only; second call fails.
    /// Returns the only RuntimeKey instance.
    pub fn init(key: u64) -> anyhow::Result<Self> {
        KEY.set(key)
            .map_err(|_| anyhow::anyhow!("runtime key already set"))?;
        Ok(RuntimeKey(()))
    }

}
