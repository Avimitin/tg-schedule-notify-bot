use crate::{schedule::ScheduleTask, BotRuntime};
use anyhow::Result;
use regex::Regex;
use teloxide::{
  dispatching::{
    dialogue::{self, InMemStorage},
    UpdateFilterExt, UpdateHandler,
  },
  payloads::SendMessageSetters,
  prelude::*,
  types::{InlineKeyboardButton, InlineKeyboardMarkup},
  utils::command::BotCommands,
};

lazy_static::lazy_static!(
    /// Parse button content. Expect `\w+|http://link`
    /// url::Url::parse will validate the http link, so we don't need to do this here
    static ref BUT_CONTENT_REGEX: Regex = Regex::new(
        r"(\w+)\s*?\|\s*?([^\s]+)"
    ).unwrap();
    /// Parse buttons. Expect `[Any character]`
    static ref BUT_PARSER: Regex = Regex::new(
        r"\[([^\[\]]*)\]"
    ).unwrap();
);

/// parse_button can parse multiple button and extract their context into a vector
fn parse_button(text: &str) -> Option<Vec<String>> {
  let mut v = Vec::with_capacity(4);
  for cap in BUT_PARSER.captures_iter(text) {
    v.push(cap[1].to_string());
  }
  if v.is_empty() {
    None
  } else {
    Some(v)
  }
}

/// parse_button_content parse *single* button context to two part.
/// This function is used to split button contents, it is not used for parsing the button.
/// So call `parse_button` for the raw button definition.
/// This function will also validate URL correctness.
/// It can split word|http link, word |http link, word | http link.
/// Return `None` if the button text is not construct with normal word character,
/// or link is not a valid URL.
fn parse_button_content(text: &str) -> Option<(String, url::Url)> {
  // early return `None` if no match found
  let cap = BUT_CONTENT_REGEX.captures(text)?;
  let url = url::Url::parse(cap.get(2)?.as_str()).ok()?;
  // early return `None` when any capture group doesn't matched
  Some((cap.get(1)?.as_str().to_string(), url))
}

#[test]
fn parse_button_text() {
  let text = "[按钮1|示例文本]";
  let result = parse_button(text);
  assert!(result.is_some());
  let result = result.unwrap();
  for r in result {
    assert_eq!(r, "按钮1|示例文本");
  }

  let text = "[按钮1|示例文本][按钮2|参考文本]  [button3|example text]";
  let result = parse_button(text);
  assert!(result.is_some());
  let result = result.unwrap();
  assert_eq!(
    result,
    vec!["按钮1|示例文本", "按钮2|参考文本", "button3|example text"]
  );
}

#[test]
fn parse_button_content_test() {
  // test invalid URL
  let text = "按钮1|示例文本";
  assert_eq!(parse_button_content(text), None);

  // test invalid text
  let text = "  |https://github.com";
  assert_eq!(parse_button_content(text), None);

  // test correct format
  let text = "Button|https://example.com";
  assert_eq!(
    parse_button_content(text),
    Some((
      "Button".to_string(),
      url::Url::parse("https://example.com").unwrap()
    ))
  );

  // test Chinese text
  let text = "按钮|https://example.com";
  assert_eq!(
    parse_button_content(text),
    Some((
      "按钮".to_string(),
      url::Url::parse("https://example.com").unwrap()
    ))
  );
}

#[derive(Clone)]
/// AddTaskDialogueCurrentState describe current add task dialogue progress.
pub enum AddTaskDialogueCurrentState {
  /// None describe that there is no add task dialogue
  None,
  /// RequestNotifyText describe that current status bot require notification text
  RequestNotifyText,
  /// RequestRepeatInterval describe that in curret status, bot require notification
  /// repeat interval settings
  RequestRepeatInterval { text: String },
  /// RequestButtons describe that in current status, bot require button definition.
  RequestButtons { text: String, interval: u64 },
  /// RequestConfirmation describe that in current status, bot require final result confirmation.
  RequestConfirmation {
    text: String,
    interval: u64,
    buttons: InlineKeyboardMarkup,
  },
}

impl Default for AddTaskDialogueCurrentState {
  fn default() -> Self {
    Self::None
  }
}

/// An alias type for shorthand, nothing special
pub type AddTaskDialogue =
  Dialogue<AddTaskDialogueCurrentState, InMemStorage<AddTaskDialogueCurrentState>>;

