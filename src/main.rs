use anyhow::{Context, Result};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use teloxide::{prelude::*, types::UserId, utils::command::BotCommands};
use tracing::{debug, error, info};

/// MessageQueue store the message for sending
#[derive(Clone)]
struct MessageQueue {
    v: Vec<String>,
}

impl MessageQueue {
    fn new() -> Self {
        Self { v: Vec::new() }
    }
}

/// Whitelist store context for authorization
struct Whitelist {
    /// Maintainers can grant admin, manage bot
    maintainers: Vec<UserId>,
    /// Admins can manage bot
    admins: Vec<UserId>,
    /// List of groups that bot make response
    groups: Vec<i64>,
}

#[derive(Clone)]
struct BotRuntime {
    whitelist: Arc<Mutex<Whitelist>>,
}

#[derive(BotCommands)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "Display this text")]
    Help,
    #[command(description = "Start")]
    Start,
    #[command(description = "Create a new admin")]
    Grant,
    #[command(description = "Add message into pending queue")]
    Add,
    #[command(description = "List pending messages")]
    List,
    #[command(description = "Remove message")]
    Remove,
    #[command(description = "Clean the whole message queue")]
    Clean,
    #[command(description = "Update timer")]
    Settime,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Bot initializing");
    dotenv::dotenv().ok();
    run().await
}

async fn run() {
    let bot = Bot::from_env().auto_send();

    let dproot = dptree::entry().branch(Update::filter_message().endpoint(message_handler));
    Dispatcher::builder(bot, dproot)
        .build()
        .setup_ctrlc_handler()
        .dispatch()
        .await;
}

async fn message_handler(msg: Message, bot: AutoSend<Bot>, rt: BotRuntime) -> Result<()> {
    let sender = msg.from().unwrap().id;
    if !authorize(msg, sender, rt).has_access() {
        anyhow::bail!("No access")
    }

    Ok(())
}

enum AuthorizeResult {
    AccessAllow,
    AccessDeny,
}

impl AuthorizeResult {
    fn has_access(self) -> bool {
        match self {
            Self::AccessDeny => false,
            Self::AccessAllow => true,
        }
    }
}

fn authorize(msg: Message, id: UserId, rt: BotRuntime) -> AuthorizeResult {
    let whitelist = rt.whitelist.lock().unwrap();
    // if it is in chat, and it is maintainer/admin calling
    let result = msg.chat.is_private()
        && whitelist.maintainers.iter().find(|&&x| x == id).is_some()
        && whitelist.admins.iter().find(|&&x| x == id).is_some();

    if result == true {
        AuthorizeResult::AccessAllow
    } else {
        AuthorizeResult::AccessDeny
    }
}
