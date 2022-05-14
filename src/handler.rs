use crate::BotRuntime;
use anyhow::Result;
use regex::Regex;
use teloxide::{
    dispatching::{
        dialogue::{self, InMemStorage},
        UpdateHandler,
    },
    prelude::*,
    types::UserId,
    utils::command::BotCommands,
};

lazy_static::lazy_static!(
    static ref BUT_CONTENT_REGEX: Regex = Regex::new(
        r"(\w+)\s*?:\s*?(http[s]?://(?:[a-zA-Z]|[0-9]|[$-_@.&+]|[!*\(\),]|(?:%[0-9a-fA-F][0-9a-fA-F]))+)"
    ).unwrap();
    static ref BUT_PARSER: Regex = Regex::new(
        r"\[([^\[\]]*)\]"
    ).unwrap();
);

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
            // Update next status to interval request
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

async fn request_repeat_interval(
    msg: Message,
    bot: AutoSend<Bot>,
    dialogue: AddTaskDialogue,
    text: String,
) -> Result<()> {
    match msg.text().map(|t| t.parse::<u64>()) {
        Some(Ok(interval)) => {
            bot.send_message(
                msg.chat.id,
                format!("bot 将会毎 {interval} 分钟发送一次：\n\n{text}"),
            )
            .await?;

            bot.send_message(
                msg.chat.id,
                format!(
                    "接下来请你输入附带在定时通知上的按钮信息:
=================================
格式: [按钮文本|链接] （这里是半角的括号）
示例：[注册|https://example.com]
如果需要给按钮分不同的行，只需要在新的一行重现写按钮就行：
示例：
[注册|https://example.com/register] [登录|https://example.com/login]
[下载|https://example.com/download] [反馈|https://example.com/feedback]
=================================
"
                ),
            )
            .await?;
            dialogue
                .update(AddTaskDialogueCurrentState::RequestButtons { text, interval })
                .await?;
        }
        _ => {
            bot.send_message(msg.chat.id, "非法输入！请只输入数字")
                .await?;
        }
    }
    Ok(())
}

async fn request_buttons(
    msg: Message,
    bot: AutoSend<Bot>,
    dialogue: AddTaskDialogue,
    (text, interval): (String, u64),
) -> Result<()> {
    if msg.text().is_none() {
        bot.send_message(msg.chat.id, "bot 需要文字消息！请重新输入！")
            .await?;
        anyhow::bail!("invalid message text for parsing buttons");
    }

    let msg_text = msg.text().unwrap();
    for line in msg_text.lines() {
        tracing::info!("{line}");
    }

    dialogue.exit().await?;

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

async fn add_task_handler(
    msg: Message,
    bot: AutoSend<Bot>,
    dialogue: AddTaskDialogue,
) -> Result<()> {
    tracing::info!(
        "User {} try adding new schedule task",
        msg.from().unwrap().id
    );
    bot.send_message(
        msg.chat.id,
        format!("正在创建一个新的定时任务，请发送通知的内容："),
    )
    .await?;
    dialogue
        .update(AddTaskDialogueCurrentState::RequestNotifyText)
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

    // test if the user has access to the bot
    let has_access = |msg: &Message, id: UserId, rt: &BotRuntime| -> bool {
        let whitelist = rt.whitelist.read();
        // if it is in chat, and it is maintainer/admin calling
        msg.chat.is_private() && whitelist.has_access(id)
    };

    let message_handler = Update::filter_message().branch(
        // basic auth
        dptree::filter(move |msg: Message, rt: BotRuntime| {
            let id = match msg.from() {
                Some(user) => user.id,
                None => return false,
            };
            has_access(&msg, id, &rt)
        })
        .branch(command_handler)
        .branch(
            dptree::case![AddTaskDialogueCurrentState::RequestNotifyText]
                .endpoint(request_notify_text),
        )
        .branch(
            dptree::case![AddTaskDialogueCurrentState::RequestRepeatInterval { text }]
                .endpoint(request_repeat_interval),
        )
        .branch(
            dptree::case![AddTaskDialogueCurrentState::RequestButtons { text, interval }]
                .endpoint(request_buttons),
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
