use crate::schedule::TaskPool;
use parking_lot::RwLock;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{ChatId, UserId};
use tokio::sync::broadcast;

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

impl Default for Whitelist {
    fn default() -> Self {
        Self::new()
    }
}

impl Whitelist {
    pub fn new() -> Self {
        Self {
            maintainers: vec![UserId(649191333)],
            admins: Vec::new(),
            groups: Arc::new(vec![ChatId(649191333)]),
        }
    }

    /// Test if the user is one of the maintainers or admins.
    pub fn has_access(&self, user: UserId) -> bool {
        self.maintainers.iter().any(|&id| id == user) || self.admins.iter().any(|&id| id == user)
    }
}

/// BotRuntime is a memory storage for running the bot.
pub struct BotRuntime {
    pub whitelist: Arc<RwLock<Whitelist>>,
    shutdown_sig: Arc<broadcast::Sender<u8>>,
    pub task_pool: TaskPool,
}

impl Clone for BotRuntime {
    fn clone(&self) -> Self {
        Self {
            whitelist: Arc::clone(&self.whitelist),
            shutdown_sig: Arc::clone(&self.shutdown_sig),
            task_pool: self.task_pool.clone(),
        }
    }
}

impl BotRuntime {
    /// get_group lock the RwLock in read mode, return a Atomic reference to the groups array
    pub fn get_group(&self) -> Arc<Vec<ChatId>> {
        let wt = self.whitelist.read();
        wt.groups.clone()
    }

    /// Create a new runtime with activated bot and bot username.
    pub fn new(bot: AutoSend<Bot>) -> Self {
        let (tx, _) = broadcast::channel(5);

        Self {
            whitelist: Arc::new(RwLock::new(Whitelist::new())),
            shutdown_sig: Arc::new(tx),
            task_pool: TaskPool::new(bot),
        }
    }

    /// Subscribe a signal to know if the BotRuntime get shutdown
    pub fn subscribe_shut_sig(&self) -> broadcast::Receiver<u8> {
        self.shutdown_sig.subscribe()
    }
}
