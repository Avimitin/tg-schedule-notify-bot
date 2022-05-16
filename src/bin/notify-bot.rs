use anyhow::Result;
use notify_bot::handler::*;
use notify_bot::BotRuntime;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Bot initializing...");
    dotenv::dotenv().ok();

    let bot = Bot::from_env().auto_send();

    let username = bot.get_me().await?.username().to_string();
    info!("Bot {} start running", username);

    // setup bot runtime
    let runtime = BotRuntime::new(bot.clone());

    // setup handler
    Dispatcher::builder(bot.clone(), handler_schema())
        .dependencies(dptree::deps![
            runtime,
            InMemStorage::<AddTaskDialogueCurrentState>::new()
        ])
        .build()
        .setup_ctrlc_handler()
        .dispatch()
        .await;

    Ok(())
}
