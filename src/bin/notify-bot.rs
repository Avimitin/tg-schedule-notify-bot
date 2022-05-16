use anyhow::Result;
use notify_bot::{handler::*, BotRuntime, Config, Whitelist};
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
  let cfg = Config::new()
    .parse_admins()
    .parse_maintainers()
    .parse_groups();

  let whitelist = Whitelist::new()
    .maintainers(cfg.maintainers)
    .admins(cfg.admins)
    .groups(cfg.groups);

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
