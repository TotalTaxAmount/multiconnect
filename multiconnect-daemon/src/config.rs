use std::path::PathBuf;

use lazy_static::lazy_static;

lazy_static! {
  pub static ref CONFIG: ConfigManager = ConfigManager::new();
}

pub struct ConfigManager {
  config_path: PathBuf
}

impl ConfigManager {
  fn new() -> Self {
    Self {
      config_path: dirs::config_dir().unwrap().join("multiconnect")
    }
  }

  pub fn get_config_dir(&self) -> &PathBuf {
    &self.config_path
  }
 }