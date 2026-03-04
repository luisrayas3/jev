mod tasks;

use std::hash::{BuildHasher, Hasher};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    jevs::manifest::init()?;
    jevs::gate::init()?;
    let key = jevs::RuntimeKey::init(
        std::hash::RandomState::new()
            .build_hasher()
            .finish(),
    )?;
    let mut needs = tasks::create(&key);
    tasks::root(&mut needs).await
}
