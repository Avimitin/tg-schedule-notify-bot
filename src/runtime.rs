use parking_lot::RwLock;
use std::sync::Arc;
use teloxide::types::{ChatId, UserId};
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::sync::broadcast;
use anyhow::Result;

/// MessageQueue store the message for sending
#[derive(Clone)]
struct MessageQueue {
    v: Vec<String>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self { v: Vec::new() }
    }
}

/// Whitelist store context for authorization
#[derive(Clone)]
pub struct Whitelist {
    /// Maintainers can grant admin, manage bot
    pub maintainers: Vec<UserId>,
    /// Admins can manage bot
    pub admins: Vec<UserId>,
    /// List of groups that bot make response
    pub groups: Arc<Vec<ChatId>>,
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

impl Whitelist {
    pub fn new() -> Self {
        Self {
            maintainers: Vec::new(),
            admins: Vec::new(),
            groups: Arc::new(Vec::new()),
        }
    }

    pub fn has_access(&self, user: UserId) -> bool {
        self.maintainers.iter().find(|&&id| id == user).is_some()
            || self.admins.iter().find(|&&id| id == user).is_some()
    }
}

#[derive(Clone)]
pub struct BotRuntime {
    pub whitelist: Arc<RwLock<Whitelist>>,
    shutdown_sig: broadcast::Sender<u8>,
}

impl BotRuntime {
    pub fn get_group(&self) -> Arc<Vec<ChatId>> {
        let wt = self.whitelist.read();
        wt.groups.clone()
    }

    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(5);

        Self {
            whitelist: Arc::new(RwLock::new(Whitelist::new())),
            shutdown_sig: tx,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let bot = Bot::from_env().auto_send();

        use crate::handler::*;
        let dproot = dptree::entry().branch(Update::filter_message().endpoint(message_handler));
        Dispatcher::builder(bot, dproot)
            .dependencies(dptree::deps![self.clone()])
            .build()
            .setup_ctrlc_handler()
            .dispatch()
            .await;

        Ok(())
    }

    pub fn subscribe_shut_sig(&self) -> broadcast::Receiver<u8> {
        self.shutdown_sig.subscribe()
    }
}

/// Send shutdown signal to all subscribed sub task
impl Drop for BotRuntime {
    fn drop(&mut self) {
        self.shutdown_sig.send(1).expect("Shutdown notify channel is already closed!");
    }
}
