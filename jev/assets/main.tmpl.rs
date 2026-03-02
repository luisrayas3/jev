mod resources;
mod tasks;

use std::hash::{BuildHasher, Hasher};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let key = jevs::runtime::RuntimeKey::init(
        std::hash::RandomState::new()
            .build_hasher()
            .finish(),
    )?;
    let mut res = resources::create(&key);
    tasks::root(&mut res).await
}
