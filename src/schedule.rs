use anyhow::Result;
use std::time::Duration;
use teloxide::{prelude::*, types::ChatId};
use tokio::time::interval;
use tracing::error;
use crate::BotRuntime;

/// scheduled_notifier send message periodly. It will precisely wait for `secs` second.
pub async fn scheduled_notifier(bot: AutoSend<Bot>, rt: BotRuntime, secs: u64) -> Result<()> {
    let mut heartbeat = interval(Duration::from_secs(secs));
    let mut rx = rt.shutdown_sig.subscribe();

    loop {
        tokio::select! {
            _ = rx.recv() => {
                // receive shutdown signal
                return Ok(())
            }
            _ = heartbeat.tick() => {
                let mut wg = Vec::new();

                let groups = rt.get_group();
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
}
