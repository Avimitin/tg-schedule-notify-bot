use crate::BotRuntime;
use anyhow::Result;
use teloxide::{
    dispatching::{
        dialogue::{self, InMemStorage},
        UpdateHandler,
    },
    prelude::*,
    types::UserId,
    utils::command::BotCommands,
};

#[derive(Clone)]
pub enum AddTaskDialogueCurrentState {
    None,
    RequestNotifyText,
    RequestRepeatInterval {
        text: String,
    },
    RequestButtons {
        text: String,
        interval: u64,
    },
    RequestConfirmation {
        text: String,
        interval: u64,
        buttons: Vec<String>,
    },
}

pub type AddTaskDialogue =
    Dialogue<AddTaskDialogueCurrentState, InMemStorage<AddTaskDialogueCurrentState>>;

impl Default for AddTaskDialogueCurrentState {
    fn default() -> Self {
        Self::None
    }
}

/// Build the bot message handle logic
pub fn handler_schema() -> UpdateHandler<anyhow::Error> {
    let command_handler = teloxide::filter_command::<Command, _>().branch(
        dptree::case![AddTaskDialogueCurrentState::None]
            .branch(dptree::case![Command::Help].endpoint(help))
            .branch(dptree::case![Command::Start].endpoint(help))
            .branch(dptree::case![Command::AddTask].endpoint(add_task_handler))
            .branch(dptree::case![Command::ListTask].endpoint(list_task_handler))
            .branch(dptree::case![Command::DelTask].endpoint(del_task_handler)),
    );

    let message_handler = Update::filter_message().branch(
        // basic auth
        dptree::filter(|msg: Message, rt: BotRuntime| {
            let id = match msg.from() {
                Some(user) => user.id,
                None => return false,
            };
            has_access(&msg, id, &rt)
        })
        .branch(
            command_handler.branch(
                dptree::case![AddTaskDialogueCurrentState::RequestNotifyText]
                    .endpoint(request_notify_text),
            ),
        ),
    );

    dialogue::enter::<
        Update,
        InMemStorage<AddTaskDialogueCurrentState>,
        AddTaskDialogueCurrentState,
        _,
    >()
    .branch(message_handler)
}

async fn help(msg: Message, bot: AutoSend<Bot>) -> Result<()> {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn request_notify_text(
    msg: Message,
    bot: AutoSend<Bot>,
    dialogue: AddTaskDialogue,
) -> Result<()> {
    match msg.text() {
        Some(notify) => {
            bot.send_message(
                msg.chat.id,
                "请发送时间间隔，只需要数字即可。（单位：分钟）",
            )
            .await?;
            dialogue
                .update(AddTaskDialogueCurrentState::RequestRepeatInterval {
                    text: notify.to_string(),
                })
                .await?;
        }
        None => {
            bot.send_message(msg.chat.id, "请发送通知的文本").await?;
        }
    }

    Ok(())
}

#[derive(BotCommands, Debug, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "Display this text")]
    Help,
    #[command(description = "Start")]
    Start,
    #[command(
        description = "添加一个新的播报任务。例子：/addtask 30 （添加一个 30s 播报一次的任务）"
    )]
    AddTask,
    #[command(description = "列出当前所有的播报任务")]
    ListTask,
    #[command(description = "删除指定的任务。例子：/deltask 1 （删除编号 1 的任务）")]
    DelTask,
}

fn has_access(msg: &Message, id: UserId, rt: &BotRuntime) -> bool {
    let whitelist = rt.whitelist.read();
    // if it is in chat, and it is maintainer/admin calling
    msg.chat.is_private() && whitelist.has_access(id)
}

async fn add_task_handler(
    msg: Message,
    bot: AutoSend<Bot>,
    dialogue: AddTaskDialogue,
    mut rt: BotRuntime,
) -> Result<()> {
    tracing::info!("User {} add new schedule task", msg.from().unwrap().id);
    rt.task_pool.add_task(1, rt.get_groups(), "".to_string());
    dialogue
        .update(AddTaskDialogueCurrentState::RequestNotifyText)
        .await?;
    bot.send_message(
        msg.chat.id,
        format!("你已经添加了每隔 {} 秒播报一次的任务。", 1),
    )
    .await?;

    Ok(())
}

async fn list_task_handler(msg: Message, bot: AutoSend<Bot>, rt: BotRuntime) -> Result<()> {
    let task = rt.task_pool.list_task();

    let text = format!("总共 {} 个任务\n", task.len());
    let text = task.iter().fold(text, |acc, x| {
        format!("{}任务 {}，循环周期：{} 秒\n", acc, x.0, x.1)
    });
    bot.send_message(msg.chat.id, text).await?;

    Ok(())
}

async fn del_task_handler(msg: Message, bot: AutoSend<Bot>, mut rt: BotRuntime) -> Result<()> {
    bot.send_message(msg.chat.id, "正在删除任务").await?;
    let text = msg.text().ok_or_else(|| anyhow::anyhow!("非法字符！"))?;
    let args = text.split(' ').skip(1).collect::<Vec<&str>>();
    if args.is_empty() {
        anyhow::bail!("需要 id 才能删除任务！，你可以用 /listtask 查看任务 id");
    }
    let id = args[0];
    let id = match id.parse::<usize>() {
        Ok(i) => i,
        Err(e) => {
            bot.send_message(msg.chat.id, format! {"{id} 不是一个合法的数字！"})
                .await?;
            anyhow::bail!("parsing {id}: {e}");
        }
    };
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
    Ok(())
}
