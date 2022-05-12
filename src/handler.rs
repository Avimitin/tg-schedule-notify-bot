use anyhow::Result;

use teloxide::{prelude::*, types::UserId};

use crate::BotRuntime;

pub async fn message_handler(msg: Message, bot: AutoSend<Bot>, rt: BotRuntime) -> Result<()> {
    let sender = msg.from().unwrap().id;
    if !has_access(msg, sender, rt) {
        anyhow::bail!("No access")
    }

    Ok(())
}

fn has_access(msg: Message, id: UserId, rt: BotRuntime) -> bool {
    let whitelist = rt.whitelist.read();
    // if it is in chat, and it is maintainer/admin calling
    msg.chat.is_private() && whitelist.has_access(id)
}
