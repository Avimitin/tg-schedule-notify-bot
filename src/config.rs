use std::{env::var, fmt::Debug, str::FromStr};

pub struct Config {
  pub maintainers: Vec<u64>,
  pub admins: Vec<u64>,
  pub groups: Vec<i64>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      maintainers: Vec::new(),
      admins: Vec::new(),
      groups: Vec::new(),
    }
  }
}

impl Config {
  pub fn new() -> Self {
    Self::default()
  }

  fn env_to_num_collect<T: FromStr>(k: &str) -> Option<Vec<T>>
  where
    <T as FromStr>::Err: Debug,
  {
    if let Ok(val) = var(k) {
      let val = val
        .split(',')
        .map(|x| {
          x.trim()
            .parse::<T>()
            .expect(format!("{x} is not a valid number").as_str())
        })
        .collect::<Vec<T>>();
      Some(val)
    } else {
      None
    }
  }

  // Expect: `export NOTIFY_BOT_MAINTAINERS="123,456,789"`
  pub fn parse_maintainers(mut self) -> Self {
    if let Some(m) = Self::env_to_num_collect("NOTIFY_BOT_MAINTAINERS") {
      self.maintainers = m
    }

    self
  }

  pub fn parse_admins(mut self) -> Self {
    if let Some(a) = Self::env_to_num_collect("NOTIFY_BOT_ADMINS") {
      self.admins = a
    }
    self
  }

  pub fn parse_groups(mut self) -> Self {
    if let Some(g) = Self::env_to_num_collect("NOTIFY_BOT_GROUPS") {
      self.groups = g
    }
    self
  }
}
