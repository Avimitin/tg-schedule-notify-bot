use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use teloxide::{prelude::*, types::ChatId};
use tokio::sync::{mpsc, watch};
use tokio::time as tok_time;
use tracing::{debug, error};

#[derive(Clone)]
/// TaskPool store tasks and a copy of bot.
pub struct TaskPool {
    pool: Arc<RwLock<Vec<ScheduleTask>>>,
    bot: AutoSend<Bot>,
}

impl TaskPool {
    /// Create a new task pool with zero size vector
    pub fn new(bot: teloxide::prelude::AutoSend<teloxide::prelude::Bot>) -> Self {
        Self {
            pool: Arc::new(RwLock::new(Vec::new())),
            bot
        }
    }

    /// Spawn a new task. It needs repeat interval, a list of groups to send message, and a init
    /// text to notify.
    pub fn add_task(&mut self, mins: u64, groups: Vec<ChatId>, init_text: String) {
        // lock the pool and write to it
        let mut pool = self.pool.write();
        // FIXME: from_secs should be replace with minutes
        let task = ScheduleTask::new(pool.len() + 1, Duration::from_secs(mins), groups, self.bot.clone(), init_text);
        pool.push(task);
    }

    /// List current running task
    pub fn list_task(&self) -> Vec<(usize, u64)> {
        let pool = self.pool.read();

        pool.iter().map(|x| (x.id, x.interval)).collect()
    }

    /// Stop a task, and remove it from pool
    pub fn remove(&mut self, index: usize) -> Result<()> {
        let mut pool = self.pool.write();
        if index == 0 || index - 1 > pool.len() - 1 {
            return Err(anyhow::anyhow!("Task {} is not exist!", index));
        }
        let task = pool.remove(index - 1);
        task.stop();
        Ok(())
    }
}

/// A unit of a repeating notify task
pub struct ScheduleTask {
    /// Task id
    id: usize,
    /// Repeat interval, in minute unit
    interval: u64,
    /// A signal to close this task
    signal: watch::Sender<u8>,
    /// A channel to edit this task
    editor: mpsc::Sender<TaskEditType>,
}

#[derive(Debug)]
/// TaskEditType describe the behavior about updating the task.
enum TaskEditType {
    /// AddNotification describe a add notification behavior. It will add a new notification
    /// text into the task storage.
    AddNotification(String),
}

impl ScheduleTask {
    /// Spawn a new tokio task to run a forever loop. It will notify when the ticker send a tick
    pub fn new(id: usize, dur: Duration, groups: Vec<ChatId>, bot: AutoSend<Bot>, init_text: String) -> Self {
        let (tx, mut rx) = watch::channel(0);
        let (editor, mut edit_sig) = mpsc::channel(3);

        let _: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async move {
            let mut ticker = tok_time::interval(dur);
            let id = id.clone();
            let mut pending_notification = vec![init_text];
            let mut groups = groups.clone();
            loop {
                debug!("schedule task {} start sending notification", id);

                tokio::select! {
                    // receive shutdown signal
                    _ = rx.changed() => {
                        tracing::info!("Schedule Task {} stop the jobs", id);
                        return Ok(())
                    }

                    // receive edit message
                    edit = edit_sig.recv() => {
                        tracing::info!("Editing task {}", id);
                        match edit {
                            Some(TaskEditType::AddNotification(s)) => {
                                pending_notification.push(s);
                            },
                            None => {
                                tracing::error!("Task {} is closed", id);
                            }
                        }
                    }

                    // new ticker received
                    _ = ticker.tick() => {
                        let mut wg = Vec::new();
                        // clone once for move between thread
                        let text = Arc::new(pending_notification[0].to_owned());

                        for gid in groups.iter() {
                            let bot = bot.clone();
                            let text = text.clone();
                            let gid = gid.0;
                            let join = tokio::spawn(async move {
                                let group_id = ChatId(gid);
                                bot.send_message(group_id, text.as_str()).await
                            });
                            wg.push(join);
                        }

                        // wait for all send message done for their jobs
                        for j in wg {
                            match j.await? {
                                Ok(_) => {},
                                Err(e) => error!("Fail to send hey: {}", e),
                            };
                        }
                    }
                }
            }
        });

        Self {
            editor,
            signal: tx,
            interval: dur.as_secs(),
            id,
        }
    }

    /// Send a to the spawed task to stop the task.
    pub fn stop(&self) {
        match self.signal.send(1) {
            Ok(_) => {}
            Err(e) => error!("fail to stop schedule task {}: {}", self.id, e),
        };
    }

    /// A wrapper function to add a new notification text to the task
    pub async fn add_notification(&self, s: String) {
        self.editor.send(TaskEditType::AddNotification(s)).await.unwrap();
    }
}