/// Handler for AddTaskDialogueCurrentState::RequestNotifyText status
/// request_notify_text receive notification text, store in memory, and change status
/// to AddTaskDialogueCurrentState::RequestRepeatInterval.
async fn request_notify_text(
  msg: Message,
  bot: AutoSend<Bot>,
  dialogue: AddTaskDialogue,
) -> Result<()> {
  match msg.text() {
    Some(notify) => {
      bot
        .send_message(
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

/// Handler for AddTaskDialogueCurrentState::RequestRepeatInterval status
/// It parse interval to u64, then update status to RequestButtons.
async fn request_repeat_interval(
  msg: Message,
  bot: AutoSend<Bot>,
  dialogue: AddTaskDialogue,
  text: String,
) -> Result<()> {
  match msg.text().map(|t| t.parse::<u64>()) {
    Some(Ok(interval)) => {
      bot
        .send_message(
          msg.chat.id,
          format!("bot 将会毎 {interval} 分钟发送一次：\n\n{text}"),
        )
        .await?;

      bot
        .send_message(
          msg.chat.id,
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
          .to_string(),
        )
        .await?;
      dialogue
        .update(AddTaskDialogueCurrentState::RequestButtons { text, interval })
        .await?;
    }
    _ => {
      bot
        .send_message(msg.chat.id, "非法输入！请只输入数字")
        .await?;
    }
  }
  Ok(())
}

/// Handler for AddTaskDialogueCurrentState::RequestButtons status
/// It parse input to buttons, then update status to RequestConfirmation.
async fn request_buttons(
  msg: Message,
  bot: AutoSend<Bot>,
  dialogue: AddTaskDialogue,
  (text, interval): (String, u64),
) -> Result<()> {
  if msg.text().is_none() {
    bot
      .send_message(msg.chat.id, "bot 需要文字消息！请重新输入！")
      .await?;
    anyhow::bail!("invalid message text for parsing buttons");
  }

  let msg_text = msg.text().unwrap();
  // the final result
  let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];
  // parse buttons line by line
  for line in msg_text.lines() {
    // create a row of button collection
    let mut row = Vec::new();
    // first parse the buttons in current line
    let buttons = parse_button(line);
    if buttons.is_none() {
      bot
        .send_message(msg.chat.id, "错误的链接定义！请参照上面的格式重新输入！")
        .await?;
      anyhow::bail!("invalid button definition: {}", line);
    }
    let buttons = buttons.unwrap();
    // then parse the contents inside of the buttons definition
    for but in buttons {
      let pair = parse_button_content(&but);
      if pair.is_none() {
        bot
          .send_message(msg.chat.id, "按钮的内容定义有问题！请重新输入！")
          .await?;
        anyhow::bail!("invalid button contents: {}", but);
      }
      let pair = pair.unwrap();
      // finally create a new button and push into row
      row.push(InlineKeyboardButton::url(pair.0, pair.1));
    }
    // push the new row into final results
    keyboard.push(row);
  }

  let buttons = InlineKeyboardMarkup::new(keyboard);

  bot
    .send_message(msg.chat.id, text.to_string())
    .reply_markup(buttons.clone())
    .await?;

  bot
    .send_message(
      msg.chat.id,
      format!("上面的信息将会每隔 {interval} 分钟重复一次。\n请确认添加这个新的通知："),
    )
    .reply_markup(create_add_task_confirm_buttons())
    .await?;

  dialogue
    .update(AddTaskDialogueCurrentState::RequestConfirmation {
      text,
      interval,
      buttons,
    })
    .await?;

  Ok(())
}

/// Create a InlineKeyboardMarkup for confirmation. Callback data is prefixed
/// by `add_task_confirm_`. Suffix `y` means confirm, `n` means cancel.
fn create_add_task_confirm_buttons() -> InlineKeyboardMarkup {
  let buttons = vec![vec![
    InlineKeyboardButton::callback("确认", "add_task_confirm_y"),
    InlineKeyboardButton::callback("取消", "add_task_confirm_n"),
  ]];
  InlineKeyboardMarkup::new(buttons)
}

/// Callback handler for buttons CallbackQuery.
async fn button_callback_handler(
  q: CallbackQuery,
  bot: AutoSend<Bot>,
  dialogue: AddTaskDialogue,
  mut rt: BotRuntime,
  (text, interval, buttons): (String, u64, InlineKeyboardMarkup),
) -> Result<()> {
  // we might create some empty button for dressing
  if q.data.is_none() {
    return Ok(());
  }

  let data = q.data.unwrap();
  let chat_id = q
    .message
    .ok_or_else(|| anyhow::anyhow!("A button callback without message can't be handle"))?
    .chat
    .id;

  match data.as_str() {
    "add_task_confirm_y" => {
      let task = ScheduleTask::new()
        .interval(interval)
        .pending_notification(vec![text])
        .groups(rt.get_group().to_vec())
        .msg_buttons(buttons);
      rt.task_pool.add_task(task);
      bot.send_message(chat_id, "你已提交了任务！").await?;
      dialogue.exit().await?;
    }
    "add_task_confirm_n" => {
      bot.send_message(chat_id, "你已取消了任务！").await?;
      dialogue.exit().await?;
    }
    _ => {}
  }

  Ok(())
}

#[derive(BotCommands, Debug, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
  #[command(description = "显示这条帮助消息")]
  Help,
  #[command(description = "显示这条帮助消息")]
  Start,
  #[command(description = "添加一个新的播报任务。")]
  AddTask,
  #[command(description = "列出当前所有的播报任务")]
  ListTask,
  #[command(description = "删除指定的任务。")]
  DelTask,
  #[command(description = "添加一个新的 bot 管理员（维护者专用）")]
  AddAdmin,
  #[command(description = "删除 bot 管理员（维护者专用）")]
  DelAdmin,
  #[command(description = "添加一个新的通知群")]
  AddGroup,
  #[command(description = "删除通知群")]
  DelGroup,
}

/// Response command man page
async fn help(msg: Message, bot: AutoSend<Bot>) -> Result<()> {
  bot
    .send_message(msg.chat.id, Command::descriptions().to_string())
    .await?;
  Ok(())
}

/// Handler for adding task command. This start the add task dialogue, and change
/// AddTaskDialogueCurrentState to RequestNotifyText.
async fn add_task_handler(
  msg: Message,
  bot: AutoSend<Bot>,
  dialogue: AddTaskDialogue,
) -> Result<()> {
  tracing::info!(
    "User {} try adding new schedule task",
    msg.from().unwrap().id
  );
  bot
    .send_message(
      msg.chat.id,
      "正在创建一个新的定时任务，请发送通知的内容：".to_string(),
    )
    .await?;
  dialogue
    .update(AddTaskDialogueCurrentState::RequestNotifyText)
    .await?;

  Ok(())
}

/// Handler for /listtask.
async fn list_task_handler(msg: Message, bot: AutoSend<Bot>, rt: BotRuntime) -> Result<()> {
  let task = rt.task_pool.list_task();

  let text = format!("总共 {} 个任务\n", task.len());
  let text = task.iter().fold(text, |acc, x| {
    let id = x.0;
    let inv = x.1;
    let content = &x.2;
    format!(
      "{acc}任务 {id}，循环周期：{inv} 秒，任务内容：{content}\n{}\n\n",
      "=".repeat(35)
    )
  });
  bot.send_message(msg.chat.id, text).await?;

  Ok(())
}

/// Handler for /deltask command.
async fn del_task_handler(msg: Message, bot: AutoSend<Bot>, mut rt: BotRuntime) -> Result<()> {
  bot.send_message(msg.chat.id, "正在删除任务").await?;
  let text = msg.text().ok_or_else(|| anyhow::anyhow!("非法字符！"))?;
  let args = text.split(' ').skip(1).collect::<Vec<&str>>();
  if args.is_empty() {
    let reply = "需要 id 才能删除任务！，你可以用 /listtask 查看任务 id";
    bot.send_message(msg.chat.id, reply).await?;
    anyhow::bail!("No task id specify");
  }
  let id = args[0];
  let id = match id.parse::<u32>() {
    Ok(i) => i,
    Err(e) => {
      bot
        .send_message(msg.chat.id, format! {"{id} 不是一个合法的数字！"})
        .await?;
      anyhow::bail!("parsing {id}: {e}");
    }
  };
  match rt.task_pool.remove(id).await {
    Ok(_) => {
      bot.send_message(msg.chat.id, "删除成功").await?;
    }
    Err(e) => {
      bot
        .send_message(
          msg.chat.id,
          format!("删除失败：{}，请用 /listtask 确认任务存在。", e),
        )
        .await?;
    }
  }
  Ok(())
}

async fn add_admin(msg: Message, bot: AutoSend<Bot>, mut rt: BotRuntime) -> Result<()> {
  let text = msg.text().ok_or_else(|| anyhow::anyhow!("非法字符！"))?;
  let args = text.split(' ').skip(1).collect::<Vec<&str>>();
  if args.is_empty() {
    let reply = "需要用户 ID 才能添加管理员！";
    bot.send_message(msg.chat.id, reply).await?;
    anyhow::bail!("No task id specify");
  }

  let id = args[0];
  let id = match id.parse::<u64>() {
    Ok(i) => i,
    Err(e) => {
      bot
        .send_message(msg.chat.id, format! {"{id} 不是一个合法的数字！"})
        .await?;
      anyhow::bail!("parsing {id}: {e}");
    }
  };

  rt.add_admin(id);
  let msg = bot
    .send_message(msg.chat.id, "添加完成，正在保存...")
    .await?;
  rt.save_whitelist().await?;
  bot
    .edit_message_text(msg.chat.id, msg.id, "保存完成。")
    .await?;

  Ok(())
}

async fn del_admin(msg: Message, bot: AutoSend<Bot>, mut rt: BotRuntime) -> Result<()> {
  let text = msg.text().ok_or_else(|| anyhow::anyhow!("非法字符！"))?;
  let args = text.split(' ').skip(1).collect::<Vec<&str>>();
  if args.is_empty() {
    let reply = "需要用户 ID 才能删除管理员！";
    bot.send_message(msg.chat.id, reply).await?;
    anyhow::bail!("No task id specify");
  }

  let id = args[0];
  let id = match id.parse::<u64>() {
    Ok(i) => i,
    Err(e) => {
      bot
        .send_message(msg.chat.id, format! {"{id} 不是一个合法的数字！"})
        .await?;
      anyhow::bail!("parsing {id}: {e}");
    }
  };

  if let Err(e) = rt.del_admin(id) {
    bot
      .send_message(msg.chat.id, "用户不存在！请重新确认 id")
      .await?;
    anyhow::bail!("fail to delete user: {e}")
  };

  let msg = bot
    .send_message(msg.chat.id, "删除完成，正在保存...")
    .await?;
  rt.save_whitelist().await?;
  bot
    .edit_message_text(msg.chat.id, msg.id, "保存完成。")
    .await?;

  Ok(())
}

/// Build the bot message handle logic
pub fn handler_schema() -> UpdateHandler<anyhow::Error> {
  let can_process_admin = |msg: &Message, rt: &BotRuntime| -> bool {
    let id = match msg.from() {
      Some(user) => user.id,
      None => return false,
    };
    let whitelist = rt.whitelist.read();
    whitelist.is_maintainers(id.0)
  };

  // build the command handler
  let command_handler = teloxide::filter_command::<Command, _>().branch(
    dptree::case![AddTaskDialogueCurrentState::None]
      // admins accessible commands
      .branch(dptree::case![Command::Help].endpoint(help))
      .branch(dptree::case![Command::Start].endpoint(help))
      .branch(dptree::case![Command::AddTask].endpoint(add_task_handler))
      .branch(dptree::case![Command::ListTask].endpoint(list_task_handler))
      .branch(dptree::case![Command::DelTask].endpoint(del_task_handler))
      .branch(
        // Maintainer only commands
        dptree::filter(move |msg: Message, rt: BotRuntime| can_process_admin(&msg, &rt))
          .branch(dptree::case![Command::AddAdmin].endpoint(add_admin))
          .branch(dptree::case![Command::DelAdmin].endpoint(del_admin)),
      ),
  );

  // test if the user has access to the bot
  let has_access = |msg: &Message, rt: &BotRuntime| -> bool {
    let id = match msg.from() {
      Some(user) => user.id,
      None => return false,
    };
    let whitelist = rt.whitelist.read();
    // if it is in chat, and it is maintainer/admin calling
    msg.chat.is_private() && whitelist.has_access(id)
  };

  // build the text message handler
  let message_handler = Update::filter_message().branch(
    // basic auth
    dptree::filter(move |msg: Message, rt: BotRuntime| has_access(&msg, &rt))
      // enter command filter
      .branch(command_handler)
      // handle non command message
      .branch(
        dptree::case![AddTaskDialogueCurrentState::RequestNotifyText].endpoint(request_notify_text),
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

  // build the callback handler
  let callback_handler = Update::filter_callback_query().branch(
    dptree::case![AddTaskDialogueCurrentState::RequestConfirmation {
      text,
      interval,
      buttons
    }]
    .endpoint(button_callback_handler),
  );

  /*
   * Update --> <IsMessage> --> message_handler --> <IsCommand> --> command_handler
   *     \                                    \
   *      \                                   * --> normal_message_handler
   *       \
   *        *--> <IsCallbackQuery> --> query_handler
   */
  let root = dptree::entry()
    .branch(message_handler)
    .branch(callback_handler);

  dialogue::enter::<
        Update,
        InMemStorage<AddTaskDialogueCurrentState>,
        AddTaskDialogueCurrentState,
        _,
    >()
    .branch(root)
}
