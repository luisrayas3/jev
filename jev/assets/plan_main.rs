mod resources;
mod tasks;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut res = resources::create();
    tasks::root(&mut res).await
}
