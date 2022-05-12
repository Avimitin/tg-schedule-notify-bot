use crate::BotRuntime;
use anyhow::Result;
use teloxide::types::ChatId;
use teloxide::{prelude::*, types::UserId, utils::command::BotCommands};

pub async fn message_handler(msg: Message, bot: AutoSend<Bot>, rt: BotRuntime) -> Result<()> {
    let sender = msg.from().unwrap().id;
    if !has_access(msg.clone(), sender, rt.clone()) {
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
    #[command(description = "Create a new admin")]
    Grant,
    #[command(description = "Add message into pending queue")]
    Add,
    #[command(description = "List pending messages")]
    List,
    #[command(description = "Remove message")]
    Remove,
    #[command(description = "Clean the whole message queue")]
    Clean,
    #[command(
        description = "添加一个新的播报任务。例子：/addtask 30 （添加一个 30s 播报一次的任务）",
        parse_with = "split"
    )]
    AddTask{ secs: u64 },
}

async fn command_handler(msg: Message, bot: AutoSend<Bot>, rt: &mut BotRuntime) -> Result<()> {
    let text = msg.text().unwrap();

    let command = BotCommands::parse(text, rt.username());

    if command.is_err() {
        return text_handler(msg.clone(), bot.clone(), rt).await;
    }

    let command = command.unwrap();
    tracing::info!("User {} using command: {:?}", msg.from().unwrap().id, command);
    match command {
        Command::Help | Command::Start => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::Grant => {}
        Command::Add => {}
        Command::List => {}
        Command::Remove => {}
        Command::Clean => {}
        Command::AddTask{ secs } => {
            tracing::info!("User {} add new schedule task", msg.from().unwrap().id);
            rt.add_schedule_task(secs)
        }
    }

    Ok(())
}

async fn text_handler(msg: Message, bot: AutoSend<Bot>, rt: &mut BotRuntime) -> Result<()> {
    Ok(())
}

fn has_access(msg: Message, id: UserId, rt: BotRuntime) -> bool {
    let whitelist = rt.whitelist.read();
    // if it is in chat, and it is maintainer/admin calling
    msg.chat.is_private() && whitelist.has_access(id)
}
