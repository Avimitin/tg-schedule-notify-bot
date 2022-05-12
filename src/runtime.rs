use parking_lot::RwLock;
use std::sync::Arc;
use teloxide::types::{ChatId, UserId};
use teloxide::prelude::*;
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
    bot_username: String,
}

impl BotRuntime {
    pub fn get_group(&self) -> Arc<Vec<ChatId>> {
        let wt = self.whitelist.read();
        wt.groups.clone()
    }

    pub fn new<T: Into<String>>(bot_username: T) -> Self {
        let (tx, _) = broadcast::channel(5);

        Self {
            whitelist: Arc::new(RwLock::new(Whitelist::new())),
            shutdown_sig: tx,
            bot_username: bot_username.into(),
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

    pub fn username(&self) -> &str {
        &self.bot_username
    }
}

/// Send shutdown signal to all subscribed sub task
impl Drop for BotRuntime {
    fn drop(&mut self) {
        self.shutdown_sig.send(1).expect("Shutdown notify channel is already closed!");
    }
}
