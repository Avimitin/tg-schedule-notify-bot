use anyhow::Result;
use notify_bot::BotRuntime;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Bot initializing");
    dotenv::dotenv().ok();

    let mut brt = BotRuntime::new();
    brt.run().await?;

    Ok(())
}
