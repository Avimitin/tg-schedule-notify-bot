use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use teloxide::{prelude::*, types::ChatId};
use tokio::sync::watch;
use tokio::time as tok_time;
use tracing::{ error, trace };

#[derive(Clone)]
pub struct TaskPool {
    pool: Arc<RwLock<Vec<ScheduleTask>>>,
}

pub struct ScheduleTask {
    id: usize,
    interval: u64,
    signal: tokio::sync::watch::Sender<u8>,
    handle: tokio::task::JoinHandle<Result<()>>,
}

impl ScheduleTask {
    pub fn new(id: usize, dur: Duration, groups: Vec<ChatId>, bot: AutoSend<Bot>) -> Self {
        let (tx, mut rx) = watch::channel(0);

        let handle = tokio::spawn(async move {
            let mut ticker = tok_time::interval(dur);
            let id = id;
            loop {
                tracing::info!("schedule task {} start sending notification", id);
                tokio::select! {
                    // receive shutdown signal
                    _ = rx.changed() => {
                        return Ok(())
                    }

                    // new ticker received
                    _ = ticker.tick() => {
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
            signal: tx,
            interval: dur.as_secs(),
            handle,
            id,
        }
    }

    pub fn stop(&self) {
        match self.signal.send(1) {
            Ok(_) => {}
            Err(e) => error!("fail to stop schedule task {}: {}", self.id, e),
        };
    }
}

impl TaskPool {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn add_task(&mut self, secs: u64, groups: Vec<ChatId>, bot: AutoSend<Bot>) {
        // lock the pool and write to it
        let mut pool = self.pool.write();
        let task = ScheduleTask::new(pool.len() + 1, Duration::from_secs(secs), groups, bot);
        pool.push(task);
    }

    pub fn list_task(&self) -> Vec<(usize, u64)> {
        let pool = self.pool.read();

        pool.iter().map(|x| (x.id, x.interval)).collect()
    }

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
