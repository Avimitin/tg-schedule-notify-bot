use anyhow::Result;
use notify_bot::{handler::*, BotRuntime, Whitelist};
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

  info!("Parsing config...");

  let whitelist = Whitelist::new()
    .parse_admins()
    .parse_groups()
    .parse_maintainers();

  info!("Bot start with maintainers: {:#?}", &whitelist);
  // setup bot runtime
  let runtime = BotRuntime::new(bot.clone()).whitelist(whitelist);

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
