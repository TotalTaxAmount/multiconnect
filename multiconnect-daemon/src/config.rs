use std::{
  fs::{self, File},
  path::PathBuf,
};

use lazy_static::lazy_static;
use log::error;

lazy_static! {
  pub static ref CONFIG: ConfigManager = ConfigManager::new();
}

pub struct ConfigManager {
  config_path: PathBuf,
}

impl ConfigManager {
  fn new() -> Self {
    let path = dirs::config_dir().unwrap().join("multiconnect");

    if let Err(e) = fs::create_dir_all(&path) {
      error!("Failed to create config dir");
    }

    Self { config_path: path }
  }

  pub fn get_config_dir(&self) -> &PathBuf {
    &self.config_path
  }
}
