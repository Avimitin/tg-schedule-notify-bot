use crate::BotRuntime;
use anyhow::Result;
use teloxide::{prelude::*, types::UserId, utils::command::BotCommands};

pub async fn message_handler(msg: Message, bot: AutoSend<Bot>, rt: BotRuntime) -> Result<()> {
    // convert Option<User> to Result<User, Error>
    let sender = msg
        .from()
        .ok_or_else(|| {
            // some type of messages that came from telegram, like new member, or member leave
            anyhow::anyhow!("A system message, abandoned")
        })?
        .id;
    if !has_access(&msg, sender, &rt) {
        anyhow::bail!("No access")
    }

    if msg.text().is_some() {
        command_handler(msg.clone(), bot, &mut rt.clone()).await?;
    }

    Ok(())
}

#[derive(BotCommands, Debug)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "Display this text")]
    Help,
    #[command(description = "Start")]
    Start,
    #[command(
        description = "添加一个新的播报任务。例子：/addtask 30 （添加一个 30s 播报一次的任务）",
        parse_with = "split"
    )]
    AddTask { secs: u64, notification: String },
    #[command(description = "列出当前所有的播报任务")]
    ListTask,
    #[command(
        description = "删除指定的任务。例子：/deltask 1 （删除编号 1 的任务）",
        parse_with = "split"
    )]
    DelTask { id: usize },
}

async fn command_handler(msg: Message, bot: AutoSend<Bot>, rt: &mut BotRuntime) -> Result<()> {
    let text = msg.text().unwrap();

    let command = BotCommands::parse(text, rt.username());

    if command.is_err() {
        // NOTE: maybe need to handle normal text message here
        return Ok(());
    }

    let command = command.unwrap();
    tracing::info!(
        "User {} using command: {:?}",
        msg.from().unwrap().id,
        command
    );
    match command {
        Command::Help | Command::Start => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::AddTask { secs, notification } => {
            tracing::info!("User {} add new schedule task", msg.from().unwrap().id);
            rt.task_pool.add_task(secs, rt.get_groups(), notification);
            bot.send_message(
                msg.chat.id,
                format!("你已经添加了每隔 {} 秒播报一次的任务。", secs),
            )
            .await?;
        }
        Command::ListTask => {
            let task = rt.task_pool.list_task();

            let text = format!("总共 {} 个任务\n", task.len());
            let text = task.iter().fold(text, |acc, x| {
                format!("{}任务 {}，循环周期：{} 秒\n", acc, x.0, x.1)
            });
            bot.send_message(msg.chat.id, text).await?;
        }
        Command::DelTask { id } => {
            bot.send_message(msg.chat.id, "正在删除任务").await?;
            match rt.task_pool.remove(id) {
                Ok(_) => {
                    bot.send_message(msg.chat.id, "删除成功").await?;
                }
                Err(e) => {
                    bot.send_message(
                        msg.chat.id,
                        format!("删除失败：{}，请用 /listtask 确认任务存在。", e),
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

fn has_access(msg: &Message, id: UserId, rt: &BotRuntime) -> bool {
    let whitelist = rt.whitelist.read();
    // if it is in chat, and it is maintainer/admin calling
    msg.chat.is_private() && whitelist.has_access(id)
}
