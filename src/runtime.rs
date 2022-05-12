use parking_lot::RwLock;
use std::sync::Arc;
use teloxide::types::{ChatId, UserId};
use teloxide::prelude::*;
use tokio::sync::broadcast;
use anyhow::Result;
use crate::schedule::TaskPool;

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
            maintainers: vec![UserId(649191333)],
            admins: Vec::new(),
            groups: Arc::new(vec![ChatId(649191333)]),
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
    task_pool: TaskPool,
    bot: AutoSend<Bot>,
}

impl BotRuntime {
    pub fn get_group(&self) -> Arc<Vec<ChatId>> {
        let wt = self.whitelist.read();
        wt.groups.clone()
    }

    pub fn new() -> Self {
        let bot = Bot::from_env().auto_send();

        let (tx, _) = broadcast::channel(5);

        Self {
            bot,
            whitelist: Arc::new(RwLock::new(Whitelist::new())),
            shutdown_sig: tx,
            bot_username: "".to_string(),
            task_pool: TaskPool::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        use crate::handler::*;

        // setup bot username
        let username = self.bot.get_me().await?;
        self.bot_username = username.username().to_string();

        // setup handler
        let dproot = dptree::entry().branch(Update::filter_message().endpoint(message_handler));
        Dispatcher::builder(self.bot.clone(), dproot)
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

    pub fn add_schedule_task(&mut self, secs: u64) {
        let wt = self.whitelist.read();
        let groups = wt.groups.clone();
        // give a copy of the groups when create task
        // this is not a frequent operation, so it is ok
        self.task_pool.add_task(secs, groups.to_vec(), self.bot.clone())
    }
}
