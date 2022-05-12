use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{ChatId, UserId},
    utils::command::BotCommands,
};
use tracing::{error, info};

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
#[derive(Clone)]
struct Whitelist {
    /// Maintainers can grant admin, manage bot
    maintainers: Vec<UserId>,
    /// Admins can manage bot
    admins: Vec<UserId>,
    /// List of groups that bot make response
    groups: Arc<Vec<ChatId>>,
}

impl Whitelist {
    fn new() -> Self {
        Self {
            maintainers: Vec::new(),
            admins: Vec::new(),
            groups: Arc::new(Vec::new()),
        }
    }
}

#[derive(Clone)]
struct BotRuntime {
    whitelist: Arc<RwLock<Whitelist>>,
    shutdown_sig: tokio::sync::broadcast::Sender<u8>,
}

impl BotRuntime {
    fn get_group(&self) -> Arc<Vec<ChatId>> {
        let wt = self.whitelist.read();
        wt.groups.clone()
    }
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
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Bot initializing");
    dotenv::dotenv().ok();
    run().await
}

async fn run() -> Result<()> {
    let bot = Bot::from_env().auto_send();

    let (tx, _) = tokio::sync::broadcast::channel(5);

    let rt = BotRuntime {
        whitelist: Arc::new(RwLock::new(Whitelist::new())),
        shutdown_sig: tx,
    };

    // AutoSend<Bot> is free to clone, just like Rc
    let sc_task_1 = tokio::spawn(scheduled_notifier(bot.clone(), rt, 30));

    let dproot = dptree::entry().branch(Update::filter_message().endpoint(message_handler));
    Dispatcher::builder(bot, dproot)
        .build()
        .setup_ctrlc_handler()
        .dispatch()
        .await;

    Ok(sc_task_1
        .await
        .with_context(|| "Error occurs during the notification time")??)
}

async fn scheduled_notifier(bot: AutoSend<Bot>, rt: BotRuntime, secs: u64) -> Result<()> {
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(secs));
    let mut rx = rt.shutdown_sig.subscribe();

    loop {
        tokio::select! {
            _ = rx.recv() => {
                return Ok(())
            }
            _ = heartbeat.tick() => {
                let groups = rt.get_group();
                let mut wg = Vec::new();
                for gid in groups.iter() {
                    let bot = bot.clone();
                    let gid = gid.0;
                    let join = tokio::spawn(async move {
                        let group_id = ChatId(gid);
                        bot.send_message(group_id, "Hey").await
                    });
                    wg.push(join);
                }

                for j in wg {
                    match j.await? {
                        Ok(_) => {},
                        Err(e) => error!("Fail to send hey: {}", e),
                    };
                }
            }
        }
    }
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
    let whitelist = rt.whitelist.read();
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
