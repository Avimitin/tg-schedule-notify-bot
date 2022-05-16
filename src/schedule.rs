use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use teloxide::payloads::SendMessageSetters;
use teloxide::types::InlineKeyboardMarkup;
use teloxide::{prelude::*, types::ChatId};
use tokio::sync::{mpsc, watch};
use tokio::time as tok_time;
use tracing::{debug, error};

/// A global counter to assign unique id for task
static TASK_INC_ID: AtomicU32 = AtomicU32::new(0);

/// TaskPool store tasks and a copy of bot.
pub struct TaskPool {
    pool: Arc<RwLock<HashMap<u32, TaskInfo>>>,
    bot: AutoSend<Bot>,
}

impl Clone for TaskPool {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
            bot: self.bot.clone(),
        }
    }
}

#[derive(Debug)]
pub struct TaskInfo {
    interval: u64,
    content: String,
    sig: ShutdownSig,
    editor: Editor,
}

impl TaskPool {
    /// Create a new task pool with zero size vector
    pub fn new(bot: AutoSend<Bot>) -> Self {
        Self {
            pool: Arc::new(RwLock::new(HashMap::new())),
            bot,
        }
    }

    /// Spawn a new task. It needs repeat interval, a list of groups to send message, and a init
    /// text to notify.
    pub fn add_task(&mut self, task: ScheduleTask) {
        // lock the pool and write to it
        let mut pool = self.pool.write();
        let id = TASK_INC_ID.fetch_add(1, Ordering::SeqCst);
        let task = task.run(id, self.bot.clone());
        // this cast might be safe, as user will not create int max 32bit task
        pool.insert(id, task);
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
#[derive(Clone, Debug)]
pub struct ShutdownSig(Arc<watch::Sender<u8>>);

impl ShutdownSig {
    /// Send a shutdown signal
    pub fn shutdown(&self) -> Result<()> {
        self.0.send(1)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Editor(mpsc::Sender<TaskEditType>);

/// A unit of a repeating notify task
pub struct ScheduleTask {
    /// Task id
    id: u32,
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

    pub fn groups(mut self, groups: Vec<ChatId>) -> Self {
        self.groups = groups;
        self
    }

    /// Spawn a new tokio task to run a forever loop. It will notify when the ticker send a tick.
    /// Task will consume itself and return necessary information about the task
    pub fn run(mut self, id: u32, bot: AutoSend<Bot>) -> TaskInfo {
        // copy a skim of the content for describing this task
        let skim = self.pending_notification[0].to_string();
        let _: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async move {
            let mut ticker = tok_time::interval(Duration::from_secs(self.interval));
            loop {
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
                                break Err(anyhow::anyhow!(
                                        "Task {} editor channel closed unexpectedly", id
                                    ));
                            }
                        }
                    }

                    // new ticker received
                    _ = ticker.tick() => {
                        tracing::trace!("schedule task {} start sending notification", id);

                        // clone once for move between thread
                        let text = Arc::new(self.pending_notification[0].to_owned());
                        let buttons = self.msg_buttons.as_ref().unwrap();

                        for gid in self.groups.iter() {
                            let bot = bot.clone();
                            let text = text.clone();
                            let gid = gid.0;
                            let group_id = ChatId(gid);
                            tracing::trace!("Going to send {:?} to {:?}", text, gid);
                            bot.send_message(group_id, text.as_str())
                                .reply_markup(buttons.clone())
                                .await?;
                        }

                        // wait for all send message done for their jobs
                    }
                }
            }
        });

        TaskInfo {
            interval: self.interval,
            content: skim,
            sig: ShutdownSig(Arc::new(self.signal)),
            editor: Editor(self.editor.clone()),
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
