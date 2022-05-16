use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use teloxide::types::InlineKeyboardMarkup;
use teloxide::{prelude::*, types::ChatId};
use tokio::sync::{mpsc, watch};
use tokio::time as tok_time;
use tracing::{debug, error};

#[derive(Clone)]
/// TaskPool store tasks and a copy of bot.
pub struct TaskPool {
    inc: u32,
    pool: Arc<RwLock<HashMap<u32, TaskInfo>>>,
    bot: AutoSend<Bot>,
}

pub struct TaskInfo {
    interval: u64,
    content: String,
    sig: ShutdownSig,
}

impl TaskPool {
    /// Create a new task pool with zero size vector
    pub fn new(bot: AutoSend<Bot>) -> Self {
        Self {
            pool: Arc::new(RwLock::new(HashMap::new())),
            bot,
            inc: 0,
        }
    }

    /// Spawn a new task. It needs repeat interval, a list of groups to send message, and a init
    /// text to notify.
    pub fn add_task(&mut self, task: ScheduleTask) {
        // lock the pool and write to it
        let mut pool = self.pool.write();
        let task = task.run(pool.len() + 1, self.bot.clone());
        self.inc += 1;
        pool.insert(self.inc, task);
    }

    /// List current running task, return a list of (id, interval, skim content)
    pub fn list_task(&self) -> Vec<(u32, u64, String)> {
        let pool = self.pool.read();

        pool.iter()
            .map(|x| (*(x.0), x.1.interval, x.1.content.to_string()))
            .collect()
    }

    /// Stop a task, and remove it from pool
    pub fn remove(&mut self, index: u32) -> Result<()> {
        let mut pool = self.pool.write();
        if !pool.contains_key(&index) {
            anyhow::bail!("Index invalid");
        }
        let task = pool.remove(&index).unwrap();
        task.sig.shutdown()?;
        Ok(())
    }
}

/// A wrapper for tokio::watch::Sender. For shutdown tokio task.
#[derive(Clone)]
pub struct ShutdownSig(Arc<watch::Sender<u8>>);

impl ShutdownSig {
    /// Send a shutdown signal
    pub fn shutdown(&self) -> Result<()> {
        self.0.send(1)?;
        Ok(())
    }
}

pub struct Editor(Arc<mpsc::Sender<TaskEditType>>);

/// A unit of a repeating notify task
pub struct ScheduleTask {
    /// Task id
    id: usize,
    /// Repeat interval, in minute unit
    interval: u64,
    /// A pool of notifications
    pending_notification: Vec<String>,
    /// A button set to attached on message
    msg_buttons: Option<InlineKeyboardMarkup>,
    /// A signal to close this task
    signal: watch::Sender<u8>,
    /// A channel to edit this task
    editor: mpsc::Sender<TaskEditType>,
    /// A list of chat id
    groups: Vec<ChatId>,

    // Temporary storage for channel receive, don't touch it!
    editor_rx: mpsc::Receiver<TaskEditType>,
    signal_rx: watch::Receiver<u8>,
}

#[derive(Debug)]
/// TaskEditType describe the behavior about updating the task.
enum TaskEditType {
    /// AddNotification describe a add notification behavior. It will add a new notification
    /// text into the task storage.
    AddNotification(String),
}

impl ScheduleTask {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(0);
        let (editor, editor_rx) = mpsc::channel(3);
        Self {
            id: 0,
            interval: 0,
            pending_notification: Vec::new(),
            msg_buttons: None,
            groups: Vec::new(),

            signal: tx,
            editor,

            signal_rx: rx,
            editor_rx,
        }
    }

    pub fn interval(mut self, interval: u64) -> Self {
        self.interval = interval;
        self
    }

    pub fn pending_notification(mut self, pn: Vec<String>) -> Self {
        self.pending_notification = pn;
        self
    }

    pub fn msg_buttons(mut self, btn: InlineKeyboardMarkup) -> Self {
        self.msg_buttons = Some(btn);
        self
    }

    /// Spawn a new tokio task to run a forever loop. It will notify when the ticker send a tick.
    /// Task will consume itself and return necessary information about the task
    pub fn run(mut self, id: usize, bot: AutoSend<Bot>) -> TaskInfo {
        // copy a skim of the content for describing this task
        let skim = self.pending_notification[0].to_string();
        let _: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async move {
            let mut ticker = tok_time::interval(Duration::from_secs(self.interval));
            loop {
                debug!("schedule task {} start sending notification", id);

                tokio::select! {
                    // receive shutdown signal
                    _ = self.signal_rx.changed() => {
                        tracing::info!("Schedule Task {} stop the jobs", id);
                        return Ok(())
                    }

                    // receive edit message
                    edit = self.editor_rx.recv() => {
                        tracing::info!("Editing task {}", id);
                        match edit {
                            Some(TaskEditType::AddNotification(s)) => {
                                self.pending_notification.push(s);
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
                        let text = Arc::new(self.pending_notification[0].to_owned());

                        for gid in self.groups.iter() {
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

        TaskInfo {
            interval: self.interval,
            content: skim,
            sig: ShutdownSig(Arc::new(self.signal)),
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
        self.editor
            .send(TaskEditType::AddNotification(s))
            .await
            .unwrap();
    }
}
