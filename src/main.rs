use anyhow::Result;
use notify_bot::BotRuntime;
use tracing::info;
use notify_bot::handler::*;
use teloxide::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Bot initializing...");
    dotenv::dotenv().ok();

    let bot = Bot::from_env().auto_send();

    let username = bot.get_me().await?.username().to_string();
    info!("Bot {} start running", username);

    // setup bot runtime
    let runtime = BotRuntime::new(bot.clone(), username);

    // setup handler
    let dproot = dptree::entry().branch(Update::filter_message().endpoint(message_handler));
    Dispatcher::builder(bot.clone(), dproot)
        .dependencies(dptree::deps![runtime])
        .build()
        .setup_ctrlc_handler()
        .dispatch()
        .await;

    Ok(())
}
