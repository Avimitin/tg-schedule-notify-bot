use crate::schedule::TaskPool;
use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{ChatId, UserId};
use tokio::fs;
use tokio::sync::broadcast;

/// Whitelist store context for authorization
#[derive(Clone, Debug)]
pub struct Whitelist {
  /// Maintainers can grant admin, manage bot
  pub maintainers: Vec<UserId>,
  /// Admins can manage bot
  pub admins: Vec<UserId>,
  /// List of groups that bot make response
  pub groups: Vec<ChatId>,
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
      groups: vec![ChatId(649191333)],
    }
  }

  /// Test if the user is one of the maintainers or admins.
  pub fn has_access(&self, user: UserId) -> bool {
    self.maintainers.iter().any(|&id| id == user) || self.admins.iter().any(|&id| id == user)
  }

  pub fn is_maintainers(&self, user: u64) -> bool {
    let user = UserId(user);
    self.maintainers.iter().any(|&id| id == user)
  }

  pub fn maintainers(mut self, m: Vec<u64>) -> Self {
    self.maintainers = m.iter().map(|x| UserId(*x)).collect();
    self
  }

  pub fn admins(mut self, m: Vec<u64>) -> Self {
    self.admins = m.iter().map(|x| UserId(*x)).collect();
    self
  }

  pub fn groups(mut self, g: Vec<i64>) -> Self {
    self.groups = g.iter().map(|x| ChatId(*x)).collect();
    self
  }

  pub async fn save(&self) -> Result<()> {
    let file = ".env";
    // we can guarantee that when we save the config, teloxide token is already init
    let content = vec![
      format!(
        "TELOXIDE_TOKEN={}",
        std::env::var("TELOXIDE_TOKEN").unwrap()
      ),
      format!(
        "NOTIFY_BOT_ADMINS={}",
        self
          .admins
          .iter()
          .map(|x| format!("{x}"))
          .collect::<Vec<String>>()
          .join(",")
      ),
      format!(
        "NOTIFY_BOT_GROUPS={}",
        self
          .groups
          .iter()
          .map(|x| format!("{x}"))
          .collect::<Vec<String>>()
          .join(",")
      ),
      format!(
        "NOTIFY_BOT_MAINTAINERS={}",
        self
          .maintainers
          .iter()
          .map(|x| format!("{x}"))
          .collect::<Vec<String>>()
          .join(",")
      ),
    ]
    .join("\n");

    Ok(fs::write(file, content).await?)
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
  pub fn get_group(&self) -> Vec<ChatId> {
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

  pub fn whitelist(mut self, wt: Whitelist) -> Self {
    self.whitelist = Arc::new(RwLock::new(wt));
    self
  }

  pub fn add_admin(&mut self, id: u64) {
    let mut wt = self.whitelist.write();
    wt.admins.push(UserId(id));
    wt.admins.sort_unstable();
  }

  pub fn del_admin(&mut self, id: u64) -> Result<()> {
    let mut wt = self.whitelist.write();
    let i = wt
      .admins
      .binary_search(&UserId(id))
      .map_err(|_| anyhow::anyhow!("User not exist!"))?;
    wt.admins.remove(i);
    Ok(())
  }

  pub fn add_group(&mut self, gid: i64) {
    let mut wt = self.whitelist.write();
    wt.groups.push(ChatId(gid));
    wt.groups.sort_unstable();
  }

  pub fn del_group(&mut self, gid: i64) -> Result<()> {
    let mut wt = self.whitelist.write();
    let i = wt
      .groups
      .binary_search(&ChatId(gid))
      .map_err(|_| anyhow::anyhow!("User not exist!"))?;
    wt.groups.remove(i);
    Ok(())
  }

  fn copy_whitelist(&self) -> Whitelist {
    let wt = self.whitelist.read();
    wt.clone()
  }

  pub async fn save_whitelist(&self) -> Result<()> {
    let wt = self.copy_whitelist();
    wt.save().await
  }
}
